// Real daemon lifecycle, private discovery and bounded broadcast delivery.

#[test]
fn stopping_an_inactive_bridge_preserves_unrelated_art_request() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let root = unique_temp_dir("inactive-bridge-art-isolation");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
    assert!(runtime.hook_bridge.lock().unwrap().worker.is_none());
    let other_images = Arc::new(Mutex::new(SharedImageStore::new()));
    let request = hook_art_request("request:other-runtime", "node:other-runtime", 1);
    let cancellation = match reserve_hook_art_request(&request, &other_images) {
        HookArtReservation::Execute(token) => token,
        _ => panic!("reserve unrelated Art request"),
    };
    stop_test_hook_bridge(&runtime);
    let was_cancelled = cancellation.load(Ordering::Acquire);
    clear_hook_canvas_runtime_state(Some(&other_images));
    drop(runtime);
    fs::remove_dir_all(root).unwrap();
    assert!(!was_cancelled, "an inactive bridge cancelled another runtime's Art request");
}

#[test]
fn broadcast_overload_evicts_slow_subscriber_without_blocking_healthy_one() {
    let hub = HookBridgeBroadcastHub::new();
    let (slow, _slow_guard) =
        register_hook_bridge_subscription(&hub, vec!["loom.hook.workflow.updated".to_owned()]);
    let (fast, _fast_guard) =
        register_hook_bridge_subscription(&hub, vec!["loom.hook.workflow.updated".to_owned()]);
    let event = hook_protocol_event_json(
        "loom.hook.workflow.updated",
        &json!({"workflowId":"hook-live"}),
    );
    for _ in 0..128 {
        assert_eq!(
            broadcast_hook_bridge_messages_with_count(&hub, &[event.clone()]),
            2
        );
        assert_eq!(fast.try_recv().unwrap(), event);
    }
    assert_eq!(
        broadcast_hook_bridge_messages_with_count(&hub, &[event.clone()]),
        1
    );
    assert_eq!(hub.subscriber_count(), 1);
    assert!(matches!(
        slow.try_recv(),
        Err(mpsc::TryRecvError::Disconnected)
    ));
    assert_eq!(fast.try_recv().unwrap(), event);
}

#[test]
fn broadcast_history_is_byte_bounded_and_eviction_requires_recovery() {
    let hub = HookBridgeBroadcastHub::new();
    let message = "x".repeat(hook_broadcast_queue::MAX_BYTES / 2);
    for _ in 0..3 {
        hub.record(std::slice::from_ref(&message));
    }
    let history = hub.history.0.lock().unwrap();
    assert_eq!(history.len(), 2);
    assert!(
        history
            .iter()
            .map(|entry| entry.message.len())
            .sum::<usize>()
            <= hook_broadcast_queue::MAX_BYTES
    );
    drop(history);
    let (_, reset, _) = hub.wait_after(1, Duration::ZERO);
    assert!(reset);
}

#[test]
fn skipped_oversized_broadcast_advances_recovery_cursor_without_retaining_payload() {
    let hub = HookBridgeBroadcastHub::new();
    hub.record(&["x".repeat(hook_broadcast_queue::MAX_BYTES + 1)]);
    let (next, reset, entries) = hub.wait_after(1, Duration::ZERO);
    assert_eq!(next, 2);
    assert!(reset);
    assert!(entries.is_empty());
    assert!(hub.history.0.lock().unwrap().is_empty());
    let (_, reset, entries) = hub.wait_after(next, Duration::ZERO);
    assert!(!reset);
    assert!(entries.is_empty());
}
fn test_bridge_discovery(root: &Path) -> loom_local_channel::BridgeDiscovery {
    let manifest: Value =
        serde_json::from_slice(&fs::read(root.join("capabilities/loom.json")).unwrap()).unwrap();
    serde_json::from_value(manifest["hookBridge"].clone()).unwrap()
}

