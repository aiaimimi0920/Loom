// Surface recovery filtering, subscriber channels, and route/tool error mapping.
fn surface_snapshot_recovery_messages_for_device(
    surface_instances: &SharedSurfaceInstanceStore,
    authenticated_device_id: Option<&str>,
) -> Vec<String> {
    let Ok(store) = surface_instances.lock() else {
        return Vec::new();
    };
    let mut latest_by_binding =
        BTreeMap::<(String, String), (u64, String, u64, SurfaceSnapshot)>::new();
    for instance in store.list() {
        let instance_id = instance.descriptor.instance_id.clone();
        let generation = instance.descriptor.generation;
        for attachment in instance.attachments.into_values() {
            if attachment.lifecycle == loom_protocol::SurfaceLifecycleState::Disposed {
                continue;
            }
            if authenticated_device_id
                .is_some_and(|device_id| attachment.descriptor.device_id != device_id)
            {
                continue;
            }
            let Some(snapshot) = attachment.snapshot else {
                continue;
            };
            let key = (
                attachment.descriptor.device_id,
                attachment.descriptor.hook_node_id,
            );
            let replace = match latest_by_binding.get(&key) {
                Some((created_at_ms, existing_instance_id, _, _)) => {
                    (instance.created_at_ms, instance_id.as_str())
                        > (*created_at_ms, existing_instance_id.as_str())
                }
                None => true,
            };
            if replace {
                latest_by_binding.insert(
                    key,
                    (
                        instance.created_at_ms,
                        instance_id.clone(),
                        generation,
                        snapshot,
                    ),
                );
            }
        }
    }
    let mut messages = latest_by_binding
        .into_iter()
        .filter_map(
            |((_device_id, hook_node_id), (_created_at_ms, _instance_id, generation, snapshot))| {
                serde_json::to_string(&json!({
                    "method": SURFACE_EVENT_SNAPSHOT,
                    "params": {
                        "hookNodeId": hook_node_id,
                        "snapshot": snapshot,
                        "generation": generation,
                    },
                }))
                .ok()
            },
        )
        .collect::<Vec<_>>();
    messages.extend(
        store
            .pending_confirmations()
            .into_iter()
            .filter_map(|confirmation| {
                if authenticated_device_id
                    .is_some_and(|device_id| confirmation.device_id != device_id)
                {
                    return None;
                }
                serde_json::to_string(&json!({
                    "method": SURFACE_EVENT_CONFIRMATION_REQUEST,
                    "params": confirmation,
                }))
                .ok()
            }),
    );
    messages
}

fn surface_message_visible_to_device(
    message: &Value,
    authenticated_device_id: Option<&str>,
    surface_instances: &SharedSurfaceInstanceStore,
) -> bool {
    let Some(device_id) = authenticated_device_id else {
        return true;
    };
    let Some(params) = message.get("params") else {
        return false;
    };
    if params.get("deviceId").and_then(Value::as_str) == Some(device_id) {
        return true;
    }
    let Ok(store) = surface_instances.lock() else {
        return false;
    };
    let attachment_matches = |instance_id: &str, attachment_id: &str| {
        store
            .get(instance_id)
            .and_then(|instance| instance.attachments.get(attachment_id).cloned())
            .is_some_and(|attachment| attachment.descriptor.device_id == device_id)
    };
    let instance_matches = |instance_id: &str| {
        store.get(instance_id).is_some_and(|instance| {
            instance
                .attachments
                .values()
                .any(|attachment| attachment.descriptor.device_id == device_id)
        })
    };

    if let Some(snapshot) = params.get("snapshot") {
        if let (Some(instance_id), Some(attachment_id)) = (
            snapshot.get("instanceId").and_then(Value::as_str),
            snapshot.get("attachmentId").and_then(Value::as_str),
        ) {
            return attachment_matches(instance_id, attachment_id);
        }
    }
    if let Some(patch) = params.get("patch") {
        if let (Some(instance_id), Some(attachment_id)) = (
            patch.get("instanceId").and_then(Value::as_str),
            patch.get("attachmentId").and_then(Value::as_str),
        ) {
            return attachment_matches(instance_id, attachment_id);
        }
    }
    if let Some(commit) = params.get("commit") {
        if let Some(instance_id) = commit.get("instanceId").and_then(Value::as_str) {
            return instance_matches(instance_id);
        }
    }
    if let Some(event) = params.get("event") {
        if let (Some(instance_id), Some(attachment_id)) = (
            event.get("instanceId").and_then(Value::as_str),
            event.get("attachmentId").and_then(Value::as_str),
        ) {
            return attachment_matches(instance_id, attachment_id);
        }
    }
    let instance_id = params
        .get("instanceId")
        .and_then(Value::as_str)
        .or_else(|| {
            params
                .get("failure")
                .and_then(|value| value.get("instanceId"))
                .and_then(Value::as_str)
        });
    if let Some(instance_id) = instance_id {
        if let Some(attachment_id) = params.get("attachmentId").and_then(Value::as_str) {
            return attachment_matches(instance_id, attachment_id);
        }
        return instance_matches(instance_id);
    }
    false
}

