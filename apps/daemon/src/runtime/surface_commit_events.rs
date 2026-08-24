// Surface result commits, events, confirmation, cancellation, and state migration.
fn commit_surface_preview(
    instance_id: &str,
    body: &str,
    surface_instances: &SharedSurfaceInstanceStore,
    surface_resources: &SharedSurfaceResourceStore,
) -> Result<(u16, String)> {
    let commit = match serde_json::from_str::<SurfacePreviewCommit>(body) {
        Ok(commit) => commit,
        Err(error) => return invalid_surface_payload(error),
    };
    if let SurfacePortValue::Resource { resource } = &commit.value {
        if let Err(error) = surface_resources
            .lock()
            .map_err(|_| anyhow::anyhow!("Surface resource store is unavailable"))?
            .validate_descriptor(resource)
        {
            return surface_resource_store_error(error);
        }
    }
    let mut store = surface_instances
        .lock()
        .map_err(|_| anyhow::anyhow!("Surface instance store is unavailable"))?;
    match store.commit_preview(instance_id, commit) {
        Ok(instance) => Ok((200, serde_json::to_string(&instance)?)),
        Err(error) => surface_store_error(error),
    }
}

fn commit_surface_result(
    instance_id: &str,
    body: &str,
    surface_instances: &SharedSurfaceInstanceStore,
    surface_resources: &SharedSurfaceResourceStore,
) -> Result<(u16, String)> {
    let commit = match serde_json::from_str::<SurfaceResultCommit>(body) {
        Ok(commit) => commit,
        Err(error) => return invalid_surface_payload(error),
    };
    {
        let mut resources = surface_resources
            .lock()
            .map_err(|_| anyhow::anyhow!("Surface resource store is unavailable"))?;
        for output in commit.outputs.values() {
            if let SurfacePortValue::Resource { resource } = output {
                if let Err(error) = resources.validate_descriptor(resource) {
                    return surface_resource_store_error(error);
                }
            }
        }
    }
    let mut store = surface_instances
        .lock()
        .map_err(|_| anyhow::anyhow!("Surface instance store is unavailable"))?;
    match store.commit_result(instance_id, commit) {
        Ok(instance) => Ok((200, serde_json::to_string(&instance)?)),
        Err(error) => surface_store_error(error),
    }
}

fn record_surface_failure(
    instance_id: &str,
    body: &str,
    surface_instances: &SharedSurfaceInstanceStore,
) -> Result<(u16, String)> {
    let failure = match serde_json::from_str::<SurfaceExecutionFailure>(body) {
        Ok(failure) => failure,
        Err(error) => return invalid_surface_payload(error),
    };
    let mut store = surface_instances
        .lock()
        .map_err(|_| anyhow::anyhow!("Surface instance store is unavailable"))?;
    match store.record_failure(instance_id, failure) {
        Ok(instance) => Ok((200, serde_json::to_string(&instance)?)),
        Err(error) => surface_store_error(error),
    }
}

