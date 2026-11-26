// Scoped tool execution and live-screen resource handlers for Loom's outbound MCP server.

fn mcp_text_result(value: Value) -> Value {
    json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string(&value).unwrap_or_default()
        }],
        "structuredContent": value,
        "isError": false
    })
}

fn handle_outbound_mcp_tool_call(
    runtime: &DaemonRuntime,
    grant: &mcp_share_tokens::McpShareGrant,
    params: &Value,
) -> std::result::Result<Value, (i32, String)> {
    use mcp_share_tokens::McpShareScope;
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| (-32602, "tools/call requires params.name".to_owned()))?;
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    if name == "loom_art_execute" || name.starts_with("art__") {
        if !grant.allows(McpShareScope::ArtsExecute) {
            return Err((-32003, "MCP token lacks scope `arts:execute`".to_owned()));
        }
        let target_art_id = if name == "loom_art_execute" {
            args.get("artId")
                .and_then(Value::as_str)
                .ok_or_else(|| (-32602, "artId is required".to_owned()))?
                .to_owned()
        } else {
            let suffix = &name["art__".len()..];
            runtime
                .tool_registry
                .list_tools()
                .unwrap_or_default()
                .into_iter()
                .find(|t| t.enabled && sanitize_mcp_tool_suffix(&t.qualified_id()) == suffix)
                .map(|t| t.qualified_id())
                .ok_or_else(|| (-32602, format!("enabled Art `{name}` not found")))?
        };
        let exec_args = if name == "loom_art_execute" {
            args.get("arguments").cloned().unwrap_or_else(|| json!({}))
        } else {
            args
        };
        let body = serde_json::to_string(&json!({ "arguments": exec_args }))
            .map_err(|e| (-32603, e.to_string()))?;
        let (status, resp) = execute_registered_tool(
            &target_art_id,
            &body,
            &runtime.mcp_servers,
            &runtime.tool_registry,
            &runtime.workflow_store,
            &runtime.framework_registry,
            &runtime.run_store,
            &runtime.control_plane_root,
            &runtime.shutting_down,
            runtime.request_executor_status.mode != "inline",
        )
        .map_err(|e| (-32000, e.to_string()))?;
        let parsed: Value = serde_json::from_str(&resp).unwrap_or_else(|_| json!({ "raw": resp }));
        if status != 200 {
            return Err((-32000, parsed.to_string()));
        }
        return Ok(mcp_text_result(parsed));
    }

    match name {
        "loom_ai_chat" => {
            if !grant.allows(McpShareScope::CapabilitiesInvoke) {
                return Err((
                    -32003,
                    "MCP token lacks scope `capabilities:invoke`".to_owned(),
                ));
            }
            let prompt = args
                .get("prompt")
                .and_then(Value::as_str)
                .ok_or_else(|| (-32602, "prompt is required".to_owned()))?;
            let chat_body = json!({
                "model": args.get("model").and_then(Value::as_str),
                "messages": [{ "role": "user", "content": prompt }]
            });
            let completion = runtime
                .ai_gateway
                .chat_completion(chat_body)
                .map_err(|e| (-32000, e.to_string()))?;
            Ok(mcp_text_result(completion.response))
        }
        "loom_capability_invoke" => {
            if !grant.allows(McpShareScope::CapabilitiesInvoke) {
                return Err((
                    -32003,
                    "MCP token lacks scope `capabilities:invoke`".to_owned(),
                ));
            }
            let capability = args
                .get("capability")
                .and_then(Value::as_str)
                .ok_or_else(|| (-32602, "capability is required".to_owned()))?;
            let body = serde_json::to_string(&json!({
                "requestId": format!("mcp:{}", Uuid::new_v4()),
                "caller": format!("mcp-share:{}", grant.token_id),
                "capability": capability,
                "input": args.get("input").cloned().unwrap_or_else(|| json!({})),
            }))
            .map_err(|e| (-32603, e.to_string()))?;
            let (status, resp) = invoke_capability(
                &body,
                &runtime.run_store,
                &runtime.ai_gateway,
                &runtime.capability_dispatch,
                &runtime.capability_resources,
                &runtime.surface_resources,
                &runtime.control_plane_root,
            )
            .map_err(|e| (-32000, e.to_string()))?;
            let parsed: Value = serde_json::from_str(&resp).unwrap_or(Value::Null);
            if status >= 400 {
                return Err((-32000, parsed.to_string()));
            }
            Ok(mcp_text_result(parsed))
        }
        "loom_live_sessions_list" => {
            if !grant.allows(McpShareScope::ScreenRead) {
                return Err((-32003, "MCP token lacks scope `screen:read`".to_owned()));
            }
            Ok(mcp_text_result(snapshot_live_sessions_summary(
                &runtime.live_sessions,
            )))
        }
        "loom_live_frame_read" => {
            if !grant.allows(McpShareScope::ScreenRead) {
                return Err((-32003, "MCP token lacks scope `screen:read`".to_owned()));
            }
            let session_id = args
                .get("sessionId")
                .and_then(Value::as_str)
                .ok_or_else(|| (-32602, "sessionId is required".to_owned()))?;
            let frame = snapshot_live_latest_frame(&runtime.live_sessions, session_id).ok_or_else(
                || {
                    (
                        -32004,
                        format!("no buffered frame for session `{session_id}`"),
                    )
                },
            )?;
            Ok(mcp_text_result(frame))
        }
        "loom_devices_hierarchy" => {
            if !grant.allows(McpShareScope::DevicesControl) {
                return Err((-32003, "MCP token lacks scope `devices:control`".to_owned()));
            }
            let (_, body) = build_devices_hierarchy_response(
                &runtime.device_registry,
                &runtime.hook_bridge,
                &runtime.control_plane_root,
            )
            .map_err(|e| (-32000, e.to_string()))?;
            Ok(mcp_text_result(
                serde_json::from_str(&body).unwrap_or(Value::Null),
            ))
        }
        "loom_otter_sync_status" => {
            if !grant.allows(McpShareScope::DevicesControl) {
                return Err((-32003, "MCP token lacks scope `devices:control`".to_owned()));
            }
            Ok(mcp_text_result(otter_sync::summary_snapshot(
                &runtime.control_plane_root,
            )))
        }
        "loom_otter_workflow_restore" => {
            if !grant.allows(McpShareScope::DevicesControl) {
                return Err((-32003, "MCP token lacks scope `devices:control`".to_owned()));
            }
            let entry_id = args
                .get("entryId")
                .and_then(Value::as_str)
                .ok_or_else(|| (-32602, "entryId is required".to_owned()))?;
            let restored = otter_sync::restore_history_entry(
                &runtime.control_plane_root,
                entry_id,
                &runtime.workflow_store,
                &runtime.hook_bridge,
            )
            .map_err(|(_, msg)| (-32000, msg))?;
            Ok(mcp_text_result(restored))
        }
        "loom_walls_state" => {
            if !grant.allows(McpShareScope::ProjectionControl) {
                return Err((
                    -32003,
                    "MCP token lacks scope `projection:control`".to_owned(),
                ));
            }
            let view = runtime
                .walls
                .snapshot(None)
                .map_err(|e| (-32000, e.message.to_owned()))?;
            Ok(mcp_text_result(
                serde_json::to_value(view).unwrap_or(Value::Null),
            ))
        }
        _ => Err((-32601, format!("unknown MCP tool `{name}`"))),
    }
}

