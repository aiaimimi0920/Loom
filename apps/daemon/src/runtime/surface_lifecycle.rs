// Surface attachment, snapshots, patches, generations, and lifecycle transitions.
fn attach_and_mount_surface(
    body: &str,
    surface_instances: &SharedSurfaceInstanceStore,
    device_registry: &SharedDeviceRegistryStore,
    tool_registry: &ToolRegistry,
    framework_registry: &FrameworkRegistry,
    control_plane_root: &Path,
    hook_bridge: &SharedHookBridgeRuntime,
    surface_resources: &SharedSurfaceResourceStore,
    shared_images: &SharedImageStoreHandle,
    authenticated_device_id: Option<&str>,
) -> Result<(u16, String)> {
    let request = match serde_json::from_str::<AttachAndMountSurfaceRequest>(body) {
        Ok(request) => request,
        Err(error) => return invalid_surface_payload(error),
    };
    if let Err(error) =
        validate_authenticated_device_identity(authenticated_device_id, request.device_id.trim())
    {
        return device_auth_error_response(error);
    }
    let create_body = serde_json::to_string(&json!({
        "artId": request.art_id,
        "expectedVersion": request.expected_version,
        "stateSchemaVersion": request.state_schema_version,
        "persistence": request.persistence,
    }))?;
    let (create_status, created) = create_surface_instance(
        &create_body,
        surface_instances,
        tool_registry,
        framework_registry,
        control_plane_root,
    )?;
    if create_status != 201 && create_status != 200 {
        return Ok((create_status, created));
    }
    let created_json: Value = serde_json::from_str(&created)?;
    let instance_id = created_json
        .pointer("/descriptor/instanceId")
        .or_else(|| created_json.pointer("/instance/descriptor/instanceId"))
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("created Surface instance has no instance id"))?
        .to_owned();
    let attach_body = serde_json::to_string(&json!({
        "hookNodeId": request.hook_node_id,
        "deviceId": request.device_id,
        "capabilities": request.capabilities,
    }))?;
    let (attach_status, attached) = attach_surface_instance(
        &instance_id,
        &attach_body,
        surface_instances,
        device_registry,
        authenticated_device_id,
    )?;
    if attach_status != 201 {
        let _ = delete_surface_instance(
            &instance_id,
            surface_instances,
            hook_bridge,
            surface_resources,
            shared_images,
        );
        return Ok((attach_status, attached));
    }
    let attached_json: Value = serde_json::from_str(&attached)?;
    let attachment_id = attached_json
        .pointer("/descriptor/attachmentId")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("created Surface attachment has no attachment id"))?;
    let mount_body = serde_json::to_string(&json!({"attachmentId": attachment_id}))?;
    let mounted = mount_surface_instance(
        &instance_id,
        &mount_body,
        surface_instances,
        tool_registry,
        framework_registry,
        control_plane_root,
        hook_bridge,
        surface_resources,
        shared_images,
    )?;
    if mounted.0 != 200 {
        let _ = delete_surface_instance(
            &instance_id,
            surface_instances,
            hook_bridge,
            surface_resources,
            shared_images,
        );
    } else {
        dispose_superseded_surface_attachments(
            &instance_id,
            attachment_id,
            request.hook_node_id.trim(),
            request.device_id.trim(),
            surface_instances,
            hook_bridge,
            surface_resources,
            shared_images,
            authenticated_device_id,
        )?;
    }
    Ok(mounted)
}

#[allow(clippy::too_many_arguments)]
fn dispose_superseded_surface_attachments(
    current_instance_id: &str,
    current_attachment_id: &str,
    hook_node_id: &str,
    device_id: &str,
    surface_instances: &SharedSurfaceInstanceStore,
    hook_bridge: &SharedHookBridgeRuntime,
    surface_resources: &SharedSurfaceResourceStore,
    shared_images: &SharedImageStoreHandle,
    authenticated_device_id: Option<&str>,
) -> Result<()> {
    let superseded = {
        let store = surface_instances
            .lock()
            .map_err(|_| anyhow::anyhow!("Surface instance store is unavailable"))?;
        let mut superseded = Vec::new();
        for instance in store.list() {
            for attachment in instance.attachments.values() {
                if attachment.lifecycle == loom_protocol::SurfaceLifecycleState::Disposed
                    || (instance.descriptor.instance_id == current_instance_id
                        && attachment.descriptor.attachment_id == current_attachment_id)
                    || attachment.descriptor.hook_node_id != hook_node_id
                    || attachment.descriptor.device_id != device_id
                {
                    continue;
                }
                superseded.push((
                    instance.descriptor.instance_id.clone(),
                    attachment.descriptor.attachment_id.clone(),
                    attachment.lifecycle_revision,
                ));
            }
        }
        superseded
    };

    for (instance_id, attachment_id, lifecycle_revision) in superseded {
        let event = SurfaceLifecycleEvent {
            protocol_version: loom_protocol::SURFACE_PROTOCOL_VERSION.to_owned(),
            instance_id: instance_id.clone(),
            attachment_id,
            state: loom_protocol::SurfaceLifecycleState::Disposed,
            revision: lifecycle_revision.saturating_add(1),
        };
        let body = serde_json::to_string(&event)?;
        let (status, response) = transition_surface_lifecycle(
            &instance_id,
            &body,
            surface_instances,
            hook_bridge,
            surface_resources,
            shared_images,
            authenticated_device_id,
        )?;
        if status != 200 {
            anyhow::bail!("superseded Surface attachment disposal returned {status}: {response}");
        }
    }
    Ok(())
}

