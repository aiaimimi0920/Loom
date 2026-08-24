// Surface mounting, package resolution, host capabilities, route paths, and URL encoding.
fn mount_surface_instance(
    instance_id: &str,
    body: &str,
    surface_instances: &SharedSurfaceInstanceStore,
    tool_registry: &ToolRegistry,
    framework_registry: &FrameworkRegistry,
    control_plane_root: &Path,
    hook_bridge: &SharedHookBridgeRuntime,
    surface_resources: &SharedSurfaceResourceStore,
    shared_images: &SharedImageStoreHandle,
) -> Result<(u16, String)> {
    let request = match serde_json::from_str::<MountSurfaceInstanceRequest>(body) {
        Ok(request) => request,
        Err(error) => return invalid_surface_payload(error),
    };
    let instance = {
        let store = surface_instances
            .lock()
            .map_err(|_| anyhow::anyhow!("Surface instance store is unavailable"))?;
        let Some(instance) = store.get(instance_id) else {
            return surface_store_error(SurfaceStoreError::NotFound(instance_id.to_owned()));
        };
        instance
    };
    let Some(attachment) = instance.attachments.get(request.attachment_id.trim()) else {
        return surface_store_error(SurfaceStoreError::NotFound(request.attachment_id));
    };
    let previous_lease_ids = attachment
        .snapshot
        .as_ref()
        .map(|snapshot| {
            snapshot
                .resource_leases
                .iter()
                .map(|lease| lease.lease_id.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let existing_snapshot_source = attachment.snapshot.clone();
    let shared_snapshot_source = if instance.descriptor.instance_mode == SurfaceInstanceMode::Shared
        && existing_snapshot_source.is_none()
    {
        instance
            .attachments
            .values()
            .filter(|candidate| {
                candidate.descriptor.attachment_id != attachment.descriptor.attachment_id
            })
            .find_map(|candidate| candidate.snapshot.clone())
    } else {
        None
    };
    let snapshot_source = existing_snapshot_source
        .as_ref()
        .or(shared_snapshot_source.as_ref());
    let tool = match loom_tool_registry::install::resolve_installed_art_package(
        control_plane_root,
        &instance.descriptor.art_id,
        &instance.descriptor.art_version,
        &instance.descriptor.package_digest,
        tool_registry,
        framework_registry,
    ) {
        Ok(tool) => tool,
        Err(error) => {
            return structured_error(
                409,
                json!({
                    "code": "surface_art_package_unavailable",
                    "message": error.to_string(),
                }),
            )
        }
    };
    let manifest = match tool.surface_manifest() {
        Ok(Some(manifest)) => manifest,
        Ok(None) => {
            return structured_error(
                409,
                json!({
                    "code": "surface_manifest_missing",
                    "message": "Art package does not declare a Surface",
                }),
            )
        }
        Err(error) => return tool_registry_error_response(error),
    };
    let host = attachment
        .host_capabilities
        .clone()
        .unwrap_or_else(default_declarative_surface_host_capabilities);
    if host.api_version != loom_protocol::SURFACE_API_VERSION {
        return structured_error(
            409,
            json!({
                "code": "surface_api_incompatible",
                "message": format!("Hook Surface API {} is not supported", host.api_version),
            }),
        );
    }
    let missing_nodes = manifest
        .required_nodes
        .iter()
        .filter(|node| !host.nodes.contains(node))
        .cloned()
        .collect::<Vec<_>>();
    let missing_capabilities = manifest
        .required_capabilities
        .iter()
        .filter(|capability| !surface_host_supports_capability(&host, capability))
        .cloned()
        .collect::<Vec<_>>();
    if !missing_nodes.is_empty() || !missing_capabilities.is_empty() {
        return structured_error(
            409,
            json!({
                "code": "surface_host_capability_missing",
                "message": "Hook cannot satisfy the Surface requirements",
                "missingNodes": missing_nodes,
                "missingCapabilities": missing_capabilities,
            }),
        );
    }
    let selected_variant = manifest.variants.iter().find(|variant| {
        matches!(
            variant.runtime,
            SurfaceRuntimeKind::Declarative | SurfaceRuntimeKind::Javascript
        ) && host.runtimes.contains(&variant.runtime)
            && variant
                .required_capabilities
                .iter()
                .all(|capability| surface_host_supports_capability(&host, capability))
    });
    let (runtime, entry) = if let Some(variant) = selected_variant {
        (variant.runtime.clone(), variant.entry.clone())
    } else if host.runtimes.contains(&SurfaceRuntimeKind::Declarative) {
        let Some(entry) = manifest.fallback_scene.clone() else {
            return structured_error(
                409,
                json!({
                    "code": "surface_runtime_incompatible",
                    "message": "Hook does not provide a compatible Surface runtime and the package has no declarative fallback",
                }),
            );
        };
        (SurfaceRuntimeKind::Declarative, entry)
    } else {
        return structured_error(
            409,
            json!({
                "code": "surface_runtime_incompatible",
                "message": "Hook does not provide a compatible Surface runtime",
            }),
        );
    };
    let package = tool
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("artPackage"));
    let Some(art_dir) = package
        .and_then(|package| package.get("dir"))
        .and_then(Value::as_str)
    else {
        return structured_error(
            409,
            json!({
                "code": "surface_art_package_required",
                "message": "Surface package directory is unavailable",
            }),
        );
    };
    let read_declarative_entry = |scene_entry: &str| {
        let scene_path =
            resolve_surface_package_entry(control_plane_root, Path::new(art_dir), scene_entry)?;
        let metadata = fs::metadata(&scene_path)
            .with_context(|| format!("read Surface scene metadata {}", scene_path.display()))?;
        if metadata.len() > MAX_SURFACE_SCENE_BYTES {
            anyhow::bail!("Surface scene exceeds {MAX_SURFACE_SCENE_BYTES} bytes");
        }
        let surface_file =
            serde_json::from_slice::<DeclarativeSurfaceFile>(&fs::read(&scene_path)?)?;
        match surface_file {
            DeclarativeSurfaceFile::Document(document) => {
                if document
                    .protocol_version
                    .as_deref()
                    .is_some_and(|version| version != loom_protocol::SURFACE_PROTOCOL_VERSION)
                {
                    anyhow::bail!("Surface scene protocol is not supported");
                }
                Ok((
                    document.scene,
                    document.authoritative_state,
                    document.resources,
                ))
            }
            DeclarativeSurfaceFile::Scene(scene) => Ok((scene, Value::Null, Vec::new())),
        }
    };
    let fallback_scene = || SurfaceNode {
        id: "surface-root".to_owned(),
        node_type: "column".to_owned(),
        children: vec![SurfaceNode {
            id: "surface-runtime-status".to_owned(),
            node_type: "text".to_owned(),
            props: json!({"text": "交互界面暂不可用"}),
            ..SurfaceNode::default()
        }],
        ..SurfaceNode::default()
    };
    let (scene, authoritative_state, resources, resource_leases, entry_resource_id) = if let Some(
        source,
    ) =
        snapshot_source
    {
        let duplicated_leases = if source.resource_leases.is_empty() {
            Vec::new()
        } else {
            let mut resource_store = surface_resources
                .lock()
                .map_err(|_| anyhow::anyhow!("Surface resource store is unavailable"))?;
            source
                .resource_leases
                .iter()
                .map(|lease| resource_store.renew_loom_resource_lease(lease))
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|error| {
                    anyhow::anyhow!("renew recovered Surface resource lease: {error}")
                })?
        };
        (
            source.scene.clone(),
            source.authoritative_state.clone(),
            source.resources.clone(),
            duplicated_leases,
            source.entry_resource_id.clone(),
        )
    } else {
        match runtime {
            SurfaceRuntimeKind::Declarative => {
                let (scene, authoritative_state, resources) = match read_declarative_entry(&entry) {
                    Ok(surface) => surface,
                    Err(error) => {
                        return structured_error(
                            400,
                            json!({
                                "code": "invalid_surface_scene",
                                "message": error.to_string(),
                            }),
                        )
                    }
                };
                (scene, authoritative_state, resources, Vec::new(), None)
            }
            SurfaceRuntimeKind::Javascript => {
                let variant = selected_variant.expect("selected JavaScript Surface variant");
                let source = match load_surface_javascript_source(
                    control_plane_root,
                    Path::new(art_dir),
                    variant,
                ) {
                    Ok(source) => source,
                    Err(error) => {
                        eprintln!("loom JavaScript Surface source load failed: {error:#}");
                        return structured_error(
                            400,
                            json!({
                                "code": "invalid_surface_javascript",
                                "message": "JavaScript Surface source or descriptor is invalid",
                            }),
                        )
                    }
                };
                let lease = {
                    let mut resource_store = surface_resources
                        .lock()
                        .map_err(|_| anyhow::anyhow!("Surface resource store is unavailable"))?;
                    match resource_store.register(
                        SurfaceResourceKind::Binary,
                        "application/javascript",
                        &source,
                        None,
                        None,
                        None,
                    ) {
                        Ok(lease) => lease,
                        Err(error) => return surface_resource_store_error(error),
                    }
                };
                let (scene, authoritative_state, mut resources) =
                    if let Some(fallback_entry) = manifest.fallback_scene.as_deref() {
                        match read_declarative_entry(fallback_entry) {
                            Ok(surface) => surface,
                            Err(error) => {
                                return structured_error(
                                    400,
                                    json!({
                                        "code": "invalid_surface_fallback",
                                        "message": error.to_string(),
                                    }),
                                )
                            }
                        }
                    } else {
                        (fallback_scene(), Value::Null, Vec::new())
                    };
                if !resources
                    .iter()
                    .any(|resource| resource.resource_id == lease.resource.resource_id)
                {
                    resources.push(lease.resource.clone());
                }
                let entry_resource_id = lease.resource.resource_id.clone();
                (
                    scene,
                    authoritative_state,
                    resources,
                    vec![lease],
                    Some(entry_resource_id),
                )
            }
            SurfaceRuntimeKind::Shader | SurfaceRuntimeKind::LoomRemote => {
                return structured_error(
                    409,
                    json!({
                        "code": "surface_runtime_incompatible",
                        "message": "Selected Surface runtime is not implemented by this mount path",
                    }),
                )
            }
        }
    };
    let revision = existing_snapshot_source
        .as_ref()
        .map(|source| source.revision.saturating_add(1))
        .or_else(|| {
            shared_snapshot_source
                .as_ref()
                .map(|source| source.revision)
        })
        .unwrap_or_else(|| instance.descriptor.surface_revision.saturating_add(1));
    let view_id = snapshot_source
        .and_then(|source| source.view_id.as_ref())
        .filter(|view_id| {
            manifest
                .views
                .iter()
                .any(|view| view.id == view_id.as_str())
        })
        .cloned()
        .or_else(|| manifest.default_view_id.clone());
    let snapshot = SurfaceSnapshot {
        protocol_version: loom_protocol::SURFACE_PROTOCOL_VERSION.to_owned(),
        instance_id: instance_id.to_owned(),
        attachment_id: attachment.descriptor.attachment_id.clone(),
        art_id: instance.descriptor.art_id.clone(),
        art_version: instance.descriptor.art_version.clone(),
        revision,
        runtime: runtime.clone(),
        entry_resource_id,
        view_id,
        scene,
        authoritative_state,
        resources,
        resource_leases,
    };
    let mut store = surface_instances
        .lock()
        .map_err(|_| anyhow::anyhow!("Surface instance store is unavailable"))?;
    match store.put_snapshot(instance_id, snapshot) {
        Ok(instance) => {
            let response = serde_json::to_string(&json!({
                "runtime": runtime,
                "entry": entry,
                "instance": instance,
            }))?;
            drop(store);
            release_surface_resource_leases(surface_resources, &previous_lease_ids, shared_images)?;
            if let Some(attachment) = instance.attachments.get(request.attachment_id.trim()) {
                if let Some(snapshot) = &attachment.snapshot {
                    broadcast_hook_bridge_json(
                        hook_bridge,
                        json!({
                            "method": SURFACE_EVENT_SNAPSHOT,
                            "params": {
                                "hookNodeId": attachment.descriptor.hook_node_id,
                                "snapshot": snapshot,
                                "generation": instance.descriptor.generation,
                            },
                        }),
                    );
                }
                broadcast_hook_bridge_json(
                    hook_bridge,
                    json!({
                        "method": SURFACE_EVENT_LIFECYCLE,
                        "params": {
                            "hookNodeId": attachment.descriptor.hook_node_id,
                            "event": {
                                "protocolVersion": loom_protocol::SURFACE_PROTOCOL_VERSION,
                                "instanceId": instance_id,
                                "attachmentId": attachment.descriptor.attachment_id,
                                "state": attachment.lifecycle,
                                "revision": attachment.lifecycle_revision,
                            }
                        }
                    }),
                );
            }
            Ok((200, response))
        }
        Err(error) => surface_store_error(error),
    }
}
