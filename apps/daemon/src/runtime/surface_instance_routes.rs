// Shared-memory cleanup, resource responses, and Surface instance CRUD.
fn release_surface_shared_memory(
    lease: &loom_protocol::SurfaceResourceLease,
    shared_images: &SharedImageStoreHandle,
) -> Result<()> {
    if lease.transport.kind != SurfaceResourceTransportKind::SharedMemory {
        return Ok(());
    }
    if let Some(handle) = lease.transport.handle.as_deref() {
        shared_images
            .lock()
            .map_err(|_| anyhow::anyhow!("lock shared image store"))?
            .release(handle);
    }
    Ok(())
}

fn surface_resource_request_digest<'a>(method: &str, path: &'a str) -> Option<&'a str> {
    if method != "GET" {
        return None;
    }
    let route = path.split('?').next().unwrap_or(path);
    let digest = route.strip_prefix("/v1/surfaces/resources/")?;
    (!digest.is_empty() && !digest.contains('/')).then_some(digest)
}

fn surface_resource_binary_response(
    digest: &str,
    lease_id: &str,
    surface_resources: &SharedSurfaceResourceStore,
) -> Result<RouteResponse> {
    let mut store = surface_resources
        .lock()
        .map_err(|_| anyhow::anyhow!("Surface resource store is unavailable"))?;
    match store.get_with_lease(digest, lease_id) {
        Ok(payload) => Ok(RouteResponse::Binary {
            status: 200,
            content_type: payload.descriptor.mime,
            body: payload.bytes,
        }),
        Err(error) => surface_resource_store_error(error)
            .map(|(status, body)| RouteResponse::Text { status, body }),
    }
}

fn surface_resource_store_error(error: SurfaceResourceStoreError) -> Result<(u16, String)> {
    structured_error(
        error.status_code(),
        json!({
            "code": error.code(),
            "message": error.to_string(),
        }),
    )
}

fn list_surface_instances(surface_instances: &SharedSurfaceInstanceStore) -> Result<(u16, String)> {
    let store = surface_instances
        .lock()
        .map_err(|_| anyhow::anyhow!("Surface instance store is unavailable"))?;
    Ok((
        200,
        serde_json::to_string(&json!({ "instances": store.list() }))?,
    ))
}

fn get_surface_instance(
    instance_id: &str,
    surface_instances: &SharedSurfaceInstanceStore,
) -> Result<(u16, String)> {
    let store = surface_instances
        .lock()
        .map_err(|_| anyhow::anyhow!("Surface instance store is unavailable"))?;
    let Some(instance) = store.get(instance_id) else {
        return surface_store_error(SurfaceStoreError::NotFound(instance_id.to_owned()));
    };
    Ok((200, serde_json::to_string(&instance)?))
}