fn accept_surface_event(
    instance_id: &str,
    body: &str,
    surface_actions: &SharedSurfaceActionExecutor,
    surface_instances: &SharedSurfaceInstanceStore,
    authenticated_device_id: Option<&str>,
) -> Result<(u16, String)> {
    let event = match serde_json::from_str::<SurfaceEvent>(body) {
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
    match surface_actions.submit(instance_id, event) {
        Ok(ack) => Ok((202, serde_json::to_string(&ack)?)),
        Err(error) => surface_store_error(error),
    }
}

fn decide_surface_confirmation(
    body: &str,
    surface_actions: &SharedSurfaceActionExecutor,
    device_registry: &SharedDeviceRegistryStore,
    authenticated_device_id: Option<&str>,
) -> Result<(u16, String)> {
    let decision = match serde_json::from_str::<SurfaceConfirmationDecision>(body) {
        Ok(decision) => decision,
        Err(error) => return invalid_surface_payload(error),
    };
    if let Err(error) =
        validate_authenticated_device_identity(authenticated_device_id, &decision.device_id)
    {
        return device_auth_error_response(error);
    }
    let authorized = device_registry
        .lock()
        .map_err(|_| anyhow::anyhow!("Device registry is unavailable"))?
        .devices
        .get(&decision.device_id)
        .is_some_and(|device| device.approval == "approved" && device.enabled);
    if !authorized {
        return structured_error(
            403,
            json!({
                "code": "surface_device_not_authorized",
                "message": "Surface confirmation device is not approved and enabled",
            }),
        );
    }
    match surface_actions.confirm(decision) {
        Ok(ack) => Ok((200, serde_json::to_string(&ack)?)),
        Err(error) => surface_store_error(error),
    }
}

fn cancel_surface_action(
    body: &str,
    surface_actions: &SharedSurfaceActionExecutor,
    device_registry: &SharedDeviceRegistryStore,
    authenticated_device_id: Option<&str>,
) -> Result<(u16, String)> {
    let request = match serde_json::from_str::<SurfaceActionCancelRequest>(body) {
        Ok(request) => request,
        Err(error) => return invalid_surface_payload(error),
    };
    if let Err(error) =
        validate_authenticated_device_identity(authenticated_device_id, &request.device_id)
    {
        return device_auth_error_response(error);
    }
    let authorized = device_registry
        .lock()
        .map_err(|_| anyhow::anyhow!("Device registry is unavailable"))?
        .devices
        .get(&request.device_id)
        .is_some_and(|device| device.approval == "approved" && device.enabled);
    if !authorized {
        return structured_error(
            403,
            json!({
                "code": "surface_device_not_authorized",
                "message": "Surface cancellation device is not approved and enabled",
            }),
        );
    }
    match surface_actions.cancel(request) {
        Ok(ack) => Ok((202, serde_json::to_string(&ack)?)),
        Err(error) => surface_store_error(error),
    }
}

fn migrate_surface_instance(
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
    let request = match serde_json::from_str::<MigrateSurfaceInstanceRequest>(body) {
        Ok(request) => request,
        Err(error) => return invalid_surface_payload(error),
    };
    let before = {
        let store = surface_instances
            .lock()
            .map_err(|_| anyhow::anyhow!("Surface instance store is unavailable"))?;
        let Some(instance) = store.get(instance_id) else {
            return surface_store_error(SurfaceStoreError::NotFound(instance_id.to_owned()));
        };
        instance
    };
    let target_tool = match loom_tool_registry::install::resolve_installed_art_package(
        control_plane_root,
        &before.descriptor.art_id,
        request.target_version.trim(),
        request.target_digest.trim(),
        tool_registry,
        framework_registry,
    ) {
        Ok(tool) => tool,
        Err(error) => {
            return structured_error(
                409,
                json!({
                    "code": "surface_migration_target_unavailable",
                    "message": error.to_string(),
                }),
            )
        }
    };
    let target_manifest = match target_tool.surface_manifest() {
        Ok(Some(manifest)) => manifest,
        Ok(None) => {
            return structured_error(
                409,
                json!({
                    "code": "surface_migration_manifest_missing",
                    "message": "target Art package does not declare a Surface",
                }),
            )
        }
        Err(error) => return tool_registry_error_response(error),
    };
    let target_art_dir = target_tool
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.pointer("/artPackage/dir"))
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("target Surface package directory is unavailable"))?;
    if let Err(error) =
        validate_surface_runtime_entries(control_plane_root, &target_art_dir, &target_manifest)
    {
        return structured_error(
            409,
            json!({
                "code": "surface_migration_health_check_failed",
                "message": error.to_string(),
            }),
        );
    }
    let is_rollback = before.migration_history.iter().any(|checkpoint| {
        checkpoint.art_version == request.target_version
            && checkpoint
                .package_digest
                .eq_ignore_ascii_case(&request.target_digest)
            && checkpoint.state_schema_version == target_manifest.state_schema_version
    });
    let migrated_state = if is_rollback {
        before.authoritative_state.clone()
    } else {
        match migrate_surface_state(
            control_plane_root,
            &target_art_dir,
            &target_manifest,
            before.descriptor.state_schema_version,
            before.authoritative_state.clone(),
        ) {
            Ok(state) => state,
            Err(error) => {
                return structured_error(
                    409,
                    json!({
                        "code": "surface_state_migration_failed",
                        "message": error.to_string(),
                    }),
                )
            }
        }
    };
    let previous_lease_ids = before
        .attachments
        .values()
        .filter_map(|attachment| attachment.snapshot.as_ref())
        .flat_map(|snapshot| snapshot.resource_leases.iter())
        .map(|lease| lease.lease_id.clone())
        .collect::<Vec<_>>();
    let attachment_states = before
        .attachments
        .values()
        .map(|attachment| {
            (
                attachment.descriptor.attachment_id.clone(),
                attachment.lifecycle.clone(),
            )
        })
        .collect::<Vec<_>>();
    let migrated = {
        let mut store = surface_instances
            .lock()
            .map_err(|_| anyhow::anyhow!("Surface instance store is unavailable"))?;
        match store.migrate_instance(
            instance_id,
            request.expected_generation,
            request.target_version.trim(),
            request.target_digest.trim(),
            target_manifest.state_schema_version,
            migrated_state,
        ) {
            Ok(instance) => instance,
            Err(error) => return surface_store_error(error),
        }
    };

    let remount = remount_surface_attachments(
        instance_id,
        &attachment_states,
        surface_instances,
        tool_registry,
        framework_registry,
        control_plane_root,
        hook_bridge,
        surface_resources,
        shared_images,
    );
    if let Err(error) = remount {
        let new_lease_ids = surface_instances
            .lock()
            .ok()
            .and_then(|store| store.get(instance_id))
            .map(|instance| {
                instance
                    .attachments
                    .values()
                    .filter_map(|attachment| attachment.snapshot.as_ref())
                    .flat_map(|snapshot| snapshot.resource_leases.iter())
                    .map(|lease| lease.lease_id.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        release_surface_resource_leases(surface_resources, &new_lease_ids, shared_images)?;
        {
            let mut store = surface_instances
                .lock()
                .map_err(|_| anyhow::anyhow!("Surface instance store is unavailable"))?;
            store.migrate_instance(
                instance_id,
                Some(migrated.descriptor.generation),
                &before.descriptor.art_version,
                &before.descriptor.package_digest,
                before.descriptor.state_schema_version,
                before.authoritative_state.clone(),
            )?;
        }
        let _ = remount_surface_attachments(
            instance_id,
            &attachment_states,
            surface_instances,
            tool_registry,
            framework_registry,
            control_plane_root,
            hook_bridge,
            surface_resources,
            shared_images,
        );
        return structured_error(
            409,
            json!({
                "code": "surface_migration_remount_failed",
                "message": error.to_string(),
                "rolledBack": true,
            }),
        );
    }
    release_surface_resource_leases(surface_resources, &previous_lease_ids, shared_images)?;
    let current = surface_instances
        .lock()
        .map_err(|_| anyhow::anyhow!("Surface instance store is unavailable"))?
        .get(instance_id)
        .ok_or_else(|| anyhow::anyhow!("migrated Surface instance disappeared"))?;
    Ok((200, serde_json::to_string(&current)?))
}
