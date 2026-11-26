// Outbound Loom-as-an-MCP-Server (`POST /mcp` JSON-RPC 2.0 protocol + tool catalog).

const OUTBOUND_MCP_PROTOCOL_VERSION: &str = "2025-06-18";

fn is_outbound_mcp_request(request: &ParsedHttpRequest) -> bool {
    let route_path = request
        .path
        .split('?')
        .next()
        .unwrap_or(request.path.as_str());
    route_path == "/mcp"
}

fn has_scoped_mcp_bearer_header(request: &ParsedHttpRequest) -> bool {
    request
        .authorization_credential("Bearer")
        .is_some_and(mcp_share_tokens::is_mcp_share_token_format)
}

fn route_outbound_mcp_http(
    runtime: &DaemonRuntime,
    request: &ParsedHttpRequest,
) -> Option<Result<(u16, String)>> {
    if !is_outbound_mcp_request(request) {
        return None;
    }
    if request.method != "POST" {
        return Some(structured_error(
            405,
            json!({ "code": "method_not_allowed", "message": "MCP endpoint requires POST" }),
        ));
    }
    let grant = if request.has_admin_credential(&runtime.auth_token) {
        mcp_share_tokens::McpShareGrant::admin_all()
    } else if let Some(bearer) = request.authorization_credential("Bearer") {
        let Some(grant) =
            mcp_share_tokens::authenticate_share_token(&runtime.control_plane_root, bearer)
        else {
            return Some(structured_error(
                401,
                json!({ "code": "invalid_mcp_share_token", "message": "invalid or expired Loom MCP share token" }),
            ));
        };
        grant
    } else {
        return Some(structured_error(
            401,
            json!({ "code": "unauthorized", "message": "missing Loom MCP share token" }),
        ));
    };

    Some(dispatch_mcp_share_jsonrpc(runtime, &grant, &request.body))
}

pub(crate) fn dispatch_mcp_share_jsonrpc(
    runtime: &DaemonRuntime,
    grant: &mcp_share_tokens::McpShareGrant,
    raw_body: &str,
) -> Result<(u16, String)> {
    let payload: Value = match serde_json::from_str(raw_body) {
        Ok(value) => value,
        Err(_) => {
            return Ok((
                200,
                serde_json::to_string(&json!({
                    "jsonrpc": "2.0",
                    "id": Value::Null,
                    "error": { "code": -32700, "message": "invalid JSON-RPC payload" }
                }))?,
            ))
        }
    };
    let id = payload.get("id").cloned().unwrap_or(Value::Null);
    let Some(method) = payload.get("method").and_then(Value::as_str) else {
        return Ok((
            200,
            serde_json::to_string(&json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32600, "message": "invalid JSON-RPC request" }
            }))?,
        ));
    };

    if method == "notifications/initialized" || method.starts_with("notifications/") {
        return Ok((202, String::new()));
    }

    let outcome = match method {
        "initialize" => {
            let requested = payload
                .pointer("/params/protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or(OUTBOUND_MCP_PROTOCOL_VERSION);
            let negotiated = if matches!(requested, "2026-07-28" | "2025-06-18" | "2024-11-05") {
                requested
            } else {
                OUTBOUND_MCP_PROTOCOL_VERSION
            };
            Ok(json!({
                "protocolVersion": negotiated,
                "capabilities": {
                    "tools": { "listChanged": false },
                    "resources": { "listChanged": false }
                },
                "serverInfo": {
                    "name": "loom-hub-mcp",
                    "version": env!("CARGO_PKG_VERSION"),
                    "grantId": &grant.token_id
                }
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": build_outbound_mcp_tools(runtime, grant) })),
        "tools/call" => {
            let params = payload.get("params").cloned().unwrap_or_else(|| json!({}));
            handle_outbound_mcp_tool_call(runtime, grant, &params)
        }
        "resources/list" => Ok(json!({
            "resources": build_outbound_mcp_resources(runtime, grant)
        })),
        "resources/read" => {
            let params = payload.get("params").cloned().unwrap_or_else(|| json!({}));
            handle_outbound_mcp_resource_read(runtime, grant, &params)
        }
        _ => Err((-32601, format!("method not found: {method}"))),
    };

    let response = match outcome {
        Ok(result) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result,
        }),
        Err((code, message)) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": code, "message": message },
        }),
    };
    Ok((200, serde_json::to_string(&response)?))
}

