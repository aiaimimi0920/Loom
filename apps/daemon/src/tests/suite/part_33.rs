// Capability extension bridge helpers keep lifecycle assertions out of the core API fixture test.
fn enable_api_fixture_through_extension_route(
    daemon: &DaemonRuntime,
    root: &Path,
    runtime: &SharedCapabilityRuntime,
    resources: &SharedCapabilityResourceBroker,
    digest: &str,
) -> (Receiver<String>, HookBridgeSubscriptionGuard) {
    let (events, subscription) = register_hook_bridge_subscription(
        &daemon.hook_bridge.lock().unwrap().broadcast_hub,
        vec![loom_protocol::EXTENSION_EVENT_SNAPSHOT_UPDATED.to_owned()],
    );
    let request = ParsedHttpRequest {
        method: "POST".to_owned(),
        path: "/v1/capability-plugins/publisher.example%2Fapi-fixture/enable".to_owned(),
        headers: Vec::new(),
        body: json!({ "digest": digest }).to_string(),
    };
    let (status, response) = route_capability_plugins(
        &request,
        &request.path,
        root,
        runtime,
        resources,
        &daemon.hook_bridge,
    )
    .expect("capability route")
    .expect("enable response");
    assert_eq!(status, 200, "{response}");
    assert_eq!(
        serde_json::from_str::<Value>(&response).unwrap()["plugin"]["status"],
        "active"
    );
    let event: Value = serde_json::from_str(
        &events
            .recv_timeout(Duration::from_secs(2))
            .expect("enabled extension snapshot event"),
    )
    .expect("enabled extension event JSON");
    assert_eq!(event["method"], loom_protocol::EXTENSION_EVENT_SNAPSHOT_UPDATED);
    assert_eq!(event["params"]["snapshot"]["generation"], 1);
    (events, subscription)
}

fn assert_api_fixture_extension_invocation(runtime: &SharedCapabilityRuntime) {
    let mut state = ExtensionConnectionState {
        hook_session_id: Some("hook:test-session".to_owned()),
        ..ExtensionConnectionState::default()
    };
    let snapshot_only_handshake = handle_extension_bridge_text(
        &json!({
            "method": loom_protocol::EXTENSION_METHOD_HANDSHAKE,
            "params": {
                "requestId": "extension-handshake",
                "hookSessionId": "hook:test-session",
                "protocol": loom_protocol::EXTENSION_PROTOCOL,
                "apiVersion": "1.0",
                "requiredFeatures": [loom_protocol::EXTENSION_FEATURE_SNAPSHOT],
                "optionalFeatures": [loom_protocol::EXTENSION_FEATURE_MENUS]
            }
        })
        .to_string(),
        &mut state,
        runtime,
    );
    assert!(snapshot_only_handshake.subscribe_to_snapshots);
    let snapshot_only_handshake: ExtensionBridgeResponse =
        serde_json::from_str(&snapshot_only_handshake.response).unwrap();
    let snapshot_only_session = snapshot_only_handshake.data["sessionId"].as_str().unwrap();
    let invocation = json!({
        "protocol": loom_protocol::EXTENSION_PROTOCOL,
        "apiVersion": "1.0",
        "requestId": "extension-command",
        "pluginId": "publisher.example/api-fixture",
        "commandId": "publisher.example/api-fixture.run",
        "snapshotGeneration": 1,
        "target": { "unitId": "unit-test", "revision": 1 },
        "input": { "text": "hello" },
        "resourceRefs": []
    });
    let rejected = handle_extension_bridge_text(
        &json!({
            "method": loom_protocol::EXTENSION_METHOD_COMMAND_INVOKE,
            "params": { "sessionId": snapshot_only_session, "invocation": invocation.clone() }
        })
        .to_string(),
        &mut state,
        runtime,
    );
    let rejected: ExtensionBridgeResponse = serde_json::from_str(&rejected.response).unwrap();
    assert_eq!(rejected.status, ExtensionBridgeStatus::Failed);
    assert_eq!(
        rejected.error.unwrap().code,
        "extension_feature_not_negotiated"
    );

    let handshake = handle_extension_bridge_text(
        &json!({
            "method": loom_protocol::EXTENSION_METHOD_HANDSHAKE,
            "params": {
                "requestId": "extension-handshake-with-command",
                "hookSessionId": "hook:test-session",
                "protocol": loom_protocol::EXTENSION_PROTOCOL,
                "apiVersion": "1.0",
                "requiredFeatures": [
                    loom_protocol::EXTENSION_FEATURE_SNAPSHOT,
                    loom_protocol::EXTENSION_FEATURE_COMMANDS
                ],
                "optionalFeatures": [loom_protocol::EXTENSION_FEATURE_MENUS]
            }
        })
        .to_string(),
        &mut state,
        runtime,
    );
    let handshake: ExtensionBridgeResponse = serde_json::from_str(&handshake.response).unwrap();
    assert_eq!(handshake.status, ExtensionBridgeStatus::Succeeded);
    let session = handshake.data["sessionId"].as_str().unwrap();
    let response = handle_extension_bridge_text(
        &json!({
            "method": loom_protocol::EXTENSION_METHOD_COMMAND_INVOKE,
            "params": {
                "sessionId": session,
                "invocation": invocation
            }
        })
        .to_string(),
        &mut state,
        runtime,
    );
    let response: ExtensionBridgeResponse = serde_json::from_str(&response.response).unwrap();
    let result: ExtensionResult = serde_json::from_value(response.data).unwrap();
    assert_eq!(result.status, ExtensionResultStatus::Succeeded);
    assert_eq!(result.output["ok"], true);
}

fn disable_api_fixture_through_extension_route(
    daemon: &DaemonRuntime,
    root: &Path,
    runtime: &SharedCapabilityRuntime,
    resources: &SharedCapabilityResourceBroker,
    events: &Receiver<String>,
) {
    let request = ParsedHttpRequest {
        method: "POST".to_owned(),
        path: "/v1/capability-plugins/publisher.example%2Fapi-fixture/disable".to_owned(),
        headers: Vec::new(),
        body: String::new(),
    };
    route_capability_plugins(
        &request,
        &request.path,
        root,
        runtime,
        resources,
        &daemon.hook_bridge,
    )
    .expect("capability route")
    .expect("disable response");
    let event: Value = serde_json::from_str(
        &events
            .recv_timeout(Duration::from_secs(2))
            .expect("disabled extension snapshot event"),
    )
    .expect("disabled extension event JSON");
    assert!(event["params"]["snapshot"]["plugins"]
        .as_array()
        .unwrap()
        .is_empty());
}