#[test]
fn local_bridge_rotates_identity_revokes_discovery_and_excludes_status_secrets() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let root = unique_temp_dir("local-bridge-rotation");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
    let original: Value =
        serde_json::from_slice(&fs::read(root.join("capabilities/loom.json")).unwrap()).unwrap();
    let started = start_test_hook_bridge(&runtime, r#"{"port":0}"#);
    let first = test_bridge_discovery(&root);
    assert!(!started.to_string().contains(&first.auth_token));
    let mut socket = loom_local_channel::connect(&first, Duration::from_secs(2)).unwrap();
    socket
        .send(tungstenite::Message::Text(
            json!({"method": "loom.hook.subscribe", "params": {"channels": []}}).to_string(),
        ))
        .unwrap();
    let _ = socket.read().unwrap();
    stop_test_hook_bridge(&runtime);
    let stopped: Value =
        serde_json::from_slice(&fs::read(root.join("capabilities/loom.json")).unwrap()).unwrap();
    assert!(stopped["hookBridge"].is_null());
    assert_eq!(stopped["transport"], original["transport"]);
    assert_eq!(stopped["startedAt"], original["startedAt"]);
    assert!(loom_local_channel::connect(&first, Duration::from_secs(1)).is_err());
    start_test_hook_bridge(&runtime, &json!({"port":started["port"]}).to_string());
    let second = test_bridge_discovery(&root);
    assert_ne!(first.instance_id, second.instance_id);
    assert_ne!(first.auth_token, second.auth_token);
    assert_ne!(first.certificate_sha256, second.certificate_sha256);
    assert!(loom_local_channel::connect(&first, Duration::from_secs(1)).is_err());
    assert!(loom_local_channel::connect(&second, Duration::from_secs(1)).is_ok());
    stop_test_hook_bridge(&runtime);
    drop(runtime);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn local_bridge_manifest_failure_never_leaves_a_started_listener() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let root = unique_temp_dir("local-bridge-publication-failure");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
    let blocked = root.join("directory-not-manifest");
    fs::create_dir(&blocked).unwrap();
    runtime
        .hook_bridge
        .lock()
        .unwrap()
        .discovery
        .as_mut()
        .unwrap()
        .path = blocked;
    let result = start_hook_bridge(
        r#"{"port":0}"#,
        &runtime.hook_bridge,
        &runtime.capability_runtime,
        &runtime.capability_resources,
        &runtime.surface_resources,
        &runtime.mcp_servers,
        &runtime.tool_registry,
        &runtime.workflow_store,
        &runtime.settings,
        &runtime.shared_images,
        &runtime.framework_registry,
        &runtime.control_plane_root,
        &runtime.run_store,
        &runtime.surface_instances,
        &runtime.surface_actions,
    );
    assert!(result.is_err());
    let state = runtime.hook_bridge.lock().unwrap();
    assert!(state.worker.is_none());
    assert!(state.port.is_none());
    assert_eq!(state.connected_clients.load(Ordering::SeqCst), 0);
    drop(state);
    drop(runtime);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn local_bridge_plaintext_connection_never_counts_as_authenticated() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let root = unique_temp_dir("local-bridge-plaintext");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
    let status = start_test_hook_bridge(&runtime, r#"{"port":0}"#);
    let port = status["port"].as_u64().unwrap() as u16;
    let tcp = TcpStream::connect(("127.0.0.1", port)).unwrap();
    tcp.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    tcp.set_write_timeout(Some(Duration::from_secs(2))).unwrap();
    assert!(tungstenite::client(format!("ws://127.0.0.1:{port}/"), tcp).is_err());
    let status = hook_bridge_status_value(&runtime);
    assert_eq!(status["connectedClients"], 0);
    assert_eq!(status["subscribedClients"], 0);
    stop_test_hook_bridge(&runtime);
    drop(runtime);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn local_bridge_daemon_shutdown_revokes_identity_and_joins_connections() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let root = unique_temp_dir("local-bridge-daemon-shutdown");
    let mut config = DaemonConfig::localhost(0).with_control_plane_root(&root);
    config.manifest_dir = Some(root.join("capabilities"));
    let daemon = LoomDaemon::bind(config).unwrap();
    let runtime = Arc::clone(&daemon.runtime);
    start_test_hook_bridge(&runtime, r#"{"port":0}"#);
    let discovery = test_bridge_discovery(&root);
    let _socket = loom_local_channel::connect(&discovery, Duration::from_secs(2)).unwrap();
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let worker = thread::spawn(move || daemon.serve_until(shutdown_rx));
    shutdown_tx.send(()).unwrap();
    worker.join().unwrap().unwrap();
    let stopped: Value =
        serde_json::from_slice(&fs::read(root.join("capabilities/loom.json")).unwrap()).unwrap();
    let revoked = stopped["hookBridge"].is_null();
    let reconnect = loom_local_channel::connect(&discovery, Duration::from_secs(1));
    // Keep failure cleanup deterministic even when testing an unfixed daemon.
    stop_test_hook_bridge(&runtime);
    assert!(revoked, "daemon shutdown retained bridge discovery");
    assert!(reconnect.is_err(), "daemon shutdown retained bridge listener");
    drop(runtime);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn local_bridge_stop_interrupts_authenticated_fragment_trickle() {
    use tungstenite::protocol::frame::{coding::{Data, OpCode}, Frame};
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let root = unique_temp_dir("local-bridge-stop-trickle");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
    start_test_hook_bridge(&runtime, r#"{"port":0}"#);
    let mut socket = loom_local_channel::connect(&test_bridge_discovery(&root), Duration::from_secs(2)).unwrap();
    let (ready_tx, ready_rx) = mpsc::channel();
    let sender = thread::spawn(move || {
        for index in 0..100 {
            let opcode = if index == 0 { Data::Text } else { Data::Continue };
            if socket.send(tungstenite::Message::Frame(Frame::message(vec![b'x'], OpCode::Data(opcode), false))).is_err() {
                break;
            }
            if index == 0 { let _ = ready_tx.send(()); }
            thread::sleep(Duration::from_millis(20));
        }
    });
    ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    thread::sleep(Duration::from_millis(50));
    let started = Instant::now();
    stop_test_hook_bridge(&runtime);
    let elapsed = started.elapsed();
    sender.join().unwrap();
    drop(runtime);
    fs::remove_dir_all(root).unwrap();
    assert!(elapsed < Duration::from_secs(1), "stop waited for peer fragments: {elapsed:?}");
}

#[test]
fn local_bridge_stop_publication_failure_still_closes_listener() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let root = unique_temp_dir("local-bridge-stop-publication-failure");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
    start_test_hook_bridge(&runtime, r#"{"port":0}"#);
    let discovery = test_bridge_discovery(&root);
    let blocked = root.join("directory-not-manifest");
    fs::create_dir(&blocked).unwrap();
    runtime.hook_bridge.lock().unwrap().discovery.as_mut().unwrap().path = blocked;
    assert!(stop_hook_bridge(&runtime.hook_bridge, &runtime.shared_images).is_err());
    assert!(runtime.hook_bridge.lock().unwrap().worker.is_none());
    assert!(runtime.hook_bridge.lock().unwrap().port.is_none());
    assert!(loom_local_channel::connect(&discovery, Duration::from_secs(1)).is_err());
    drop(runtime);
    fs::remove_dir_all(root).unwrap();
}
