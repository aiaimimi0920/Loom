// Use a pinned TLS connection and a real signed worker which reports progress
// before blocking; cancellation after an already-completed command is not proof.
#[test]
fn otter_extension_disconnect_retires_a_running_signed_plugin_and_allows_retry() {
    use loom_capability_runtime::{CapabilityHostError, CapabilityInvocation};
    use tungstenite::Message;

    let fixture = Fixture::with_service();
    let call = |id: &str, hang| CapabilityInvocation {
        request_id: id.into(),
        command_id: "publisher.example/local-service.run".into(),
        input: json!({"hang": hang}),
        target: None,
        resource_refs: vec![],
        unit_attachments: vec![],
        staged_resources: vec![],
        user_gesture_token: None,
        timeout: None,
    };
    for close in ["tcp", "websocket", "bridge-stop"] {
        let runtime = Arc::clone(&fixture.runtime);
        let invocation = call("extension:disconnect", true);
        let (control_tx, control_rx) = mpsc::sync_channel(1);
        let stopped = Arc::new(AtomicBool::new(false));
        let bridge_stopped = Arc::clone(&stopped);
        let (mut client, bridge) = otter_liveness_socket(move |mut socket| {
            let mut deferred = None;
            let result = wait_otter_extension_command(
                &mut socket,
                &bridge_stopped,
                &mut deferred,
                |control| {
                    control_tx.send(Arc::clone(&control)).unwrap();
                    let result = runtime.invoke_controlled(invocation, control);
                    assert!(
                        matches!(result, Err(CapabilityHostError::Cancelled)),
                        "{result:?}"
                    );
                },
            );
            assert!(
                result.is_none(),
                "disconnected invocation returned a response"
            );
        });
        let control = control_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while control.progress().is_none() {
            assert!(Instant::now() < deadline, "fixture command never started");
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(control.progress().unwrap().payload.unwrap()["step"], 1);
        assert_eq!(fixture.runtime.process_ids().len(), 1);
        let disconnected_at = Instant::now();
        if close != "tcp" {
            if close == "websocket" {
                client.close(None).unwrap();
            } else {
                stopped.store(true, Ordering::SeqCst);
            }
            assert!(matches!(client.read().unwrap(), Message::Close(_)));
        }
        drop(client);
        bridge.join().unwrap();
        assert!(disconnected_at.elapsed() < Duration::from_secs(3));
        assert!(
            fixture.runtime.process_ids().is_empty(),
            "cancelled process was retained"
        );
        let retry = fixture
            .runtime
            .invoke(call("extension:disconnect", false))
            .unwrap();
        assert_eq!(retry.payload.unwrap()["output"]["calls"], 1);
    }
}

#[test]
fn otter_bridge_extension_disconnect_releases_real_upload_and_runtime() {
    use loom_capability_runtime::{CapabilityHostError, CapabilityInvocation};
    use tungstenite::Message;

    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let root = unique_temp_dir("hook-extension-disconnect");
    let daemon = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
    let runtime = &daemon.capability_runtime;
    install_local_service_fixture_scoped(&root, runtime, true);
    start_test_otter_bridge(&daemon, r#"{"port":0}"#);
    let mut socket =
        loom_local_channel::connect(&test_bridge_discovery(&root), Duration::from_secs(2)).unwrap();
    let exchange = |socket: &mut loom_local_channel::ClientSocket, method, params| {
        socket
            .send(Message::Text(
                json!({"method": method, "params": params}).to_string(),
            ))
            .unwrap();
        read_otter_bridge_json(socket)
    };
    let hook = exchange(
        &mut socket,
        loom_protocol::HOOK_METHOD_HANDSHAKE,
        json!({
            "protocolVersion": loom_protocol::HOOK_PROTOCOL_VERSION,
            "supportedProtocolVersions": [loom_protocol::HOOK_PROTOCOL_VERSION],
            "clientId": "cancel-regression", "clientVersion": "0.4.2", "platform": "windows",
            "transports": ["websocket"],
        }),
    );
    let extension = exchange(
        &mut socket,
        loom_protocol::EXTENSION_METHOD_HANDSHAKE,
        json!({
            "requestId": "handshake:cancel", "hookSessionId": hook["sessionId"],
            "protocol": loom_protocol::EXTENSION_PROTOCOL, "apiVersion": "1.0",
            "requiredFeatures": [
                loom_protocol::EXTENSION_FEATURE_SNAPSHOT, loom_protocol::EXTENSION_FEATURE_COMMANDS,
                loom_protocol::EXTENSION_FEATURE_RESOURCE_AUTHORIZATION
            ],
        }),
    );
    assert_eq!(extension["status"], "succeeded", "{extension}");
    let session = &extension["data"]["sessionId"];
    let generation = &extension["data"]["snapshot"]["generation"];
    let plugin = "publisher.example/local-service";
    let command = "publisher.example/local-service.run";
    let target = json!({"unitId":"unit-test","revision":1});
    let approval = exchange(
        &mut socket,
        "loom.extension.command.authorize",
        json!({
            "requestId": "authorize:cancel", "sessionId": session, "pluginId": plugin,
            "commandId": command, "snapshotGeneration": generation, "target": target,
        }),
    );
    assert_eq!(approval["status"], "succeeded", "{approval}");
    socket.send(Message::Text(json!({
        "method": loom_protocol::EXTENSION_METHOD_COMMAND_INVOKE,
        "params": {
            "sessionId": session, "authorizationId": approval["data"]["authorizationId"],
            "invocation": {
                "protocol": loom_protocol::EXTENSION_PROTOCOL, "apiVersion": "1.0",
                "requestId": "invoke:cancel", "pluginId": plugin, "commandId": command,
                "snapshotGeneration": generation, "target": target, "input": {"hang":true},
                "resourceRefs": []
            },
            "resourceUploads": [{"kind":"image","mime":"image/png","dataBase64":test_png_base64()}]
        }
    }).to_string())).unwrap();
    let leases = || {
        daemon
            .surface_resources
            .lock()
            .unwrap()
            .active_leases_for_test()
    };
    let busy = || {
        matches!(
            runtime.invoke(CapabilityInvocation {
                request_id: "independent:probe".into(),
                command_id: command.into(),
                input: json!({}),
                target: None,
                resource_refs: vec![],
                unit_attachments: vec![],
                staged_resources: vec![],
                user_gesture_token: None,
                timeout: Some(Duration::from_secs(1)),
            }),
            Err(CapabilityHostError::Busy)
        )
    };
    let start_deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if leases() > 0 && busy() {
            // Ping confirms the production connection keeps servicing frames while the upload is held.
            socket.send(Message::Ping(vec![3])).unwrap();
            assert_eq!(socket.read().unwrap(), Message::Pong(vec![3]));
            assert!(leases() > 0);
            break;
        }
        assert!(
            Instant::now() < start_deadline,
            "command did not stage its image"
        );
        thread::sleep(Duration::from_millis(5));
    }
    drop(socket);
    let release_deadline = Instant::now() + Duration::from_secs(3);
    while leases() != 0 || !runtime.process_ids().is_empty() {
        assert!(
            Instant::now() < release_deadline,
            "disconnect retained resources: leases={}, pids={:?}",
            leases(),
            runtime.process_ids()
        );
        thread::sleep(Duration::from_millis(5));
    }
    stop_test_otter_bridge(&daemon);
    drop(daemon);
    remove_test_dir(&root);
}
