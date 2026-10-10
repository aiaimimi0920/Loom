// Exercise daemon liveness helpers over the actual pinned TLS transport.
fn hook_liveness_socket(
    serve: impl FnOnce(loom_local_channel::ServerSocket) + Send + 'static,
) -> (loom_local_channel::ClientSocket, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let identity =
        loom_local_channel::ServerIdentity::generate(listener.local_addr().unwrap().port())
            .unwrap();
    let discovery = identity.discovery().clone();
    let worker = thread::spawn(move || {
        let (tcp, _) = listener.accept().unwrap();
        serve(identity.accept(tcp).unwrap());
    });
    let socket = loom_local_channel::connect(&discovery, Duration::from_secs(2)).unwrap();
    (socket, worker)
}

fn hook_liveness_receive(socket: &mut loom_local_channel::ServerSocket) -> tungstenite::Message {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match read_hook_bridge_message(socket) {
            Ok(message) => return message,
            Err(error) if hook_bridge_read_timed_out(&error) && Instant::now() < deadline => {}
            Err(error) => panic!("expected message before test deadline: {error}"),
        }
    }
}

#[test]
fn hook_bridge_liveness_pings_and_expires_without_inbound_messages() {
    let (mut client, worker) = hook_liveness_socket(|mut socket| {
        let start = Instant::now();
        let mut liveness = HookBridgeLiveness::new(start, HOOK_BRIDGE_IDLE_TIMEOUT);
        assert!(liveness.maintain(&mut socket, start + HOOK_BRIDGE_PING_INTERVAL));
        assert!(!liveness.maintain(&mut socket, start + HOOK_BRIDGE_IDLE_TIMEOUT));
    });
    assert!(matches!(
        client.read().unwrap(),
        tungstenite::Message::Ping(_)
    ));
    worker.join().unwrap();
}

#[test]
fn hook_bridge_liveness_completed_work_renews_idle_budget() {
    let (mut client, worker) = hook_liveness_socket(|mut socket| {
        let start = Instant::now();
        let mut liveness = HookBridgeLiveness::new(start, HOOK_BRIDGE_IDLE_TIMEOUT);
        // A valid synchronous request may take longer than the idle window.
        let completed = start + HOOK_BRIDGE_IDLE_TIMEOUT * 2;
        liveness.received(completed);
        assert!(liveness.maintain(&mut socket, completed));
        assert!(!liveness.maintain(&mut socket, completed + HOOK_BRIDGE_IDLE_TIMEOUT));
    });
    assert!(matches!(
        client.read().unwrap(),
        tungstenite::Message::Ping(_)
    ));
    worker.join().unwrap();
}