fn attach_surface_instance(
    instance_id: &str,
    body: &str,
    surface_instances: &SharedSurfaceInstanceStore,
    device_registry: &SharedDeviceRegistryStore,
    authenticated_device_id: Option<&str>,
) -> Result<(u16, String)> {
    let request = match serde_json::from_str::<AttachSurfaceInstanceRequest>(body) {
        Ok(request) => request,
        Err(error) => return invalid_surface_payload(error),
    };
    if let Err(error) =
        validate_authenticated_device_identity(authenticated_device_id, request.device_id.trim())
    {
        return device_auth_error_response(error);
    }
    {
        let devices = device_registry
            .lock()
            .map_err(|_| anyhow::anyhow!("device registry is unavailable"))?;
        let Some(device) = devices.devices.get(request.device_id.trim()) else {
            return structured_error(
                404,
                json!({
                    "code": "surface_device_not_found",
                    "message": "Surface attachment device was not found",
                }),
            );
        };
        if device.approval != "approved" || !device.enabled {
            return structured_error(
                403,
                json!({
                    "code": "surface_device_not_authorized",
                    "message": "Surface attachment device is not approved and enabled",
                }),
            );
        }
    }
    let mut store = surface_instances
        .lock()
        .map_err(|_| anyhow::anyhow!("Surface instance store is unavailable"))?;
    match store.attach(
        instance_id,
        request.hook_node_id.trim(),
        request.device_id.trim(),
        request.capabilities,
    ) {
        Ok(attachment) => Ok((201, serde_json::to_string(&attachment)?)),
        Err(error) => surface_store_error(error),
    }
}

