// Capability extension bridge helpers keep lifecycle assertions out of the core API fixture test.
#[test]
fn extension_bridge_preserves_unsuccessful_runtime_status() {
    use loom_protocol::{CapabilityProtocolError, CapabilityRuntimeStatus};
    for status in [CapabilityRuntimeStatus::Failed, CapabilityRuntimeStatus::Cancelled,
        CapabilityRuntimeStatus::Accepted, CapabilityRuntimeStatus::Progress] {
        let output = loom_capability_runtime::CapabilityInvocationOutput {
            plugin_id: "publisher.example/api-fixture".to_owned(),
            package_digest: "fixture".to_owned(), status,
            payload: Some(json!({ "output": { "untrusted": true }, "effects": ["untrusted"] })),
            error: Some(CapabilityProtocolError { code: CapabilityErrorCode::Busy,
                message: "private-runtime-detail".to_owned(), retryable: false }),
        };
        let response = extension_runtime_status_failure("failed-command", &output).unwrap();
        assert!(!response.response.contains("private-runtime-detail"));
        let envelope: ExtensionBridgeResponse = serde_json::from_str(&response.response).unwrap();
        let result: ExtensionResult = serde_json::from_value(envelope.data).unwrap();
        assert_eq!(result.request_id, "failed-command");
        assert_eq!(result.status, ExtensionResultStatus::Failed);
        assert!(result.output.is_null());
        assert!(result.effects.is_empty());
        assert_eq!(result.error.unwrap().code, if status == CapabilityRuntimeStatus::Cancelled {
            CapabilityErrorCode::Cancelled
        } else { CapabilityErrorCode::Busy });
    }
    let mut output = loom_capability_runtime::CapabilityInvocationOutput {
        plugin_id: "publisher.example/api-fixture".to_owned(), package_digest: "fixture".to_owned(),
        status: CapabilityRuntimeStatus::Succeeded, payload: None, error: None,
    };
    assert!(extension_runtime_status_failure("success", &output).is_none());
    output.status = CapabilityRuntimeStatus::Failed;
    let response = extension_runtime_status_failure("missing-error", &output).unwrap();
    let envelope: ExtensionBridgeResponse = serde_json::from_str(&response.response).unwrap();
    let result: ExtensionResult = serde_json::from_value(envelope.data).unwrap();
    assert_eq!(result.error.unwrap().code, CapabilityErrorCode::RuntimeFault);
}

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

fn assert_api_fixture_extension_invocation(
    runtime: &SharedCapabilityRuntime,
    resources: &SharedCapabilityResourceBroker,
    surface_resources: &SharedSurfaceResourceStore,
) {
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
                "optionalFeatures": [
                    loom_protocol::EXTENSION_FEATURE_MENUS,
                    loom_protocol::EXTENSION_FEATURE_NOTICES
                ]
            }
        })
        .to_string(),
        &mut state,
        runtime,
        resources,
        surface_resources,
    );
    assert!(snapshot_only_handshake.subscribe_to_snapshots);
    let snapshot_only_handshake: ExtensionBridgeResponse =
        serde_json::from_str(&snapshot_only_handshake.response).unwrap();
    let snapshot_only_session = snapshot_only_handshake.data["sessionId"].as_str().unwrap();
    let lease = surface_resources
        .lock()
        .unwrap()
        .register(
            SurfaceResourceKind::File,
            "application/octet-stream",
            b"extension bridge resource",
            None,
            None,
            None,
        )
        .expect("register extension bridge resource");
    let digest = lease
        .resource
        .resource_id
        .strip_prefix("sha256:")
        .unwrap()
        .to_owned();
    let resource_ref = ExtensionResourceRef {
        resource_id: lease.resource.resource_id,
        kind: loom_protocol::ExtensionResourceKind::File,
        digest,
        byte_length: lease.resource.size,
        lease_id: lease.lease_id,
    };
    let invocation = json!({
        "protocol": loom_protocol::EXTENSION_PROTOCOL,
        "apiVersion": "1.0",
        "requestId": "extension-command",
        "pluginId": "publisher.example/api-fixture",
        "commandId": "publisher.example/api-fixture.run",
        "snapshotGeneration": 1,
        "target": { "unitId": "unit-test", "revision": 1 },
        "input": { "text": "hello" },
        "resourceRefs": [resource_ref]
    });
    let rejected = handle_extension_bridge_text(
        &json!({
            "method": loom_protocol::EXTENSION_METHOD_COMMAND_INVOKE,
            "params": { "sessionId": snapshot_only_session, "invocation": invocation.clone() }
        })
        .to_string(),
        &mut state,
        runtime,
        resources,
        surface_resources,
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
                "optionalFeatures": [
                    loom_protocol::EXTENSION_FEATURE_MENUS,
                    loom_protocol::EXTENSION_FEATURE_NOTICES
                ]
            }
        })
        .to_string(),
        &mut state,
        runtime,
        resources,
        surface_resources,
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
        resources,
        surface_resources,
    );
    let response: ExtensionBridgeResponse = serde_json::from_str(&response.response).unwrap();
    let result: ExtensionResult = serde_json::from_value(response.data).unwrap();
    assert_eq!(result.status, ExtensionResultStatus::Succeeded);
    assert_eq!(result.output["ok"], true);
    assert!(result.effects.iter().any(|effect| {
        effect.effect_type == loom_protocol::ExtensionEffectType::AttachmentUpsert
    }));
    let attachment = result
        .effects
        .iter()
        .find(|effect| effect.effect_type == loom_protocol::ExtensionEffectType::AttachmentUpsert)
        .expect("attachment effect");
    assert_eq!(attachment.payload["resourceRefs"].as_array().unwrap().len(), 1);
    assert!(result.effects.iter().any(|effect| {
        effect.effect_type == loom_protocol::ExtensionEffectType::NoticeShow
    }));
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
