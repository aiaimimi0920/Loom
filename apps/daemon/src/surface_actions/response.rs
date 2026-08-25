// Action response parsing, host-brokered resources, and Surface state commits.
fn parse_surface_action_response(
    mut value: Value,
) -> Result<SurfaceActionResponse, SurfaceExecutionError> {
    if value.get("status").and_then(Value::as_str) == Some("error") {
        return Err(parse_surface_runtime_error(&value));
    }
    let payload = value
        .as_object_mut()
        .and_then(|output| output.remove("surfaceAction"))
        .ok_or_else(|| {
            execution_error(
                "surface_action_response_missing",
                "Art output has no surfaceAction response",
            )
        })?;
    if !loom_security::json::value_is_within_depth(
        &payload,
        loom_security::json::MAX_PROCESS_RESPONSE_DEPTH,
    ) {
        return Err(execution_error(
            "surface_action_response_limit",
            format!(
                "Surface action response exceeds the nesting limit of {} levels",
                loom_security::json::MAX_PROCESS_RESPONSE_DEPTH
            ),
        ));
    }
    let response = serde_json::from_value::<SurfaceActionResponse>(payload).map_err(|error| {
        execution_error(
            "surface_action_response_invalid",
            format!("Surface action response is invalid: {error}"),
        )
    })?;
    validate_surface_protocol(&response.protocol_version)
        .map_err(|error| execution_error("surface_action_protocol_invalid", error.to_string()))?;
    Ok(response)
}

/// Preserve the Art runtime's structured failure instead of reporting it as a
/// malformed success payload with a missing `surfaceAction` field.
fn parse_surface_runtime_error(value: &Value) -> SurfaceExecutionError {
    let runtime_error = value.get("error").and_then(Value::as_object);
    let code = runtime_error
        .and_then(|error| error.get("code"))
        .and_then(Value::as_str)
        .filter(|code| !code.is_empty())
        .unwrap_or("unknown");
    let message = runtime_error
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .filter(|message| !message.is_empty())
        .unwrap_or("Art runtime returned an error");
    execution_error(
        "surface_art_runtime_error",
        format!("Art runtime error `{code}`: {message}"),
    )
}

