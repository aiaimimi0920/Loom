// Mount existing Art instances once per endpoint; repeated placements share this view.
impl WallSurfaceServices<'_> {
    fn open(
        &self,
        input: WallSurfaceOpen,
        actor: &str,
        links: &mut WallSurfaceLinks,
    ) -> WallSurfaceResult<(u16, String)> {
        let inputs = self
            .walls
            .surface_inputs(actor, &input.binding, &input.instance_id)?;
        let key = (input.binding.endpoint_id.clone(), input.instance_id.clone());
        if let Some(link) = links.get_mut(&key) {
            if link.device_id != actor {
                return Err(wall_route_unavailable());
            }
            if link.closing {
                return Err(WallStoreError::new(
                    409,
                    "wall_surface_closing",
                    "Art view is finishing accepted work",
                ));
            }
            if link.binding != input.binding {
                link.binding = input.binding;
                link.sequence = 0;
            }
            return self.state(link, None);
        }
        if links.len() >= 128
            || links
                .values()
                .filter(|link| link.binding.endpoint_id == input.binding.endpoint_id)
                .count()
                >= 4
        {
            return Err(WallStoreError::new(
                409,
                "wall_surface_capacity",
                "Surface view capacity reached",
            ));
        }
        let instance = self
            .instances
            .lock()
            .map_err(|_| wall_route_unavailable())?
            .get(&input.instance_id)
            .ok_or_else(|| {
                WallStoreError::new(404, "surface_not_found", "Art instance is unavailable")
            })?;
        let tool = loom_tool_registry::install::resolve_installed_art_package(
            self.root,
            &instance.descriptor.art_id,
            &instance.descriptor.art_version,
            &instance.descriptor.package_digest,
            self.tools,
            self.frameworks,
        )
        .map_err(|_| {
            WallStoreError::new(
                409,
                "surface_art_package_unavailable",
                "Art package is unavailable",
            )
        })?;
        let manifest = tool
            .surface_manifest()
            .map_err(|_| wall_route_unavailable())?
            .ok_or_else(|| {
                WallStoreError::new(
                    409,
                    "surface_manifest_missing",
                    "Art has no Surface manifest",
                )
            })?;
        let size = manifest
            .views
            .iter()
            .find(|view| Some(&view.id) == manifest.default_view_id.as_ref())
            .map(|view| view.full_size.clone())
            .or_else(|| {
                manifest
                    .minimum_size
                    .as_ref()
                    .map(|minimum| loom_protocol::SurfaceSize {
                        // Minimum size is a lower bound, not the preferred Art viewport.
                        width: minimum.width.max(800),
                        height: minimum.height.max(600),
                    })
            })
            .unwrap_or(loom_protocol::SurfaceSize {
                width: 800,
                height: 600,
            });
        if size.width == 0 || size.height == 0 || size.width > 4096 || size.height > 4096 {
            return Err(WallStoreError::new(
                409,
                "wall_surface_size_limit",
                "Art viewport exceeds terminal limits",
            ));
        }
        let mut capabilities = default_declarative_surface_host_capabilities();
        capabilities.transports.push("loom_resource".into());
        capabilities.capabilities.push("remote_resources".into());
        capabilities.input.pointer =
            inputs.contains(&loom_protocol::wall::TileInputCapability::Pointer);
        capabilities.input.hover = capabilities.input.pointer;
        capabilities.input.keyboard =
            inputs.contains(&loom_protocol::wall::TileInputCapability::Keyboard);
        capabilities.input.touch = false;
        let attachment = self
            .instances
            .lock()
            .map_err(|_| wall_route_unavailable())?
            .attach_ephemeral(
                &input.instance_id,
                &format!("wall:{}", input.binding.endpoint_id),
                actor,
                capabilities,
            )
            .map_err(wall_surface_store_error)?;
        let link = crate::wall_store::WallSurfaceLink {
            binding: input.binding,
            device_id: actor.to_owned(),
            instance_id: input.instance_id,
            attachment_id: attachment.descriptor.attachment_id,
            width: size.width,
            height: size.height,
            sequence: 0,
            accepted_requests: Default::default(),
            closing: false,
            cancelable_actions: manifest
                .actions
                .iter()
                .filter(|action| action.cancelable)
                .map(|action| action.id.clone())
                .collect(),
        };
        let body = json!({"attachmentId": link.attachment_id}).to_string();
        let mounted = mount_surface_instance(
            &link.instance_id,
            &body,
            self.instances,
            self.tools,
            self.frameworks,
            self.root,
            self.bridge,
            self.resources,
            self.shared_images,
        );
        match mounted {
            Ok((status, body)) if status != 200 => {
                release_wall_surface_link(
                    self.instances,
                    self.resources,
                    self.shared_images,
                    &link,
                )?;
                return Ok((status, body));
            }
            Err(_) => {
                release_wall_surface_link(
                    self.instances,
                    self.resources,
                    self.shared_images,
                    &link,
                )?;
                return Err(wall_route_unavailable());
            }
            _ => {}
        }
        let result = self.state(&link, None);
        if result.is_ok() {
            links.insert(key, link);
        } else {
            release_wall_surface_link(self.instances, self.resources, self.shared_images, &link)?;
        }
        result
    }

    fn state(
        &self,
        link: &crate::wall_store::WallSurfaceLink,
        revision: Option<u64>,
    ) -> WallSurfaceResult<(u16, String)> {
        let store = self
            .instances
            .lock()
            .map_err(|_| wall_route_unavailable())?;
        let instance = store.get(&link.instance_id).ok_or_else(|| {
            WallStoreError::new(404, "surface_not_found", "Art instance is unavailable")
        })?;
        let attachment = instance
            .attachments
            .get(&link.attachment_id)
            .ok_or_else(|| {
                WallStoreError::new(409, "wall_surface_detached", "Art view was removed")
            })?;
        let snapshot = attachment
            .snapshot
            .as_ref()
            .filter(|snapshot| snapshot.runtime == SurfaceRuntimeKind::Declarative)
            .ok_or_else(|| {
                WallStoreError::new(
                    409,
                    "wall_surface_runtime_unsupported",
                    "Terminal requires a declarative Art view",
                )
            })?;
        let snapshot = if revision == Some(snapshot.revision) {
            None
        } else {
            let mut snapshot = snapshot.clone();
            // Resources are fetched through the wall grant, never a transferable Surface lease.
            snapshot.resource_leases.clear();
            Some(snapshot)
        };
        let confirmations = instance
            .pending_confirmations
            .values()
            .filter(|pending| pending.request.attachment_id == link.attachment_id)
            .map(|pending| &pending.request)
            .collect::<Vec<_>>();
        let owned_acks = instance.ephemeral_event_acks(&link.attachment_id);
        let pending = link.accepted_requests.pending(&owned_acks);
        let failure = instance.last_failure.as_ref().filter(|failure| {
            link.accepted_requests.contains(&failure.request_id)
                && failure.generation == instance.descriptor.generation
                && owned_acks.values().any(|ack| ack.request_id == failure.request_id
                    && matches!(ack.status, loom_protocol::SurfaceActionStatus::Failed | loom_protocol::SurfaceActionStatus::Interrupted))
        }).map(|failure| json!({"requestId": failure.request_id, "code": "wall_surface_action_failed"}));
        wall_json(
            &json!({"protocolVersion": loom_protocol::wall::WALL_PROTOCOL_VERSION,
            "view": WallSurfaceView { binding: link.binding.clone(), instance_id: link.instance_id.clone(), attachment_id: link.attachment_id.clone() },
            "width": link.width, "height": link.height, "generation": instance.descriptor.generation, "sequence": link.sequence,
            "snapshot": snapshot, "preview": instance.latest_preview, "result": instance.latest_result,
            "confirmations": confirmations, "pending": pending, "failure": failure }),
        )
    }
}

