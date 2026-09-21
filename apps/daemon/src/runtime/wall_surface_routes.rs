// Every Surface operation rechecks the wall binding; no general attachment grant escapes.
fn route_wall_surfaces(
    request: &ParsedHttpRequest,
    path: &str,
    services: &WallSurfaceServices<'_>,
    actor: Option<&str>,
) -> Option<Result<(u16, String)>> {
    if !path.starts_with("/v1/walls/surfaces/") {
        return None;
    }
    let response = (|| {
        let actor = wall_presenter_actor(actor)?;
        if request.method != "POST" {
            return Err(WallStoreError::new(
                405,
                "wall_surface_method",
                "Surface view requests require POST",
            ));
        }
        services.walls.with_surface_links(|links| match path {
            "/v1/walls/surfaces/open" => {
                services.open(parse_wall_body(&request.body)?, actor, links)
            }
            "/v1/walls/surfaces/state" => {
                let input: WallSurfaceRead = parse_wall_body(&request.body)?;
                let link = wall_surface_link(links, &input.view, actor)?;
                services
                    .walls
                    .authorize_surface(actor, &link.binding, &link.instance_id, None)?;
                services.state(link, input.snapshot_revision)
            }
            "/v1/walls/surfaces/close" => {
                let input: WallSurfaceClose = parse_wall_body(&request.body)?;
                let Some(link) = links.get_mut(&input.view.key()) else {
                    return wall_accepted();
                };
                if link.device_id != actor {
                    return Err(WallStoreError::new(
                        403,
                        "wall_surface_forbidden",
                        "Art view belongs to another device",
                    ));
                }
                if link.binding == input.view.binding
                    && link.attachment_id == input.view.attachment_id
                {
                    link.closing = true;
                    if release_wall_surface_link(
                        services.instances,
                        services.resources,
                        services.shared_images,
                        link,
                    )? {
                        links.remove(&input.view.key());
                    }
                }
                wall_accepted()
            }
            "/v1/walls/surfaces/image" => {
                services.image(parse_wall_body(&request.body)?, actor, links)
            }
            "/v1/walls/surfaces/event" => {
                services.event(parse_wall_body(&request.body)?, actor, links)
            }
            "/v1/walls/surfaces/confirmation" => {
                services.confirm(parse_wall_body(&request.body)?, actor, links)
            }
            "/v1/walls/surfaces/cancel" => {
                services.cancel(parse_wall_body(&request.body)?, actor, links)
            }
            _ => Err(WallStoreError::new(
                404,
                "wall_surface_route_not_found",
                "Art view route was not found",
            )),
        })
    })();
    Some(response.or_else(|error: WallStoreError| {
        structured_error(
            error.status,
            json!({"code": error.code, "message": error.message}),
        )
    }))
}

