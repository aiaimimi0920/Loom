// Loom daemon tests fragment 17; included into the shared crate test module.
#[test]
fn daemon_hook_bridge_runtime_start_status_stop() {
    let root = unique_temp_dir("hook-bridge-runtime");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

    let stopped = expect_json_result_response(hook_bridge_status(&runtime.hook_bridge), 200);
    assert_eq!(stopped["running"], false);
    assert_eq!(stopped["connectedClients"], 0);

    let started = expect_json_result_response(
        start_hook_bridge(
            r#"{"port":0}"#,
            &runtime.hook_bridge,
            &runtime.mcp_servers,
            &runtime.tool_registry,
            &runtime.workflow_store,
            &runtime.settings,
            &runtime.shared_images,
            &runtime.ocr_provider,
            &runtime.framework_registry,
            &runtime.control_plane_root,
            &runtime.run_store,
            &runtime.surface_instances,
            &runtime.surface_actions,
        ),
        200,
    );
    assert_eq!(started["running"], true);
    assert!(started["port"].as_u64().expect("assigned bridge port") > 0);
    assert_eq!(started["connectedClients"], 0);
    assert_eq!(started["protocol"], loom_protocol::HOOK_PROTOCOL_VERSION);

    let running = expect_json_result_response(hook_bridge_status(&runtime.hook_bridge), 200);
    assert_eq!(running["running"], true);
    assert_eq!(running["port"], started["port"]);

    let duplicate_start = expect_json_result_response(
        start_hook_bridge(
            r#"{"port":0}"#,
            &runtime.hook_bridge,
            &runtime.mcp_servers,
            &runtime.tool_registry,
            &runtime.workflow_store,
            &runtime.settings,
            &runtime.shared_images,
            &runtime.ocr_provider,
            &runtime.framework_registry,
            &runtime.control_plane_root,
            &runtime.run_store,
            &runtime.surface_instances,
            &runtime.surface_actions,
        ),
        409,
    );
    assert_eq!(duplicate_start["error"]["code"], "hook_bridge_running");

    let stopped_again = expect_json_result_response(
        stop_hook_bridge(&runtime.hook_bridge, &runtime.shared_images),
        200,
    );
    assert_eq!(stopped_again["running"], false);
    assert_eq!(stopped_again["connectedClients"], 0);
    assert_eq!(stopped_again["port"], 19820);

    let final_status = expect_json_result_response(hook_bridge_status(&runtime.hook_bridge), 200);
    assert_eq!(final_status["running"], false);
    assert_eq!(final_status["port"], 19820);

    drop(runtime);
    fs::remove_dir_all(root).expect("cleanup hook bridge root");
}

#[test]
fn daemon_hook_bridge_accepts_websocket_handshake_request() {
    let root = unique_temp_dir("hook-bridge-handshake");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

    let started = expect_json_result_response(
        start_hook_bridge(
            r#"{"port":0}"#,
            &runtime.hook_bridge,
            &runtime.mcp_servers,
            &runtime.tool_registry,
            &runtime.workflow_store,
            &runtime.settings,
            &runtime.shared_images,
            &runtime.ocr_provider,
            &runtime.framework_registry,
            &runtime.control_plane_root,
            &runtime.run_store,
            &runtime.surface_instances,
            &runtime.surface_actions,
        ),
        200,
    );
    let bridge_port = started["port"].as_u64().expect("bridge port") as u16;
    let stream = TcpStream::connect(("127.0.0.1", bridge_port)).expect("connect bridge tcp socket");
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .expect("set websocket read timeout");
    stream
        .set_write_timeout(Some(Duration::from_secs(20)))
        .expect("set websocket write timeout");
    let (mut socket, _) = tungstenite::client(format!("ws://127.0.0.1:{bridge_port}"), stream)
        .expect("connect bridge websocket");

    socket
        .send(tungstenite::Message::Text(
            json!({
                "method": loom_protocol::HOOK_METHOD_HANDSHAKE,
                "params": {
                    "protocolVersion": loom_protocol::HOOK_PROTOCOL_VERSION,
                    "supportedProtocolVersions": [loom_protocol::HOOK_PROTOCOL_VERSION],
                    "clientId": "daemon-test",
                    "clientVersion": "0.4.2",
                    "platform": "windows",
                    "transports": ["websocket"],
                }
            })
            .to_string(),
        ))
        .expect("send handshake");
    let response = socket.read().expect("read handshake response");
    let response = response.into_text().expect("text response");
    let response: serde_json::Value = serde_json::from_str(&response).expect("response json");

    assert_eq!(
        response["protocolVersion"],
        loom_protocol::HOOK_PROTOCOL_VERSION
    );
    assert_eq!(response["serverVersion"], "0.1.0");
    assert!(response["sessionId"].as_str().is_some());

    let running = expect_json_result_response(hook_bridge_status(&runtime.hook_bridge), 200);
    assert_eq!(running["running"], true);
    assert!(
        running["connectedClients"]
            .as_u64()
            .expect("connected clients")
            >= 1
    );

    drop(socket);
    let stopped = expect_json_result_response(
        stop_hook_bridge(&runtime.hook_bridge, &runtime.shared_images),
        200,
    );
    assert_eq!(stopped["running"], false);

    drop(runtime);
    fs::remove_dir_all(root).expect("cleanup hook bridge root");
}