#[test]
fn hook_bridge_liveness_read_deadline_yields_under_fragment_trickle() {
    use tungstenite::protocol::frame::{
        coding::{Data, OpCode},
        Frame,
    };
    let (ready_tx, ready_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let (mut client, worker) = hook_liveness_socket(move |mut socket| {
        ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let started = Instant::now();
        let error = read_hook_bridge_message(&mut socket).unwrap_err();
        assert!(hook_bridge_read_timed_out(&error));
        assert!(started.elapsed() < Duration::from_secs(1));
        // An expired read slice must not poison the next outbound operation.
        send_hook_bridge_message(&mut socket, tungstenite::Message::Ping(vec![7])).unwrap();
        done_tx.send(()).unwrap();
    });
    client
        .send(tungstenite::Message::Frame(Frame::message(
            vec![b'x'],
            OpCode::Data(Data::Text),
            false,
        )))
        .unwrap();
    ready_tx.send(()).unwrap();
    let limit = Instant::now() + Duration::from_secs(2);
    while done_rx.try_recv().is_err() && Instant::now() < limit {
        if client
            .send(tungstenite::Message::Frame(Frame::message(
                vec![b'x'],
                OpCode::Data(Data::Continue),
                false,
            )))
            .is_err()
        {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    worker.join().unwrap();
}

#[test]
fn hook_bridge_liveness_response_renews_expired_io_budget() {
    let (mut client, worker) = hook_liveness_socket(|mut socket| {
        socket.get_mut().sock.set_operation_deadline(Instant::now());
        send_hook_bridge_message(&mut socket, tungstenite::Message::Text("response".into()))
            .unwrap();
    });
    assert_eq!(client.read().unwrap().into_text().unwrap(), "response");
    worker.join().unwrap();
}

#[test]
fn hook_bridge_liveness_close_reply_renews_expired_read_budget() {
    // Negative control: the old path can report ConnectionClosed even when the
    // TLS close reply could not be flushed within the expired read deadline.
    let (mut client, worker) = hook_liveness_socket(|mut socket| {
        let tungstenite::Message::Close(close) = hook_liveness_receive(&mut socket) else {
            panic!("expected peer close");
        };
        socket.get_mut().sock.set_operation_deadline(Instant::now());
        let _ = socket.close(close);
    });
    client.close(None).unwrap();
    assert!(
        client.read().is_err(),
        "negative control unexpectedly delivered Close"
    );
    worker.join().unwrap();

    let (mut client, worker) = hook_liveness_socket(|mut socket| {
        let message = hook_liveness_receive(&mut socket);
        let tungstenite::Message::Close(close) = message else {
            panic!("expected peer close");
        };
        socket.get_mut().sock.set_operation_deadline(Instant::now());
        assert!(matches!(
            close_hook_bridge_socket(&mut socket, close),
            Ok(()) | Err(tungstenite::Error::ConnectionClosed)
        ));
    });
    client.close(None).unwrap();
    assert!(matches!(
        client.read().unwrap(),
        tungstenite::Message::Close(_)
    ));
    worker.join().unwrap();
}

#[test]
fn hook_bridge_liveness_connection_capacity_rejects_and_reaps_without_new_accepts() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let root = unique_temp_dir("hook-bridge-native-capacity");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
    start_test_hook_bridge(&runtime, r#"{"port":0}"#);
    let discovery = test_bridge_discovery(&root);
    let mut clients = Vec::new();
    for _ in 0..32 {
        clients.push(loom_local_channel::connect(&discovery, Duration::from_secs(2)).unwrap());
    }
    let connections = runtime.hook_bridge.lock().unwrap().connections.clone();
    let deadline = Instant::now() + Duration::from_secs(2);
    while !connections.at_capacity() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(connections.at_capacity());
    assert!(loom_local_channel::connect(&discovery, Duration::from_secs(1)).is_err());
    drop(clients);
    let deadline = Instant::now() + Duration::from_secs(2);
    while !connections.workers.lock().unwrap().is_empty() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    let reaped = connections.workers.lock().unwrap().is_empty();
    let replacement = loom_local_channel::connect(&discovery, Duration::from_secs(2));
    stop_test_hook_bridge(&runtime);
    drop(runtime);
    fs::remove_dir_all(root).unwrap();
    assert!(reaped, "idle accept loop retained finished worker handles");
    assert!(replacement.is_ok(), "released capacity was not reusable");
}

#[test]
fn hook_bridge_liveness_daemon_releases_silent_authenticated_peer() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let root = unique_temp_dir("hook-bridge-native-idle");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
    let idle_timeout = Duration::from_millis(300);
    runtime.hook_bridge.lock().unwrap().connections.idle_timeout = idle_timeout;
    start_test_hook_bridge(&runtime, r#"{"port":0}"#);
    let started = Instant::now();
    // Do not read: tungstenite would automatically reply to received Ping frames.
    let _client =
        loom_local_channel::connect(&test_bridge_discovery(&root), Duration::from_secs(2)).unwrap();
    let connections = runtime.hook_bridge.lock().unwrap().connections.clone();
    let deadline = Instant::now() + Duration::from_secs(2);
    while connections.workers.lock().unwrap().is_empty() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(!connections.workers.lock().unwrap().is_empty());
    let deadline = started + idle_timeout + Duration::from_secs(3);
    while !connections.workers.lock().unwrap().is_empty() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(25));
    }
    let released = connections.workers.lock().unwrap().is_empty();
    let elapsed = started.elapsed();
    stop_test_hook_bridge(&runtime);
    drop(runtime);
    fs::remove_dir_all(root).unwrap();
    assert!(
        released,
        "silent authenticated peer retained a worker beyond its idle deadline"
    );
    assert!(elapsed >= idle_timeout - Duration::from_millis(50));
}