impl WallSurfaceServices<'_> {
    fn image(
        &self,
        input: WallSurfaceImage,
        actor: &str,
        links: &WallSurfaceLinks,
    ) -> WallSurfaceResult<(u16, String)> {
        let link = wall_surface_link(links, &input.view, actor)?;
        self.walls
            .authorize_surface(actor, &link.binding, &link.instance_id, None)?;
        let instance = self
            .instances
            .lock()
            .map_err(|_| wall_route_unavailable())?
            .get(&link.instance_id)
            .ok_or_else(|| {
                WallStoreError::new(404, "surface_not_found", "Art instance is unavailable")
            })?;
        let referenced = instance
            .attachments
            .get(&link.attachment_id)
            .and_then(|attachment| attachment.snapshot.as_ref())
            .is_some_and(|snapshot| {
                snapshot
                    .resources
                    .iter()
                    .any(|resource| resource.resource_id == input.resource_id)
            })
            || instance.latest_preview.as_ref().is_some_and(|preview| {
                surface_value_resource_matches(&preview.value, &input.resource_id)
            })
            || instance.latest_result.as_ref().is_some_and(|result| {
                result
                    .outputs
                    .values()
                    .any(|value| surface_value_resource_matches(value, &input.resource_id))
            });
        if !referenced {
            return Err(WallStoreError::new(
                403,
                "wall_surface_resource_forbidden",
                "Resource is not referenced by this Art view",
            ));
        }
        wall_image_payload(self.resources, &input.resource_id)
    }

    fn event(
        &self,
        input: WallSurfaceEvent,
        actor: &str,
        links: &mut WallSurfaceLinks,
    ) -> WallSurfaceResult<(u16, String)> {
        let link = wall_surface_link(links, &input.view, actor)?.clone();
        if input.sequence != link.sequence.saturating_add(1)
            || input.sequence > loom_protocol::wall::WALL_MAX_REVISION
            || input.event.instance_id != link.instance_id
            || input.event.attachment_id != link.attachment_id
        {
            return Err(WallStoreError::new(
                409,
                "wall_surface_event_stale",
                "Art event identity or sequence is stale",
            ));
        }
        self.walls.with_surface_authority(
            actor,
            &link.binding,
            &link.instance_id,
            Some(&input.placement_id),
            Some(input.pixel),
            |_| {
                let instance = self
                    .instances
                    .lock()
                    .map_err(|_| wall_route_unavailable())?
                    .get(&link.instance_id)
                    .ok_or_else(wall_route_unavailable)?;
                let current = links.get_mut(&input.view.key()).expect("validated view");
                if !current
                    .accepted_requests
                    .make_room(&instance.ephemeral_event_acks(&link.attachment_id))
                {
                    return Err(WallStoreError::new(
                        409,
                        "wall_surface_capacity",
                        "Art view has too much unfinished work",
                    ));
                }
                current.sequence = input.sequence;
                let action = input.event.action.clone().unwrap_or_default();
                let cancelable = input.event.class != loom_protocol::SurfaceEventClass::Continuous
                    && link.cancelable_actions.contains(&action);
                let ack = self
                    .actions
                    .submit(&link.instance_id, input.event)
                    .map_err(wall_surface_store_error)?;
                if ack.accepted {
                    current
                        .accepted_requests
                        .record(ack.clone(), action, cancelable);
                }
                wall_json(&ack).map(|(_, body)| (202, body))
            },
        )
    }

    fn confirm(
        &self,
        input: WallSurfaceDecision,
        actor: &str,
        links: &WallSurfaceLinks,
    ) -> WallSurfaceResult<(u16, String)> {
        let link = wall_surface_link(links, &input.view, actor)?;
        self.walls.with_surface_authority(
            actor,
            &link.binding,
            &link.instance_id,
            Some(&input.placement_id),
            Some(input.pixel),
            |_| {
                let ack = self
                    .actions
                    .confirm(SurfaceConfirmationDecision {
                        protocol_version: loom_protocol::SURFACE_PROTOCOL_VERSION.to_owned(),
                        confirmation_id: input.confirmation_id,
                        instance_id: link.instance_id.clone(),
                        attachment_id: link.attachment_id.clone(),
                        device_id: actor.to_owned(),
                        approved: input.approved,
                    })
                    .map_err(wall_surface_store_error)?;
                wall_json(&ack)
            },
        )
    }

    fn cancel(
        &self,
        input: WallSurfaceCancel,
        actor: &str,
        links: &WallSurfaceLinks,
    ) -> WallSurfaceResult<(u16, String)> {
        let link = wall_surface_link(links, &input.view, actor)?;
        self.walls
            .authorize_surface(actor, &link.binding, &link.instance_id, None)?;
        let instance = self
            .instances
            .lock()
            .map_err(|_| wall_route_unavailable())?
            .get(&link.instance_id)
            .ok_or_else(|| {
                WallStoreError::new(404, "surface_not_found", "Art instance is unavailable")
            })?;
        let owned = instance.pending_events.iter().any(|event| {
            event.attachment_id == link.attachment_id
                && instance
                    .event_acks
                    .get(&event.event_id)
                    .is_some_and(|ack| ack.request_id == input.request_id)
        });
        if !owned {
            return Err(WallStoreError::new(
                403,
                "wall_surface_forbidden",
                "Action belongs to another Art view",
            ));
        }
        let ack = self
            .actions
            .cancel(SurfaceActionCancelRequest {
                protocol_version: loom_protocol::SURFACE_PROTOCOL_VERSION.to_owned(),
                instance_id: link.instance_id.clone(),
                device_id: actor.to_owned(),
                request_id: input.request_id,
            })
            .map_err(wall_surface_store_error)?;
        wall_json(&ack).map(|(_, body)| (202, body))
    }
}

fn surface_value_resource_matches(value: &SurfacePortValue, id: &str) -> bool {
    matches!(value, SurfacePortValue::Resource { resource } if resource.resource_id == id)
}
