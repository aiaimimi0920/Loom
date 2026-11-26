// End-to-end Milestone M4 integration tests:
// 1. Outbound Loom MCP Server (`POST /mcp`) + Scoped MCP Share Tokens + Public/Tunnel Host support
// 2. Two-level Host -> App Sessions Device Hierarchy (`GET /v1/devices/hierarchy`)
// 3. Multi-device Otter Sync live state + Workflow History Archive & Restore
// 4. NAccount 3-tier Internet Cloud Relay (`Otter -> Loom 1 -> Cloud Relay -> Loom 2 -> Otter` + MCP tunnel)
// 5. Schema-driven Companion App Settings Hub (`/v1/configuration/companion-apps/*`)

#[test]
fn m4_outbound_mcp_scoped_tokens_cloud_relay_otter_sync_and_companion_settings() {
    let root = unique_temp_dir("m4-hub-integration");
    let server = EditionTestDaemon::start(
        DaemonConfig::localhost(0)
            .with_control_plane_root(&root)
            .with_bearer_token("m4-admin-secret")
            .with_bounded_request_executor(4, 16),
    );
    let port = server.port;

    let http_json = |method: &str,
                     path: &str,
                     token: &str,
                     host: &str,
                     body: Option<&Value>|
     -> (u16, Value) {
        let payload = body
            .map(|v| serde_json::to_string(v).unwrap())
            .unwrap_or_default();
        let content_headers = if body.is_some() {
            format!(
                "Content-Type: application/json\r\nContent-Length: {}\r\n",
                payload.len()
            )
        } else {
            String::new()
        };
        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        let raw = format!(
            "{method} {path} HTTP/1.1\r\nHost: {host}\r\nAuthorization: Bearer {token}\r\n{content_headers}Connection: close\r\n\r\n{payload}"
        );
        stream.write_all(raw.as_bytes()).expect("write");
        let mut out = String::new();
        stream.read_to_string(&mut out).expect("read");
        let (head, resp_body) = out.split_once("\r\n\r\n").expect("split http");
        let status = head
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(0);
        let parsed = serde_json::from_str(resp_body).unwrap_or(Value::Null);
        (status, parsed)
    };

    let localhost = format!("127.0.0.1:{port}");

    // 1. Register an enabled Workflow Art to expose via Outbound MCP.
    assert_eq!(
        http_json(
            "PUT",
            "/v1/workflows/echo_m4",
            "m4-admin-secret",
            &localhost,
            Some(&json!({ "data": "nodes: []\n" }))
        )
        .0,
        200
    );
    assert_eq!(
        http_json(
            "PUT",
            "/v1/tools/m4-demo-art",
            "m4-admin-secret",
            &localhost,
            Some(&json!({
                "id": "m4-demo-art",
                "name": "M4 Demo Art",
                "description": "Callable over Loom MCP",
                "enabled": true,
                "execution": { "type": "workflow", "workflowId": "echo_m4" }
            }))
        )
        .0,
        200
    );

    // 2. Multi-device Otter Sync + Workflow Archive + Two-Level Device Hierarchy.
    assert_eq!(
        http_json(
            "POST",
            "/v1/otter-sync/devices/device-000-local/state",
            "m4-admin-secret",
            &localhost,
            Some(&json!({
                "executionState": "running",
                "workflowId": "echo_m4",
                "workflowName": "Home Automation Flow",
                "nodeCount": 3,
                "progress": 0.5
            }))
        )
        .0,
        200
    );
    let (arch_status, arch_body) = http_json(
        "POST",
        "/v1/otter-sync/history/archive",
        "m4-admin-secret",
        &localhost,
        Some(&json!({
            "workflowId": "restored_flow",
            "name": "Reusable Portrait Pipeline",
            "data": "nodes: []\n",
            "tags": ["portrait", "preset"]
        })),
    );
    assert_eq!(arch_status, 201);
    let entry_id = arch_body["entry"]["entryId"].as_str().unwrap().to_owned();

    let (hier_status, hier_body) = http_json(
        "GET",
        "/v1/devices/hierarchy",
        "m4-admin-secret",
        &localhost,
        None,
    );
    assert_eq!(hier_status, 200);
    assert_eq!(hier_body["model"], "host_to_apps");
    assert_eq!(
        hier_body["hosts"][0]["apps"][0]["executionState"],
        "offline"
    );
    assert_eq!(hier_body["hosts"][0]["apps"][0]["connected"], false);
    assert_eq!(hier_body["hosts"][0]["terminalAgentSlotReady"], true);

    // 3. Issue Scoped MCP Share Token (`arts:execute` + `devices:control`, NO `screen:read`).
    let (tok_status, tok_body) = http_json(
        "POST",
        "/v1/mcp-share/tokens",
        "m4-admin-secret",
        &localhost,
        Some(&json!({
            "label": "External Agent",
            "scopes": ["arts:execute", "devices:control"]
        })),
    );
    assert_eq!(tok_status, 201);
    let share_token = tok_body["token"]
        .as_str()
        .expect("plaintext token")
        .to_owned();
    assert!(share_token.starts_with("loom_mcp_"));

    // 4. Call `POST /mcp` over a simulated public tunnel Host header (`loom-tunnel.example.com`)!
    let (init_status, init_resp) = http_json(
        "POST",
        "/mcp",
        &share_token,
        "loom-tunnel.example.com",
        Some(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": { "protocolVersion": "2025-06-18" }
        })),
    );
    assert_eq!(init_status, 200);
    assert_eq!(init_resp["result"]["serverInfo"]["name"], "loom-hub-mcp");

    let (_, list_resp) = http_json(
        "POST",
        "/mcp",
        &share_token,
        "loom-tunnel.example.com",
        Some(&json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" })),
    );
    let tool_names = list_resp["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect::<Vec<_>>();
    let art_name = tool_names
        .iter()
        .find(|name| name.starts_with("art__"))
        .expect("dynamic Art tool");
    assert_eq!(
        tool_names
            .iter()
            .filter(|name| name.starts_with("art__"))
            .count(),
        1
    );
    assert!(tool_names.contains(&"loom_otter_workflow_restore"));
    assert!(!tool_names.contains(&"loom_live_frame_read"));

    // Calling `loom_live_frame_read` without `screen:read` scope must fail closed (-32003)!
    let (_, denied_resp) = http_json(
        "POST",
        "/mcp",
        &share_token,
        "loom-tunnel.example.com",
        Some(&json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": { "name": "loom_live_frame_read", "arguments": { "sessionId": "s1" } }
        })),
    );
    assert_eq!(denied_resp["error"]["code"], -32003);

    // Invoke the catalog's collision-resistant Art name and the workflow restore tool.
    let (_, call_art_resp) = http_json(
        "POST",
        "/mcp",
        &share_token,
        "loom-tunnel.example.com",
        Some(&json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": { "name": art_name, "arguments": {} }
        })),
    );
    assert_eq!(call_art_resp["result"]["isError"], false);

    let (_, restore_resp) = http_json(
        "POST",
        "/mcp",
        &share_token,
        "loom-tunnel.example.com",
        Some(&json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "tools/call",
            "params": {
                "name": "loom_otter_workflow_restore",
                "arguments": { "entryId": entry_id }
            }
        })),
    );
    assert_eq!(
        restore_resp["result"]["structuredContent"]["restored"],
        true
    );

    // 5. Configuration/login are not remote transport: never claim undelivered data succeeded.
    assert_eq!(
        http_json(
            "PUT",
            "/v1/cloud-relay/config",
            "m4-admin-secret",
            &localhost,
            Some(&json!({
                "enabled": true,
                "relayUrl": "https://relay.aiaimimi.com",
                "exposeMcp": true,
                "exposeProjectionRelay": true
            }))
        )
        .0,
        200
    );
    let (unsigned_relay_status, _) = http_json(
        "POST",
        "/v1/cloud-relay/projection/send",
        "m4-admin-secret",
        &localhost,
        Some(&json!({
            "targetLoomId": "loom-office",
            "sourceDeviceId": "device-home-1",
            "payload": { "kind": "frame_offer" }
        })),
    );
    assert_eq!(unsigned_relay_status, 401);

    fs::write(
        root.join("settings/test-naccount-session.json"),
        r#"{"accountId":"NU000000001"}"#,
    )
    .expect("seed test NAccount session");

    let (signed_relay_status, signed_relay_body) = http_json(
        "POST",
        "/v1/cloud-relay/projection/send",
        "m4-admin-secret",
        &localhost,
        Some(&json!({
            "sourceLoomId": "loom-home",
            "targetLoomId": "loom-office",
            "sourceDeviceId": "device-home-1",
            "targetDeviceId": "device-office-otter",
            "payload": { "kind": "frame_offer" }
        })),
    );
    assert_eq!(signed_relay_status, 501);
    assert_eq!(
        signed_relay_body["error"]["code"],
        "cloud_relay_transport_unavailable"
    );
    assert_eq!(cloud_relay::status_value(&root)["active"], false);
    assert_eq!(cloud_relay::status_value(&root)["queuedEnvelopes"], 0);

    let (relay_mcp_status, relay_mcp_body) = http_json(
        "POST",
        "/v1/cloud-relay/mcp",
        &share_token,
        &localhost,
        Some(&json!({ "jsonrpc": "2.0", "id": 9, "method": "ping" })),
    );
    assert_eq!(relay_mcp_status, 200);
    assert_eq!(relay_mcp_body["id"], 9);

    // 6. Companion App Settings Hub with schema + optimistic revision concurrency!
    assert_eq!(
        http_json(
            "PUT",
            "/v1/configuration/companion-apps/family-album/schema",
            "m4-admin-secret",
            &localhost,
            Some(&json!({
                "displayName": "Family Album Sync",
                "schemaVersion": 1,
                "sections": [],
                "defaultConfig": { "autoBackup": true, "maxConcurrency": 2 }
            }))
        )
        .0,
        200
    );
    let (upd_ok, upd_doc) = http_json(
        "PUT",
        "/v1/configuration/companion-apps/family-album",
        "m4-admin-secret",
        &localhost,
        Some(&json!({
            "expectedRevision": 1,
            "config": { "autoBackup": false, "maxConcurrency": 4 }
        })),
    );
    assert_eq!(upd_ok, 200);
    assert_eq!(upd_doc["document"]["revision"], 2);

    let (upd_conflict, _) = http_json(
        "PUT",
        "/v1/configuration/companion-apps/family-album",
        "m4-admin-secret",
        &localhost,
        Some(&json!({
            "expectedRevision": 1,
            "config": { "autoBackup": true }
        })),
    );
    assert_eq!(upd_conflict, 409);

    server.stop();
    let _ = fs::remove_dir_all(root);
}
