// Canvas previews, workflow listing and mutation, bridge sessions, and snapshots.
fn sanitize_preview_file_stem(node_id: &str) -> String {
    let mut stem = String::with_capacity(node_id.len());
    for ch in node_id.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            stem.push(ch);
        } else {
            stem.push('_');
        }
    }
    if stem.is_empty() {
        "node".to_owned()
    } else {
        stem
    }
}

fn percent_encode_path_segment(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(char::from(byte));
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn list_canvas_workflows(canvas_workflow_root: &Path) -> Result<(u16, String)> {
    let mut workflows: Vec<Value> = Vec::new();
    if let Ok(entries) = fs::read_dir(canvas_workflow_root) {
        for entry in entries.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            let meta_path = entry.path().join("meta.json");
            if let Ok(text) = fs::read_to_string(&meta_path) {
                if let Ok(meta) = serde_json::from_str::<Value>(&text) {
                    workflows.push(meta);
                }
            }
        }
    }
    workflows.sort_by(|a, b| {
        // savedAt is written as epoch-millis u64; sort newest first.
        let sa = a.get("savedAt").and_then(Value::as_u64).unwrap_or(0);
        let sb = b.get("savedAt").and_then(Value::as_u64).unwrap_or(0);
        sb.cmp(&sa)
    });
    Ok((
        200,
        serde_json::to_string(&json!({ "workflows": workflows }))?,
    ))
}

fn get_canvas_workflow_snapshot(
    path_id: &str,
    canvas_workflow_root: &Path,
) -> Result<(u16, String)> {
    let Some(workflow_dir) = canvas_workflow_dir(canvas_workflow_root, path_id) else {
        return invalid_request("invalid workflow id");
    };
    match fs::read_to_string(workflow_dir.join("snapshot.json")) {
        Ok(json) => Ok((200, json)),
        Err(_) => structured_error(
            404,
            json!({
                "code": "canvas_workflow_not_found",
                "message": "Canvas workflow was not found",
            }),
        ),
    }
}

fn canvas_workflow_preview_response(
    workflow_id: &str,
    node_id: &str,
    canvas_workflow_root: &Path,
) -> Result<RouteResponse> {
    let Some(workflow_dir) = canvas_workflow_dir(canvas_workflow_root, workflow_id) else {
        return structured_error(
            400,
            json!({ "code": "invalid_request", "message": "invalid workflow id" }),
        )
        .map(|(status, body)| RouteResponse::Text { status, body });
    };
    let images_dir = workflow_dir.join("images");
    let Ok(canonical_images) = fs::canonicalize(&images_dir) else {
        return structured_error(
            404,
            json!({ "code": "preview_not_found", "message": "Hook canvas preview was not found" }),
        )
        .map(|(status, body)| RouteResponse::Text { status, body });
    };
    let stem = sanitize_preview_file_stem(node_id);
    // Try known extensions for this node stem.
    for ext in ["png", "jpg", "jpeg", "webp"] {
        let candidate = images_dir.join(format!("{stem}.{ext}"));
        let Ok(canonical) = fs::canonicalize(&candidate) else {
            continue;
        };
        if !canonical.starts_with(&canonical_images) || !canonical.is_file() {
            continue;
        }
        if let Ok(bytes) = fs::read(&canonical) {
            return hook_canvas_preview_binary_response(bytes);
        }
    }
    structured_error(
        404,
        json!({ "code": "preview_not_found", "message": "Hook canvas preview was not found" }),
    )
    .map(|(status, body)| RouteResponse::Text { status, body })
}

fn delete_canvas_workflow(path_id: &str, canvas_workflow_root: &Path) -> Result<(u16, String)> {
    let Some(workflow_dir) = canvas_workflow_dir(canvas_workflow_root, path_id) else {
        return invalid_request("invalid workflow id");
    };
    if !workflow_dir.is_dir() {
        return structured_error(
            404,
            json!({
                "code": "canvas_workflow_not_found",
                "message": "Canvas workflow was not found",
            }),
        );
    }
    if fs::remove_dir_all(&workflow_dir).is_err() {
        return structured_error(
            500,
            json!({
                "code": "canvas_workflow_delete_failed",
                "message": "Unable to delete canvas workflow",
            }),
        );
    }
    Ok((
        200,
        serde_json::to_string(&json!({ "workflowId": path_id, "deleted": true }))?,
    ))
}