#[test]
fn daemon_hook_bridge_fans_out_broadcasts_to_subscribed_websocket_clients() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let root = unique_temp_dir("hook-bridge-fanout");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

    let started = start_test_hook_bridge(&runtime, r#"{"port":0}"#);
    let bridge_port = started["port"].as_u64().expect("bridge port") as u16;

    let mut subscriber = connect_hook_bridge_websocket(bridge_port);
    subscriber
        .send(tungstenite::Message::Text(
            json!({
                "method": loom_protocol::HOOK_METHOD_SUBSCRIBE,
                "params": {
                    "requestId": "subscribe-workflow",
                    "events": [loom_protocol::HOOK_EVENT_WORKFLOW_INSTANTIATED],
                }
            })
            .to_string(),
        ))
        .expect("send subscribe");
    let subscribe_response = read_hook_bridge_json(&mut subscriber);
    assert_eq!(subscribe_response["status"], "succeeded");

    let mut publisher = connect_hook_bridge_websocket(bridge_port);
    publisher
            .send(tungstenite::Message::Text(
                json!({
                    "method": loom_protocol::HOOK_METHOD_WORKFLOW_INSTANTIATE,
                    "params": {
                        "requestId": "instantiate-workflow",
                        "workflowId": "wf-broadcast",
                        "mode": "reference",
                        "nodes": [{"id":"prompt","type":"artNode","data":{"artId":"neuro.official/prompt"}}],
                        "edges": [{"source":"prompt","target":"out"}],
                    }
                })
                .to_string(),
            ))
            .expect("send instantiate workflow");
    let publish_response = read_hook_bridge_json(&mut publisher);
    assert_eq!(publish_response["status"], "succeeded");

    let broadcast = read_hook_bridge_json(&mut subscriber);
    assert_eq!(
        broadcast["method"],
        loom_protocol::HOOK_EVENT_WORKFLOW_INSTANTIATED
    );
    assert_eq!(broadcast["params"]["workflowId"], "wf-broadcast");
    assert_eq!(broadcast["params"]["nodes"][0]["id"], "prompt");
    assert_eq!(broadcast["params"]["edges"][0]["target"], "out");

    let running = hook_bridge_status_value(&runtime);
    assert!(
        running["subscribedClients"]
            .as_u64()
            .expect("subscribed clients")
            >= 1
    );

    drop(publisher);
    drop(subscriber);
    let stopped = stop_test_hook_bridge(&runtime);
    assert_eq!(stopped["running"], false);

    drop(runtime);
    fs::remove_dir_all(root).expect("cleanup Hook Bridge fanout root");
}

#[test]
fn daemon_hook_bridge_accepts_versioned_surface_subscriptions() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let root = unique_temp_dir("hook-bridge-surface-subscription");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

    let started = start_test_hook_bridge(&runtime, r#"{"port":0}"#);
    let bridge_port = started["port"].as_u64().expect("bridge port") as u16;
    let mut subscriber = connect_hook_bridge_websocket(bridge_port);
    subscriber
        .send(tungstenite::Message::Text(
            json!({
                "method": loom_protocol::HOOK_METHOD_SUBSCRIBE,
                "params": {
                    "requestId": "subscribe-surface",
                    "events": loom_protocol::SURFACE_EVENT_METHODS,
                }
            })
            .to_string(),
        ))
        .expect("send Surface subscribe");
    let response = read_hook_bridge_json(&mut subscriber);
    assert_eq!(response["status"], "succeeded");
    assert_eq!(
        response["data"]["events"][0],
        loom_protocol::SURFACE_EVENT_SNAPSHOT
    );

    drop(subscriber);
    let stopped = stop_test_hook_bridge(&runtime);
    assert_eq!(stopped["running"], false);
    drop(runtime);
    fs::remove_dir_all(root).expect("cleanup Surface subscription root");
}