fn build_outbound_mcp_resources(
    runtime: &DaemonRuntime,
    grant: &mcp_share_tokens::McpShareGrant,
) -> Vec<Value> {
    if !grant.allows(mcp_share_tokens::McpShareScope::ScreenRead) {
        return Vec::new();
    }
    let Ok(sessions) = runtime.live_sessions.state.lock() else {
        return Vec::new();
    };
    sessions
        .iter()
        .filter(|(_, record)| !record.closed)
        .map(|(session_id, record)| {
            json!({
                "uri": format!("loom://live-sessions/{session_id}/latest-frame"),
                "name": format!("Live Screen ({})", record.session.source_device_id),
                "mimeType": "application/json",
                "description": format!("Buffered frames: {}", record.frames.len())
            })
        })
        .collect()
}

fn handle_outbound_mcp_resource_read(
    runtime: &DaemonRuntime,
    grant: &mcp_share_tokens::McpShareGrant,
    params: &Value,
) -> std::result::Result<Value, (i32, String)> {
    if !grant.allows(mcp_share_tokens::McpShareScope::ScreenRead) {
        return Err((-32003, "MCP token lacks scope `screen:read`".to_owned()));
    }
    let uri = params
        .get("uri")
        .and_then(Value::as_str)
        .ok_or_else(|| (-32602, "resources/read requires params.uri".to_owned()))?;
    let session_id = uri
        .strip_prefix("loom://live-sessions/")
        .and_then(|rest| rest.strip_suffix("/latest-frame"))
        .ok_or_else(|| (-32602, format!("unsupported resource uri `{uri}`")))?;
    let frame = snapshot_live_latest_frame(&runtime.live_sessions, session_id)
        .ok_or_else(|| (-32004, format!("no buffered frame for `{session_id}`")))?;
    Ok(json!({
        "contents": [{
            "uri": uri,
            "mimeType": "application/json",
            "text": serde_json::to_string(&frame).unwrap_or_default()
        }]
    }))
}

fn snapshot_live_sessions_summary(live_sessions: &SharedLiveSessionStore) -> Value {
    let Ok(sessions) = live_sessions.state.lock() else {
        return json!({ "sessions": [] });
    };
    let list = sessions
        .iter()
        .filter(|(_, record)| !record.closed)
        .map(|(id, record)| {
            json!({
                "sessionId": id,
                "sourceDeviceId": record.session.source_device_id,
                "sourceConnected": record.source_connected,
                "lastFrameId": record.last_frame_id,
                "bufferedFrames": record.frames.len(),
            })
        })
        .collect::<Vec<_>>();
    json!({ "sessions": list })
}

fn snapshot_live_latest_frame(
    live_sessions: &SharedLiveSessionStore,
    session_id: &str,
) -> Option<Value> {
    let sessions = live_sessions.state.lock().ok()?;
    let record = sessions.get(session_id)?;
    let frame = record.frames.back()?;
    let (epoch, meta) = LiveBinaryFrame::decode_header(&frame.bytes).ok()?;
    let payload_b64 = BASE64.encode(frame.bytes.get(64..).unwrap_or_default());
    Some(json!({
        "sessionId": session_id,
        "sourceDeviceId": record.session.source_device_id,
        "epoch": epoch,
        "frameId": meta.frame_id,
        "width": meta.width,
        "height": meta.height,
        "codec": meta.codec,
        "keyframe": meta.keyframe,
        "payloadBase64": payload_b64,
    }))
}