fn create_surface_instance(
    body: &str,
    surface_instances: &SharedSurfaceInstanceStore,
    tool_registry: &ToolRegistry,
    framework_registry: &FrameworkRegistry,
    control_plane_root: &Path,
) -> Result<(u16, String)> {
    let request = match serde_json::from_str::<CreateSurfaceInstanceRequest>(body) {
        Ok(request) => request,
        Err(error) => return invalid_surface_payload(error),
    };
    let tool = match tool_registry.get_tool(request.art_id.trim()) {
        Ok(Some(tool)) => tool,
        Ok(None) => {
            return structured_error(
                404,
                json!({
                    "code": "surface_art_not_found",
                    "message": "installed Art was not found",
                }),
            )
        }
        Err(error) => {
            return structured_error(
                500,
                json!({
                    "code": "surface_art_lookup_failed",
                    "message": error.to_string(),
                }),
            )
        }
    };
    let tool = match resolve_registered_tool_package(
        &tool,
        tool_registry,
        framework_registry,
        control_plane_root,
    ) {
        Ok(tool) => tool,
        Err(error) => {
            return structured_error(
                409,
                json!({
                    "code": "surface_art_integrity_failed",
                    "message": error.to_string(),
                }),
            )
        }
    };
    let surface_manifest = match tool.surface_manifest() {
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
    if request
        .state_schema_version
        .is_some_and(|version| version != surface_manifest.state_schema_version)
    {
        return structured_error(
            409,
            json!({
                "code": "surface_state_schema_conflict",
                "message": format!(
                    "Art state schema is {}",
                    surface_manifest.state_schema_version
                ),
            }),
        );
    }
    let package = tool
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("artPackage"));
    let Some(version) = package
        .and_then(|package| package.get("version"))
        .and_then(Value::as_str)
    else {
        return structured_error(
            409,
            json!({
                "code": "surface_art_package_required",
                "message": "Surface instances require an immutable installed Art package",
            }),
        );
    };
    let Some(digest) = package
        .and_then(|package| package.get("digest"))
        .and_then(Value::as_str)
    else {
        return structured_error(
            409,
            json!({
                "code": "surface_art_digest_missing",
                "message": "installed Art package has no locked digest",
            }),
        );
    };
    if request
        .expected_version
        .as_deref()
        .is_some_and(|expected| expected != version)
    {
        return structured_error(
            409,
            json!({
                "code": "surface_art_version_conflict",
                "message": format!("installed Art version is {version}"),
            }),
        );
    }
    let mut store = surface_instances
        .lock()
        .map_err(|_| anyhow::anyhow!("Surface instance store is unavailable"))?;
    let qualified_art_id = tool.qualified_id();
    if surface_manifest.instance_mode == SurfaceInstanceMode::Shared {
        if let Some(existing) =
            store.find_shared(&qualified_art_id, version, digest, &request.persistence)
        {
            return Ok((
                200,
                serde_json::to_string(&json!({
                    "reused": true,
                    "instance": existing,
                }))?,
            ));
        }
    }
    match store.create(
        &qualified_art_id,
        version,
        digest,
        surface_manifest.state_schema_version,
        request.persistence,
        surface_manifest.instance_mode,
    ) {
        Ok(instance) => Ok((201, serde_json::to_string(&instance)?)),
        Err(error) => surface_store_error(error),
    }
}

fn delete_surface_instance(
    instance_id: &str,
    surface_instances: &SharedSurfaceInstanceStore,
    hook_bridge: &SharedHookBridgeRuntime,
    surface_resources: &SharedSurfaceResourceStore,
    shared_images: &SharedImageStoreHandle,
) -> Result<(u16, String)> {
    let mut store = surface_instances
        .lock()
        .map_err(|_| anyhow::anyhow!("Surface instance store is unavailable"))?;
    let existing = store.get(instance_id);
    match store.delete(instance_id) {
        Ok(()) => {
            drop(store);
            if let Some(existing) = existing {
                let lease_ids = existing
                    .attachments
                    .values()
                    .filter_map(|attachment| attachment.snapshot.as_ref())
                    .flat_map(|snapshot| snapshot.resource_leases.iter())
                    .map(|lease| lease.lease_id.clone())
                    .collect::<Vec<_>>();
                release_surface_resource_leases(surface_resources, &lease_ids, shared_images)?;
                // The deleted instance was the last thing referring to its resources, so this is
                // the moment they become collectable. The pass runs after the leases are released
                // so it sees them gone, and it re-reads the instance store, so a resource this
                // instance shared with another one is still protected.
                collect_surface_resource_garbage_logged(
                    surface_instances,
                    surface_resources,
                    "instance deleted",
                );
                for attachment in existing.attachments.values() {
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
                                    "state": "disposed",
                                    "revision": attachment.lifecycle_revision.saturating_add(1),
                                }
                            }
                        }),
                    );
                    broadcast_hook_bridge_json(
                        hook_bridge,
                        json!({
                            "method": SURFACE_EVENT_DISPOSE,
                            "params": {
                                "hookNodeId": attachment.descriptor.hook_node_id,
                                "instanceId": instance_id,
                                "attachmentId": attachment.descriptor.attachment_id,
                            },
                        }),
                    );
                }
            }
            Ok((204, String::new()))
        }
        Err(error) => surface_store_error(error),
    }
}