fn apply_action_response(
    job: &SurfaceActionJob,
    response: SurfaceActionResponse,
    surface_instances: &SharedSurfaceInstanceStore,
    surface_resources: &SharedSurfaceResourceStore,
    hook_bridge: &SharedHookBridgeRuntime,
) -> Result<(), SurfaceExecutionError> {
    let response = broker_action_resource_uploads(response, surface_resources)?;
    validate_action_response_resources(&response, surface_resources)?;
    for update in response.patches {
        let target_attachments = {
            let store = surface_instances.lock().map_err(|_| {
                execution_error("surface_store_unavailable", "Surface store is unavailable")
            })?;
            let instance = store.get(&job.event.instance_id).ok_or_else(|| {
                execution_error("surface_instance_missing", "Surface instance was removed")
            })?;
            if let Some(attachment_id) = update.attachment_id.as_ref() {
                vec![attachment_id.clone()]
            } else if instance.descriptor.instance_mode == SurfaceInstanceMode::Shared {
                instance
                    .attachments
                    .values()
                    .filter(|attachment| attachment.snapshot.is_some())
                    .map(|attachment| attachment.descriptor.attachment_id.clone())
                    .collect::<Vec<_>>()
            } else {
                vec![job.event.attachment_id.clone()]
            }
        };
        for (target_index, target_attachment) in target_attachments.into_iter().enumerate() {
            let mut target_update = update.clone();
            if target_index > 0 && !target_update.resource_leases.is_empty() {
                let mut resources = surface_resources.lock().map_err(|_| {
                    execution_error(
                        "surface_resource_store_unavailable",
                        "Surface resource store is unavailable",
                    )
                })?;
                target_update.resource_leases = target_update
                    .resource_leases
                    .iter()
                    .map(|lease| resources.duplicate_loom_resource_lease(lease))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(resource_execution_error)?;
            }
            let (patch, hook_node_id) = {
                let mut store = surface_instances.lock().map_err(|_| {
                    execution_error("surface_store_unavailable", "Surface store is unavailable")
                })?;
                let instance = store.get(&job.event.instance_id).ok_or_else(|| {
                    execution_error("surface_instance_missing", "Surface instance was removed")
                })?;
                ensure_current_action_generation(
                    instance.descriptor.generation,
                    job.event.generation,
                )?;
                let attachment = instance
                    .attachments
                    .get(&target_attachment)
                    .ok_or_else(|| {
                        execution_error(
                            "surface_attachment_missing",
                            "Surface attachment was removed",
                        )
                    })?;
                let base_revision = attachment
                    .snapshot
                    .as_ref()
                    .ok_or_else(|| {
                        execution_error("surface_snapshot_missing", "Surface is not mounted")
                    })?
                    .revision;
                let patch = SurfacePatch {
                    protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
                    instance_id: job.event.instance_id.clone(),
                    attachment_id: target_attachment.clone(),
                    base_revision,
                    revision: base_revision.saturating_add(1),
                    operations: target_update.operations,
                    state_patch: target_update.state_patch,
                    resources: target_update.resources,
                    resource_leases: target_update.resource_leases,
                };
                let hook_node_id = attachment.descriptor.hook_node_id.clone();
                store
                    .apply_patch(&job.event.instance_id, patch.clone())
                    .map_err(store_execution_error)?;
                (patch, hook_node_id)
            };
            broadcast_hook_bridge_json(
                hook_bridge,
                json!({
                    "method": SURFACE_EVENT_PATCH,
                    "params": {
                        "hookNodeId": hook_node_id,
                        "patch": patch,
                        "generation": job.event.generation,
                    }
                }),
            );
        }
    }

    if let Some(preview) = response.preview {
        let (commit, hook_nodes) = {
            let mut store = surface_instances.lock().map_err(|_| {
                execution_error("surface_store_unavailable", "Surface store is unavailable")
            })?;
            let instance = store.get(&job.event.instance_id).ok_or_else(|| {
                execution_error("surface_instance_missing", "Surface instance was removed")
            })?;
            ensure_current_action_generation(instance.descriptor.generation, job.event.generation)?;
            let commit = SurfacePreviewCommit {
                protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
                instance_id: job.event.instance_id.clone(),
                request_id: job.ack.request_id.clone(),
                generation: job.event.generation,
                preview_revision: instance.descriptor.preview_revision.saturating_add(1),
                port_id: preview.port_id,
                value: preview.value,
            };
            let hook_nodes = instance
                .attachments
                .values()
                .map(|attachment| attachment.descriptor.hook_node_id.clone())
                .collect::<Vec<_>>();
            store
                .commit_preview(&job.event.instance_id, commit.clone())
                .map_err(store_execution_error)?;
            (commit, hook_nodes)
        };
        for hook_node_id in hook_nodes {
            broadcast_hook_bridge_json(
                hook_bridge,
                json!({
                    "method": SURFACE_EVENT_PREVIEW,
                    "params": { "hookNodeId": hook_node_id, "commit": &commit }
                }),
            );
        }
    }

    if let Some(result) = response.result {
        let (commit, hook_nodes) = {
            let mut store = surface_instances.lock().map_err(|_| {
                execution_error("surface_store_unavailable", "Surface store is unavailable")
            })?;
            let instance = store.get(&job.event.instance_id).ok_or_else(|| {
                execution_error("surface_instance_missing", "Surface instance was removed")
            })?;
            ensure_current_action_generation(instance.descriptor.generation, job.event.generation)?;
            let commit = SurfaceResultCommit {
                protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
                instance_id: job.event.instance_id.clone(),
                request_id: job.ack.request_id.clone(),
                generation: job.event.generation,
                result_revision: instance.descriptor.result_revision.saturating_add(1),
                outputs: result.outputs,
                state_patch: result.state_patch,
            };
            let hook_nodes = instance
                .attachments
                .values()
                .map(|attachment| attachment.descriptor.hook_node_id.clone())
                .collect::<Vec<_>>();
            store
                .commit_result(&job.event.instance_id, commit.clone())
                .map_err(store_execution_error)?;
            (commit, hook_nodes)
        };
        for hook_node_id in hook_nodes {
            broadcast_hook_bridge_json(
                hook_bridge,
                json!({
                    "method": SURFACE_EVENT_RESULT,
                    "params": { "hookNodeId": hook_node_id, "commit": &commit }
                }),
            );
        }
    }
    Ok(())
}

fn ensure_current_action_generation(
    current_generation: u64,
    action_generation: u64,
) -> Result<(), SurfaceExecutionError> {
    if current_generation != action_generation {
        return Err(execution_error(
            "surface_action_stale_generation",
            "Surface action completed for a stale generation",
        ));
    }
    Ok(())
}