fn put_surface_snapshot(
    instance_id: &str,
    body: &str,
    surface_instances: &SharedSurfaceInstanceStore,
    surface_resources: &SharedSurfaceResourceStore,
    hook_bridge: &SharedHookBridgeRuntime,
) -> Result<(u16, String)> {
    let snapshot = match serde_json::from_str::<SurfaceSnapshot>(body) {
        Ok(snapshot) => snapshot,
        Err(error) => return invalid_surface_payload(error),
    };
    if let Err(error) = surface_resources
        .lock()
        .map_err(|_| anyhow::anyhow!("Surface resource store is unavailable"))?
        .validate_references(&snapshot.resources, &snapshot.resource_leases)
    {
        return surface_resource_store_error(error);
    }
    let mut store = surface_instances
        .lock()
        .map_err(|_| anyhow::anyhow!("Surface instance store is unavailable"))?;
    let attachment_id = snapshot.attachment_id.clone();
    match store.put_snapshot(instance_id, snapshot) {
        Ok(instance) => {
            let response = serde_json::to_string(&instance)?;
            drop(store);
            if let Some(attachment) = instance.attachments.get(&attachment_id) {
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

fn apply_surface_patch(
    instance_id: &str,
    body: &str,
    surface_instances: &SharedSurfaceInstanceStore,
    surface_resources: &SharedSurfaceResourceStore,
    hook_bridge: &SharedHookBridgeRuntime,
) -> Result<(u16, String)> {
    let patch = match serde_json::from_str::<SurfacePatch>(body) {
        Ok(patch) => patch,
        Err(error) => return invalid_surface_payload(error),
    };
    if let Err(error) = surface_resources
        .lock()
        .map_err(|_| anyhow::anyhow!("Surface resource store is unavailable"))?
        .validate_references(&patch.resources, &patch.resource_leases)
    {
        return surface_resource_store_error(error);
    }
    let mut store = surface_instances
        .lock()
        .map_err(|_| anyhow::anyhow!("Surface instance store is unavailable"))?;
    let attachment_id = patch.attachment_id.clone();
    let outbound_patch = patch.clone();
    match store.apply_patch(instance_id, patch) {
        Ok(instance) => {
            let response = serde_json::to_string(&instance)?;
            drop(store);
            if let Some(attachment) = instance.attachments.get(&attachment_id) {
                broadcast_hook_bridge_json(
                    hook_bridge,
                    json!({
                        "method": SURFACE_EVENT_PATCH,
                        "params": {
                            "hookNodeId": attachment.descriptor.hook_node_id,
                            "patch": outbound_patch,
                            "generation": instance.descriptor.generation,
                        },
                    }),
                );
            }
            Ok((200, response))
        }
        Err(error) => surface_store_error(error),
    }
}

fn begin_surface_generation(
    instance_id: &str,
    body: &str,
    surface_instances: &SharedSurfaceInstanceStore,
    hook_bridge: &SharedHookBridgeRuntime,
) -> Result<(u16, String)> {
    let request = if body.trim().is_empty() {
        BeginSurfaceGenerationRequest::default()
    } else {
        match serde_json::from_str::<BeginSurfaceGenerationRequest>(body) {
            Ok(request) => request,
            Err(error) => return invalid_surface_payload(error),
        }
    };
    let mut store = surface_instances
        .lock()
        .map_err(|_| anyhow::anyhow!("Surface instance store is unavailable"))?;
    match store.begin_generation(instance_id, request.expected_generation) {
        Ok(descriptor) => {
            let response = serde_json::to_string(&descriptor)?;
            let attachments = store
                .get(instance_id)
                .map(|instance| instance.attachments)
                .unwrap_or_default();
            drop(store);
            for attachment in attachments.values() {
                broadcast_hook_bridge_json(
                    hook_bridge,
                    json!({
                        "method": SURFACE_EVENT_GENERATION,
                        "params": {
                            "hookNodeId": attachment.descriptor.hook_node_id,
                            "instanceId": instance_id,
                            "attachmentId": attachment.descriptor.attachment_id,
                            "generation": descriptor.generation,
                        },
                    }),
                );
            }
            Ok((200, response))
        }
        Err(error) => surface_store_error(error),
    }
}

fn transition_surface_lifecycle(
    instance_id: &str,
    body: &str,
    surface_instances: &SharedSurfaceInstanceStore,
    hook_bridge: &SharedHookBridgeRuntime,
    surface_resources: &SharedSurfaceResourceStore,
    shared_images: &SharedImageStoreHandle,
    authenticated_device_id: Option<&str>,
) -> Result<(u16, String)> {
    let event = match serde_json::from_str::<SurfaceLifecycleEvent>(body) {
        Ok(event) => event,
        Err(error) => return invalid_surface_payload(error),
    };
    if let Err(error) = validate_authenticated_surface_attachment(
        authenticated_device_id,
        instance_id,
        &event.attachment_id,
        surface_instances,
    ) {
        return device_auth_error_response(error);
    }
    let mut store = surface_instances
        .lock()
        .map_err(|_| anyhow::anyhow!("Surface instance store is unavailable"))?;
    let lease_ids = if event.state == loom_protocol::SurfaceLifecycleState::Disposed {
        store
            .get(instance_id)
            .and_then(|instance| instance.attachments.get(&event.attachment_id).cloned())
            .and_then(|attachment| attachment.snapshot)
            .map(|snapshot| {
                snapshot
                    .resource_leases
                    .into_iter()
                    .map(|lease| lease.lease_id)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    match store.transition_lifecycle(instance_id, event.clone()) {
        Ok(attachment) => {
            let response = serde_json::to_string(&attachment)?;
            drop(store);
            release_surface_resource_leases(surface_resources, &lease_ids, shared_images)?;
            broadcast_hook_bridge_json(
                hook_bridge,
                json!({
                    "method": SURFACE_EVENT_LIFECYCLE,
                    "params": {
                        "hookNodeId": attachment.descriptor.hook_node_id,
                        "event": event,
                    }
                }),
            );
            if attachment.lifecycle == loom_protocol::SurfaceLifecycleState::Disposed {
                broadcast_hook_bridge_json(
                    hook_bridge,
                    json!({
                        "method": SURFACE_EVENT_DISPOSE,
                        "params": {
                            "hookNodeId": attachment.descriptor.hook_node_id,
                            "instanceId": attachment.descriptor.instance_id,
                            "attachmentId": attachment.descriptor.attachment_id,
                        }
                    }),
                );
            }
            Ok((200, response))
        }
        Err(error) => surface_store_error(error),
    }
}