#[test]
fn daemon_hook_bridge_rejects_legacy_surface_subscription_alias() {
    let root = unique_temp_dir("hook-bridge-legacy-surface-subscription");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
    let response = run_hook_bridge_text(
        &runtime,
        &json!({
            "method": loom_protocol::HOOK_METHOD_SUBSCRIBE,
            "params": {
                "requestId": "subscribe-legacy-surface",
                "events": ["surface"],
            }
        })
        .to_string(),
    );
    assert_eq!(response["status"], "failed");
    assert_eq!(response["error"]["code"], "unsupported_event");
    drop(runtime);
    fs::remove_dir_all(root).expect("cleanup legacy Surface subscription root");
}

#[test]
fn daemon_hook_bridge_filters_broadcasts_by_subscribed_channel() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let root = unique_temp_dir("hook-bridge-channel-filter");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

    let started = start_test_hook_bridge(&runtime, r#"{"port":0}"#);
    let bridge_port = started["port"].as_u64().expect("bridge port") as u16;

    let mut subscriber = connect_hook_bridge_websocket(bridge_port);
    subscriber
        .send(tungstenite::Message::Text(
            json!({
                "method": loom_protocol::HOOK_METHOD_SUBSCRIBE,
                "params": {
                    "requestId": "subscribe-workflow-only",
                    "events": [loom_protocol::HOOK_EVENT_WORKFLOW_INSTANTIATED],
                }
            })
            .to_string(),
        ))
        .expect("send subscribe");
    let subscribe_response = read_hook_bridge_json(&mut subscriber);
    assert_eq!(subscribe_response["status"], "succeeded");

    let saved_tool = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request(
                "PUT",
                "/v1/tools/filter-art",
                &[],
                Some(
                    r#"{"id":"filter-art","name":"Filter Art","description":"channel filter fixture","enabled":true,"execution":{"type":"framework_art","framework":"process"},"params":[{"id":"strength","default":0.1}]}"#,
                ),
            ),
        ),
        200,
    );
    assert_eq!(saved_tool["tool"]["id"], "filter-art");

    subscriber
        .get_mut()
        .set_read_timeout(Some(Duration::from_millis(400)))
        .expect("shrink websocket read timeout");
    let read_result = subscriber.read();
    match read_result {
        Err(tungstenite::Error::Io(error))
            if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
        other => panic!("expected timeout without unrelated broadcast, got {other:?}"),
    }

    drop(subscriber);
    let stopped = stop_test_hook_bridge(&runtime);
    assert_eq!(stopped["running"], false);

    drop(runtime);
    fs::remove_dir_all(root).expect("cleanup channel filter root");
}

#[test]
fn daemon_hook_bridge_executes_mcp_backed_art_node() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let root = unique_temp_dir("mcp-art-node");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
    let fixture = current_test_binary_mcp_fixture_config();

    let saved_server = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request(
                "PUT",
                "/v1/mcp/servers/fixture",
                &[],
                Some(&fixture.to_string()),
            ),
        ),
        200,
    );
    assert_eq!(saved_server["server"]["id"], "fixture");

    let saved_tool = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request(
                "PUT",
                "/v1/tools/fixture-art",
                &[],
                Some(
                    r#"{
              "id": "fixture-art",
              "name": "Fixture Art",
              "description": "Execute fixture MCP through Hook bridge",
              "enabled": true,
              "execution": {
                "type": "mcp",
                "serverId": "fixture",
                "toolName": "echo"
              }
            }"#,
                ),
            ),
        ),
        200,
    );
    assert_eq!(saved_tool["tool"]["id"], "fixture-art");

    let started = start_test_hook_bridge(&runtime, r#"{"port":0}"#);
    let bridge_port = started["port"].as_u64().expect("bridge port") as u16;
    let mut socket = connect_hook_bridge_websocket(bridge_port);

    socket
        .send(tungstenite::Message::Text(formal_art_execute_request(
            "mcp-execute",
            "node-mcp",
            "fixture-art",
            None,
            json!({ "text": "execute art node runtime" }),
        )))
        .expect("send execute art node");
    let response = read_hook_terminal_response(&mut socket, "mcp-execute");

    assert_eq!(response["status"], "succeeded", "response={response}");
    assert_eq!(response["data"]["nodeId"], "node-mcp");
    assert!(
        response.to_string().contains("execute art node runtime"),
        "response={response}"
    );

    drop(socket);
    let stopped = stop_test_hook_bridge(&runtime);
    assert_eq!(stopped["running"], false);

    drop(runtime);
    fs::remove_dir_all(root).expect("cleanup mcp art node root");
}