fn broker_action_resource_uploads(
    mut response: SurfaceActionResponse,
    surface_resources: &SharedSurfaceResourceStore,
) -> Result<SurfaceActionResponse, SurfaceExecutionError> {
    let uploads = std::mem::take(&mut response.resource_uploads);
    if uploads.is_empty() {
        return Ok(response);
    }
    if uploads.len() > 32 {
        return Err(execution_error(
            "surface_resource_upload_limit",
            "Surface action returned more than 32 resource uploads",
        ));
    }
    if response.patches.is_empty() {
        return Err(execution_error(
            "surface_resource_patch_required",
            "Surface action resource uploads require at least one patch",
        ));
    }
    let mut aliases = BTreeMap::new();
    let mut leases = Vec::new();
    let mut total_bytes = 0_usize;
    let mut store = surface_resources.lock().map_err(|_| {
        execution_error(
            "surface_resource_store_unavailable",
            "Surface resource store is unavailable",
        )
    })?;
    for upload in uploads {
        if upload.id.is_empty()
            || upload.id.len() > 160
            || !upload
                .id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        {
            return Err(execution_error(
                "surface_resource_upload_id_invalid",
                format!("Surface resource upload id `{}` is invalid", upload.id),
            ));
        }
        let alias = format!("surface-upload:{}", upload.id);
        if aliases.contains_key(&alias) {
            return Err(execution_error(
                "surface_resource_upload_duplicate",
                format!("Surface resource upload `{}` is duplicated", upload.id),
            ));
        }
        let bytes = BASE64.decode(upload.data_base64.trim()).map_err(|_| {
            execution_error(
                "surface_resource_upload_base64_invalid",
                format!(
                    "Surface resource upload `{}` is not valid Base64",
                    upload.id
                ),
            )
        })?;
        total_bytes = total_bytes.checked_add(bytes.len()).ok_or_else(|| {
            execution_error(
                "surface_resource_upload_limit",
                "Surface resource upload size overflowed",
            )
        })?;
        if total_bytes > super::surface_resources::MAX_SURFACE_RESOURCE_BYTES {
            return Err(execution_error(
                "surface_resource_upload_limit",
                "Surface action resource uploads exceed the 16 MiB request budget",
            ));
        }
        let lease = store
            .register(
                upload.kind,
                &upload.mime,
                &bytes,
                upload.width,
                upload.height,
                upload.lease_millis,
            )
            .map_err(resource_execution_error)?;
        aliases.insert(alias, lease.resource.resource_id.clone());
        leases.push(lease);
    }
    drop(store);

    let mut value = serde_json::to_value(&response).map_err(|error| {
        execution_error(
            "surface_resource_upload_resolution_failed",
            format!("serialize Surface action response: {error}"),
        )
    })?;
    replace_surface_resource_aliases(&mut value, &aliases);
    let mut response = serde_json::from_value::<SurfaceActionResponse>(value).map_err(|error| {
        execution_error(
            "surface_resource_upload_resolution_failed",
            format!("deserialize Surface action response: {error}"),
        )
    })?;
    let first_patch = response
        .patches
        .first_mut()
        .expect("resource uploads require a patch");
    for lease in leases {
        first_patch.resources.push(lease.resource.clone());
        first_patch.resource_leases.push(lease);
    }
    Ok(response)
}

fn replace_surface_resource_aliases(value: &mut Value, aliases: &BTreeMap<String, String>) {
    match value {
        Value::String(text) => {
            if let Some(resource_id) = aliases.get(text) {
                *text = resource_id.clone();
            }
        }
        Value::Array(values) => {
            for value in values {
                replace_surface_resource_aliases(value, aliases);
            }
        }
        Value::Object(values) => {
            for value in values.values_mut() {
                replace_surface_resource_aliases(value, aliases);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

fn validate_action_response_resources(
    response: &SurfaceActionResponse,
    surface_resources: &SharedSurfaceResourceStore,
) -> Result<(), SurfaceExecutionError> {
    let mut store = surface_resources.lock().map_err(|_| {
        execution_error(
            "surface_resource_store_unavailable",
            "Surface resource store is unavailable",
        )
    })?;
    for update in &response.patches {
        store
            .validate_references(&update.resources, &update.resource_leases)
            .map_err(resource_execution_error)?;
    }
    if let Some(preview) = &response.preview {
        if let loom_protocol::SurfacePortValue::Resource { resource } = &preview.value {
            store
                .validate_descriptor(resource)
                .map_err(resource_execution_error)?;
        }
    }
    if let Some(result) = &response.result {
        for output in result.outputs.values() {
            if let loom_protocol::SurfacePortValue::Resource { resource } = output {
                store
                    .validate_descriptor(resource)
                    .map_err(resource_execution_error)?;
            }
        }
    }
    Ok(())
}