fn subscriber_accepts_broadcast(subscriber: &HookBridgeSubscriber, broadcast: &str) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(broadcast) else {
        return false;
    };
    let Some(method) = value.get("method").and_then(Value::as_str) else {
        return false;
    };
    subscriber
        .channels
        .iter()
        .any(|channel| channel_accepts_method(channel, method))
}

fn channel_accepts_method(channel: &str, method: &str) -> bool {
    let channel = channel.trim();
    !channel.is_empty() && method == channel
}

fn hook_bridge_read_timed_out(error: &tungstenite::Error) -> bool {
    matches!(
        error,
        tungstenite::Error::Io(io_error)
            if matches!(io_error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut)
    )
}

fn invalid_request(message: impl Into<String>) -> Result<(u16, String)> {
    structured_error(
        400,
        json!({
            "code": "invalid_request",
            "message": message.into(),
        }),
    )
}

fn id_mismatch(kind: &str, path_id: &str, body_id: &str) -> Result<(u16, String)> {
    structured_error(
        400,
        json!({
            "code": "id_mismatch",
            "message": format!("path {kind} id `{path_id}` does not match body id `{body_id}`"),
            "path_id": path_id,
            "body_id": body_id,
        }),
    )
}

fn tool_registry_error_response(error: ToolRegistryError) -> Result<(u16, String)> {
    match error {
        ToolRegistryError::InvalidToolDefinition { id, reason } => structured_error(
            400,
            json!({
                "code": "invalid_tool",
                "message": reason,
                "tool_id": id,
            }),
        ),
        ToolRegistryError::ExecutionRejected { id } => structured_error(
            400,
            json!({
                "code": "tool_disabled",
                "message": format!("tool `{id}` is disabled"),
                "tool_id": id,
            }),
        ),
        // A cancelled run is not a server fault, so it is reported as a conflict with the state the
        // caller itself put the run into rather than as a 500. The code matches the `cancelled` code
        // already used for a cancelled Art run on the Hook bridge, so a client recognises both alike.
        ToolRegistryError::ExecutionCancelled { id } => structured_error(
            409,
            json!({
                "code": "cancelled",
                "message": format!("tool `{id}` execution was cancelled"),
                "tool_id": id,
            }),
        ),
        ToolRegistryError::ParameterBinding { id, reason } => structured_error(
            400,
            json!({
                "code": "art_parameter_binding_error",
                "message": reason,
                "tool_id": id,
            }),
        ),
        ToolRegistryError::AmbiguousToolId { id } => structured_error(
            409,
            json!({
                "code": "ambiguous_tool_id",
                "message": format!("tool id `{id}` matches multiple publishers; use qualifiedId"),
                "tool_id": id,
            }),
        ),
        ToolRegistryError::UnsupportedExecution { id, execution_type } => structured_error(
            400,
            json!({
                "code": "unsupported_tool_execution",
                "message": format!("tool `{id}` execution type `{execution_type}` is not supported"),
                "tool_id": id,
                "execution_type": execution_type,
            }),
        ),
        ToolRegistryError::MissingMcpServer { tool_id, server_id } => structured_error(
            404,
            json!({
                "code": "mcp_server_not_found",
                "message": format!("MCP server `{server_id}` for tool `{tool_id}` was not found or is disabled"),
                "tool_id": tool_id,
                "server_id": server_id,
            }),
        ),
        ToolRegistryError::Mcp(error) => structured_error(
            500,
            json!({
                "code": "mcp_execution_error",
                "message": error.to_string(),
            }),
        ),
        ToolRegistryError::McpDependency {
            tool_id,
            server_id,
            code,
            reason,
        } => structured_error(
            if code == "mcp_dependency_missing" {
                404
            } else {
                409
            },
            json!({
                "code": code,
                "message": reason,
                "tool_id": tool_id,
                "server_id": server_id,
            }),
        ),
        ToolRegistryError::CloudInvalidMethod { id, method } => structured_error(
            400,
            json!({
                "code": "cloud_api_error",
                "message": format!("cloud API method `{method}` is not supported"),
                "tool_id": id,
                "method": method,
            }),
        ),
        ToolRegistryError::CloudRequest {
            id,
            endpoint,
            source,
        } => structured_error(
            500,
            json!({
                "code": "cloud_api_error",
                "message": source.to_string(),
                "tool_id": id,
                "endpoint": endpoint,
            }),
        ),
        ToolRegistryError::CloudSecurity {
            id,
            endpoint,
            reason,
        } => structured_error(
            403,
            json!({
                "code": "cloud_api_security_policy",
                "message": reason,
                "tool_id": id,
                "endpoint": endpoint,
            }),
        ),
        ToolRegistryError::CloudHttpStatus {
            id,
            endpoint,
            status,
            body,
        } => structured_error(
            500,
            json!({
                "code": "cloud_api_error",
                "message": format!("cloud API returned HTTP {status}: {body}"),
                "tool_id": id,
                "endpoint": endpoint,
                "status": status,
            }),
        ),
        ToolRegistryError::CloudJson {
            id,
            endpoint,
            source,
            body,
        } => structured_error(
            500,
            json!({
                "code": "cloud_api_error",
                "message": format!("cloud API returned invalid JSON: {source}"),
                "tool_id": id,
                "endpoint": endpoint,
                "body": body,
            }),
        ),
        ToolRegistryError::CloudTemplate { id, field, reason } => structured_error(
            400,
            json!({
                "code": "cloud_api_error",
                "message": format!("cloud API {field} template is invalid: {reason}"),
                "tool_id": id,
                "field": field,
            }),
        ),
        ToolRegistryError::FrameworkPackageNotFound {
            id,
            framework,
            path,
        } => structured_error(
            404,
            json!({
                "code": "framework_package_not_found",
                "message": format!("framework package `{framework}` for tool `{id}` was not found"),
                "tool_id": id,
                "framework": framework,
                "path": path,
            }),
        ),
        ToolRegistryError::FrameworkArtDirectoryNotFound { id, path } => structured_error(
            404,
            json!({
                "code": "framework_art_directory_not_found",
                "message": format!("framework Art directory for tool `{id}` was not found"),
                "tool_id": id,
                "path": path,
            }),
        ),
        ToolRegistryError::FrameworkProcessSpawn {
            id,
            framework,
            reason,
        } => structured_error(
            500,
            json!({
                "code": "framework_process_spawn_error",
                "message": reason,
                "tool_id": id,
                "framework": framework,
            }),
        ),
        ToolRegistryError::FrameworkProcessTimeout {
            id,
            framework,
            timeout_ms,
        } => structured_error(
            504,
            json!({
                "code": "framework_process_timeout",
                "message": format!("framework process timed out after {timeout_ms}ms"),
                "tool_id": id,
                "framework": framework,
                "timeoutMs": timeout_ms,
            }),
        ),
        ToolRegistryError::FrameworkProcessIo {
            id,
            framework,
            reason,
        } => structured_error(
            500,
            json!({
                "code": "framework_process_io_error",
                "message": reason,
                "tool_id": id,
                "framework": framework,
            }),
        ),
        ToolRegistryError::FrameworkProcessProtocol {
            id,
            framework,
            reason,
        } => structured_error(
            502,
            json!({
                "code": "framework_process_protocol_error",
                "message": reason,
                "tool_id": id,
                "framework": framework,
            }),
        ),
        ToolRegistryError::FrameworkProcessFailed {
            id,
            framework,
            code,
            message,
            detail,
        } => structured_error(
            500,
            json!({
                "code": "framework_execution_error",
                "message": message,
                "detail": detail,
                "frameworkCode": code,
                "tool_id": id,
                "framework": framework,
            }),
        ),
        ToolRegistryError::Io(error) => structured_error(
            500,
            json!({
                "code": "tool_registry_error",
                "message": error.to_string(),
            }),
        ),
        ToolRegistryError::Json(error) => structured_error(
            500,
            json!({
                "code": "tool_registry_error",
                "message": error.to_string(),
            }),
        ),
        ToolRegistryError::ArtSettings(error) => structured_error(
            500,
            json!({
                "code": "art_settings_error",
                "message": error.to_string(),
            }),
        ),
    }
}