// Rename a frozen canvas workflow by updating the `name` field in its meta.json.
// The id (directory name) is stable; only the display name changes.
fn rename_canvas_workflow(
    path_id: &str,
    body: &str,
    canvas_workflow_root: &Path,
) -> Result<(u16, String)> {
    let request = match serde_json::from_str::<RenameCanvasWorkflowRequest>(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let name = request.name.trim();
    if name.is_empty() {
        return invalid_request("name is required");
    }
    let Some(workflow_dir) = canvas_workflow_dir(canvas_workflow_root, path_id) else {
        return invalid_request("invalid workflow id");
    };
    let meta_path = workflow_dir.join("meta.json");
    let Ok(text) = fs::read_to_string(&meta_path) else {
        return structured_error(
            404,
            json!({
                "code": "canvas_workflow_not_found",
                "message": "Canvas workflow was not found",
            }),
        );
    };
    let mut meta = match serde_json::from_str::<Value>(&text) {
        Ok(meta) => meta,
        Err(_) => json!({ "id": path_id }),
    };
    if let Some(object) = meta.as_object_mut() {
        object.insert("name".to_owned(), json!(name));
    }
    if fs::write(&meta_path, meta.to_string()).is_err() {
        return structured_error(
            500,
            json!({
                "code": "canvas_workflow_rename_failed",
                "message": "Unable to rename canvas workflow",
            }),
        );
    }
    Ok((200, serde_json::to_string(&meta)?))
}

fn delete_workflow(path_id: &str, workflow_store: &WorkflowStore) -> Result<(u16, String)> {
    if let Err(error) = workflow_store.load_workflow(path_id) {
        return workflow_store_error_response(error);
    }
    if let Err(error) = workflow_store.delete_workflow(path_id) {
        return workflow_store_error_response(error);
    }

    Ok((
        200,
        serde_json::to_string(&json!({ "workflowId": path_id, "deleted": true }))?,
    ))
}

fn hook_bridge_status(hook_bridge: &SharedHookBridgeRuntime) -> Result<(u16, String)> {
    let runtime = hook_bridge
        .lock()
        .map_err(|_| anyhow::anyhow!("lock hook bridge runtime"))?;
    Ok((
        200,
        serde_json::to_string(&hook_bridge_status_json(&runtime))?,
    ))
}

fn instantiate_hook_workflow(
    body: &str,
    hook_bridge: &SharedHookBridgeRuntime,
) -> Result<(u16, String)> {
    let request_body = if body.trim().is_empty() { "{}" } else { body };
    let request: HookWorkflowInstantiateHttpRequest = match serde_json::from_str(request_body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let mode = if request.mode.trim().is_empty() {
        "reference".to_owned()
    } else {
        request.mode.trim().to_owned()
    };
    let (workflow_root, hub) = {
        let runtime = hook_bridge
            .lock()
            .map_err(|_| anyhow::anyhow!("lock hook bridge runtime"))?;
        (runtime.workflow_root.clone(), runtime.broadcast_hub.clone())
    };
    let event = instantiate_workflow(
        &workflow_root,
        request.nodes,
        request.edges,
        &mode,
        request.workflow_id,
    )
    .map_err(|error| anyhow::anyhow!("instantiate Hook workflow: {error}"))?;
    let serialized = serde_json::to_string(&event)?;
    let delivered_clients = broadcast_hook_bridge_messages_with_count(&hub, &[serialized]);
    if delivered_clients == 0 {
        return structured_error(
            409,
            json!({
                "code": "no_hook_client",
                "message": "No Hook client is connected to receive workflow instantiation",
            }),
        );
    }
    Ok((
        200,
        serde_json::to_string(&json!({
            "protocolVersion": loom_protocol::HOOK_PROTOCOL_VERSION,
            "status": "succeeded",
            "method": loom_protocol::HOOK_EVENT_WORKFLOW_INSTANTIATED,
            "broadcasted": true,
            "subscribedClients": delivered_clients,
            "deliveredClients": delivered_clients,
            "params": event.params,
        }))?,
    ))
}

fn update_hook_workflow_node(
    body: &str,
    hook_bridge: &SharedHookBridgeRuntime,
) -> Result<(u16, String)> {
    let request_body = if body.trim().is_empty() { "{}" } else { body };
    let request: HookWorkflowNodeUpdateHttpRequest = match serde_json::from_str(request_body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let workflow_id = request.workflow_id.trim();
    let node_id = request.node_id.trim();
    let param = request.param.trim();
    if workflow_id.is_empty() {
        return invalid_request("update_workflow_node requires workflow_id");
    }
    if node_id.is_empty() {
        return invalid_request("update_workflow_node requires node_id");
    }
    if param.is_empty() {
        return invalid_request("update_workflow_node requires param");
    }

    let (workflow_root, broadcast_hub) = {
        let runtime = hook_bridge
            .lock()
            .map_err(|_| anyhow::anyhow!("lock hook bridge runtime"))?;
        (runtime.workflow_root.clone(), runtime.broadcast_hub.clone())
    };
    if is_hook_live_workflow_id(workflow_id) {
        let mut patch = HookCanvasPersistPatch::default();
        patch
            .param_updates
            .push((param.to_owned(), request.value.clone()));
        if let Err(error) = persist_hook_canvas_live_node_patch(node_id, &patch) {
            let status = match &error {
                HookCanvasPersistError::RevisionConflict { .. }
                | HookCanvasPersistError::SnapshotUnavailable
                | HookCanvasPersistError::UnsupportedDocument(_) => 409,
                HookCanvasPersistError::NodeUnavailable(_) => 422,
                HookCanvasPersistError::SessionUnavailable(_)
                | HookCanvasPersistError::WriteFailed(_) => 500,
            };
            return structured_error(
                status,
                json!({
                    "code": error.code(),
                    "message": error.message(),
                    "refreshPath": "/v1/hook-bridge/session",
                    "retryable": true,
                }),
            );
        }
    }
    let event = match update_workflow_node(
        &workflow_root,
        workflow_id,
        node_id,
        param,
        request.value.clone(),
    ) {
        Ok(event) => event,
        Err(error) => {
            return structured_error(
                422,
                json!({
                    "code": "workflow_update_failed",
                    "message": error.to_string(),
                }),
            )
        }
    };

    broadcast_hook_bridge_messages(&broadcast_hub, &[serde_json::to_string(&event)?]);
    Ok((
        200,
        serde_json::to_string(&json!({
            "protocolVersion": loom_protocol::HOOK_PROTOCOL_VERSION,
            "status": "succeeded",
            "workflowId": workflow_id,
            "nodeId": node_id,
            "parameterId": param,
            "value": request.value,
        }))?,
    ))
}

fn hook_bridge_session(hook_bridge: &SharedHookBridgeRuntime) -> Result<(u16, String)> {
    let runtime = hook_bridge
        .lock()
        .map_err(|_| anyhow::anyhow!("lock hook bridge runtime"))?;
    let (session_path, available, session, error) = read_hook_session_snapshot();
    Ok((
        200,
        serde_json::to_string(&json!({
            "running": runtime.worker.is_some(),
            "port": runtime.port.unwrap_or(HOOK_BRIDGE_PORT),
            "connectedClients": runtime.connected_clients.load(Ordering::SeqCst),
            "subscribedClients": runtime.broadcast_hub.subscriber_count(),
            "protocolVersion": loom_protocol::HOOK_PROTOCOL_VERSION,
            "sessionPath": session_path.to_string_lossy(),
            "available": available,
            "error": error,
            "session": session,
        }))?,
    ))
}

fn hook_canvas_snapshot() -> Result<(u16, String)> {
    match load_active_hook_canvas_document() {
        Ok(document) => Ok((200, serde_json::to_string(&document.snapshot)?)),
        Err(error) => {
            eprintln!("loom Hook canvas snapshot failed: {error:#}");
            structured_error(
                500,
                json!({
                    "code": "hook_canvas_error",
                    "message": "Hook canvas snapshot is temporarily unavailable",
                }),
            )
        }
    }
}
