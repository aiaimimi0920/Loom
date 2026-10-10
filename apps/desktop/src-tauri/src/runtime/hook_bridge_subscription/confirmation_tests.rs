use super::*;

fn handshake_only(socket: &mut ServerSocket) {
    assert_eq!(receive(socket)["method"], "loom.hook.handshake");
    send(
        socket,
        json!({"protocolVersion":"loom.hook.v1", "sessionId":"test", "transport":"websocket"}),
    );
    assert_eq!(receive(socket)["method"], "loom.hook.subscribe");
}

#[test]
fn stop_interrupts_application_handshake_and_subscribe_waits() {
    for subscribe in [false, true] {
        let fixture = Fixture::new();
        let mut subscription = Subscription::start(fixture.manifest.clone()).unwrap();
        let mut socket = fixture.accept();
        if subscribe {
            handshake_only(&mut socket);
        } else {
            assert_eq!(receive(&mut socket)["method"], "loom.hook.handshake");
        }
        let start = Instant::now();
        subscription.stop();
        assert!(start.elapsed() < Duration::from_secs(2));
        assert!(!subscription.shared.counters.lock().unwrap().connected);
    }
}

#[test]
fn failed_or_inexact_subscription_ack_never_marks_connected() {
    for (status, events) in [
        (
            "failed",
            json!([
                "loom.hook.workflow.updated",
                "loom.hook.capabilities.updated"
            ]),
        ),
        ("succeeded", json!(["loom.hook.workflow.updated"])),
        (
            "succeeded",
            json!([
                "loom.hook.workflow.updated",
                "loom.hook.capabilities.updated",
                "extra"
            ]),
        ),
    ] {
        let fixture = Fixture::new();
        let shared = Arc::new(Shared::default());
        let task = shared.clone();
        let path = fixture.manifest.clone();
        let worker = std::thread::spawn(move || session::run(&path, &task));
        let mut socket = fixture.accept();
        handshake_only(&mut socket);
        send(
            &mut socket,
            json!({"protocolVersion":"loom.hook.v1", "requestId":"desktop-subscribe",
            "status":status, "data":{"events":events}}),
        );
        assert!(worker.join().unwrap().is_err());
        let state = shared.counters.lock().unwrap();
        assert!(!state.connected);
        assert_eq!(state.epoch, 0);
        drop(state);
        shared.disconnect();
    }
}

#[test]
fn subscribe_events_before_ack_do_not_extend_absolute_confirmation_deadline() {
    let fixture = Fixture::new();
    let shared = Arc::new(Shared::default());
    let task = shared.clone();
    let path = fixture.manifest.clone();
    let worker = std::thread::spawn(move || session::run(&path, &task));
    let mut socket = fixture.accept();
    handshake_only(&mut socket);
    let start = Instant::now();
    while !worker.is_finished() && start.elapsed() < Duration::from_secs(5) {
        let _ = socket.send(Message::Text(
            json!({"method":"loom.hook.capabilities.updated"}).to_string(),
        ));
        std::thread::sleep(Duration::from_millis(40));
    }
    assert!(worker.join().unwrap().is_err());
    assert!(start.elapsed() < Duration::from_secs(5));
    let state = shared.counters.lock().unwrap();
    assert!(!state.connected);
    assert!(state.capabilities > 1);
    drop(state);
    shared.disconnect();
}

#[test]
fn stop_during_tls_negotiation_is_bounded_by_transport_deadline() {
    let fixture = Fixture::new();
    let mut subscription = Subscription::start(fixture.manifest.clone()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let _tcp = loop {
        if let Ok((tcp, _)) = fixture.listener.accept() {
            break tcp;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    };
    let start = Instant::now();
    subscription.stop();
    assert!(start.elapsed() < Duration::from_secs(5));
    assert!(!subscription.shared.counters.lock().unwrap().connected);
}