fn release_wall_surface_link(
    instances: &SharedSurfaceInstanceStore,
    resources: &SharedSurfaceResourceStore,
    shared_images: &SharedImageStoreHandle,
    link: &crate::wall_store::WallSurfaceLink,
) -> WallSurfaceResult<bool> {
    let removed = {
        let mut store = instances.lock().map_err(|_| wall_route_unavailable())?;
        if store.get(&link.instance_id).is_some_and(|instance| {
            link.accepted_requests
                .has_execution(&instance.ephemeral_event_acks(&link.attachment_id))
        }) {
            return Ok(false);
        }
        store.remove_ephemeral_attachment(&link.instance_id, &link.attachment_id)
    };
    let attachment = match removed {
        Ok(attachment) => attachment,
        Err(SurfaceStoreError::Conflict(_)) => return Ok(false),
        Err(error) => return Err(wall_surface_store_error(error)),
    };
    let leases = attachment
        .and_then(|attachment| attachment.snapshot)
        .map(|snapshot| {
            snapshot
                .resource_leases
                .into_iter()
                .map(|lease| lease.lease_id)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    release_surface_resource_leases(resources, &leases, shared_images)
        .map_err(|_| wall_route_unavailable())?;
    Ok(true)
}

fn prune_wall_surfaces(
    walls: &SharedWallStore,
    devices: &SharedDeviceRegistryStore,
    instances: &SharedSurfaceInstanceStore,
    resources: &SharedSurfaceResourceStore,
    shared_images: &SharedImageStoreHandle,
) {
    let _ = walls.with_surface_links(|links| {
        let stale = links
            .iter()
            .filter(|(_, link)| {
                link.closing
                    || authorize_wall_registration(devices, &link.device_id).is_err()
                    || walls
                        .authorize_surface(&link.device_id, &link.binding, &link.instance_id, None)
                        .is_err()
                    || instances
                        .lock()
                        .ok()
                        .and_then(|store| store.get(&link.instance_id))
                        .is_none_or(|instance| {
                            !instance.attachments.contains_key(&link.attachment_id)
                        })
            })
            .map(|(key, link)| (key.clone(), link.clone()))
            .collect::<Vec<_>>();
        for (key, link) in stale {
            if release_wall_surface_link(instances, resources, shared_images, &link)? {
                links.remove(&key);
            }
        }
        Ok(())
    });
}