fn sanitize_mcp_tool_suffix(raw: &str) -> String {
    // Qualified identity is authoritative; lossy truncation aliases unrelated Arts.
    use sha2::Digest;
    format!("{:x}", sha2::Sha256::digest(raw.as_bytes()))
}

fn build_outbound_mcp_tools(
    runtime: &DaemonRuntime,
    grant: &mcp_share_tokens::McpShareGrant,
) -> Vec<Value> {
    use mcp_share_tokens::McpShareScope;
    let mut tools = Vec::new();

    if grant.allows(McpShareScope::ArtsExecute) {
        tools.push(json!({
            "name": "loom_art_execute",
            "description": "Execute any enabled Loom Art or composite Workflow Art (supports Loom <-> Otter hybrid relay).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "artId": { "type": "string", "description": "Tool ID or publisher-qualified Art ID" },
                    "arguments": { "type": "object", "description": "Arguments passed to the Art" }
                },
                "required": ["artId"]
            }
        }));
        for tool in runtime.tool_registry.list_tools().unwrap_or_default() {
            if !tool.enabled {
                continue;
            }
            tools.push(json!({
                "name": format!("art__{}", sanitize_mcp_tool_suffix(&tool.qualified_id())),
                "description": format!("[Loom Art {}] {}", tool.qualified_id(), tool.description),
                "inputSchema": {
                    "type": "object",
                    "properties": {},
                    "additionalProperties": true
                }
            }));
        }
    }

    if grant.allows(McpShareScope::CapabilitiesInvoke) {
        tools.push(json!({
            "name": "loom_ai_chat",
            "description": "Invoke Loom's globally configured AI Gateway chat completion.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "prompt": { "type": "string" },
                    "model": { "type": "string" }
                },
                "required": ["prompt"]
            }
        }));
        tools.push(json!({
            "name": "loom_capability_invoke",
            "description": "Invoke an installed Loom Capability Package command (e.g., OCR, translation).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "capability": { "type": "string" },
                    "input": { "type": "object" }
                },
                "required": ["capability"]
            }
        }));
    }

    if grant.allows(McpShareScope::ScreenRead) {
        tools.push(json!({
            "name": "loom_live_sessions_list",
            "description": "List active paired Otter live screen sessions relayed by Loom.",
            "inputSchema": { "type": "object", "properties": {} }
        }));
        tools.push(json!({
            "name": "loom_live_frame_read",
            "description": "Read the latest buffered live screen frame metadata and base64 payload for a session.",
            "inputSchema": {
                "type": "object",
                "properties": { "sessionId": { "type": "string" } },
                "required": ["sessionId"]
            }
        }));
    }

    if grant.allows(McpShareScope::DevicesControl) {
        tools.push(json!({
            "name": "loom_devices_hierarchy",
            "description": "Read Loom's two-level Host Device -> App Sessions hierarchy.",
            "inputSchema": { "type": "object", "properties": {} }
        }));
        tools.push(json!({
            "name": "loom_otter_sync_status",
            "description": "Inspect live Otter execution status across LAN devices and archived workflow history.",
            "inputSchema": { "type": "object", "properties": {} }
        }));
        tools.push(json!({
            "name": "loom_otter_workflow_restore",
            "description": "Restore an archived Otter workflow and broadcast instantiation to LAN Otter terminals.",
            "inputSchema": {
                "type": "object",
                "properties": { "entryId": { "type": "string" } },
                "required": ["entryId"]
            }
        }));
    }

    if grant.allows(McpShareScope::ProjectionControl) {
        tools.push(json!({
            "name": "loom_walls_state",
            "description": "Inspect Loom display wall layouts, tiles, and connected wall endpoints.",
            "inputSchema": { "type": "object", "properties": {} }
        }));
    }

    tools
}
