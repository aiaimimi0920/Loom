use super::*;
use loom_local_channel::{ServerIdentity, ServerSocket};
use serde_json::{json, Value};
use std::net::TcpListener;
use std::sync::atomic::AtomicUsize;
use std::time::Instant;
use tungstenite::Message;

static NEXT_FILE: AtomicUsize = AtomicUsize::new(0);

#[path = "confirmation_tests.rs"]
mod confirmations;

struct Fixture {
    manifest: PathBuf,
    listener: TcpListener,
    identity: ServerIdentity,
}

impl Fixture {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let identity = ServerIdentity::generate(listener.local_addr().unwrap().port()).unwrap();
        let manifest = std::env::temp_dir().join(format!(
            "loom-desktop-bridge-{}-{}.json",
            std::process::id(),
            NEXT_FILE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(
            &manifest,
            serde_json::to_vec(&json!({ "hookBridge": identity.discovery() })).unwrap(),
        )
        .unwrap();
        Self {
            manifest,
            listener,
            identity,
        }
    }

    fn accept(&self) -> ServerSocket {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match self.listener.accept() {
                Ok((tcp, _)) => {
                    tcp.set_nonblocking(false).unwrap();
                    let mut socket = self.identity.accept(tcp).unwrap();
                    socket
                        .get_mut()
                        .sock
                        .finish_handshake(Duration::from_secs(2))
                        .unwrap();
                    return socket;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        Instant::now() < deadline,
                        "native subscription did not connect"
                    );
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("accept failed: {error}"),
            }
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.manifest);
    }
}

fn receive(socket: &mut ServerSocket) -> Value {
    serde_json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap()
}

fn send(socket: &mut ServerSocket, value: Value) {
    socket.send(Message::Text(value.to_string())).unwrap();
}

fn confirm(socket: &mut ServerSocket) {
    let handshake = receive(socket);
    assert_eq!(handshake["method"], "loom.hook.handshake");
    assert_eq!(handshake["params"]["transports"], json!(["websocket"]));
    send(
        socket,
        json!({"protocolVersion":"loom.hook.v1", "sessionId":"test-session", "transport":"websocket"}),
    );
    let subscribe = receive(socket);
    assert_eq!(subscribe["method"], "loom.hook.subscribe");
    assert_eq!(
        subscribe["params"]["events"],
        json!([
            "loom.hook.workflow.updated",
            "loom.hook.capabilities.updated"
        ])
    );
    send(
        socket,
        json!({"protocolVersion":"loom.hook.v1", "requestId":"desktop-subscribe", "status":"succeeded",
        "data":{"events":subscribe["params"]["events"]}}),
    );
}

fn wait_until(predicate: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !predicate() {
        assert!(Instant::now() < deadline, "subscription state deadline");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn state_contains_only_counters_and_main_window_is_required() {
    assert!(require_main("main").is_ok());
    for label in ["", "settings", "external", "Main"] {
        assert!(require_main(label).is_err());
    }
    let value = serde_json::to_value(Counters::default().snapshot()).unwrap();
    assert_eq!(
        value,
        json!({"connected":false,"epoch":"0","workflowRevision":"0","capabilitiesRevision":"0"})
    );
}

#[test]
fn only_fixed_events_and_live_workflow_increment_counters() {
    let shared = Shared::default();
    for workflow in [json!("other"), json!(null), json!(42)] {
        session::apply_event(
            &shared,
            &json!({"method":"loom.hook.workflow.updated", "params":{"workflowId":workflow}}),
        );
    }
    session::apply_event(&shared, &json!({"method":"loom.extension.command.invoke"}));
    assert_eq!(shared.counters.lock().unwrap().workflow, 0);
    session::apply_event(&shared, &json!({"method":"loom.hook.workflow.updated"}));
    session::apply_event(
        &shared,
        &json!({"method":"loom.hook.workflow.updated", "params":{"workflowId":"hook-live"}}),
    );
    session::apply_event(&shared, &json!({"method":"loom.hook.capabilities.updated"}));
    let state = shared.counters.lock().unwrap();
    assert_eq!((state.workflow, state.capabilities), (2, 1));
}

#[test]
fn native_tls_fixed_subscription_updates_and_stop_joins_idle_socket() {
    let fixture = Fixture::new();
    let mut subscription = Subscription::start(fixture.manifest.clone()).unwrap();
    let mut socket = fixture.accept();
    confirm(&mut socket);
    send(
        &mut socket,
        json!({"method":"loom.hook.workflow.updated", "params":{"workflowId":"hook-live"}}),
    );
    send(
        &mut socket,
        json!({"method":"loom.hook.capabilities.updated"}),
    );
    wait_until(|| subscription.shared.counters.lock().unwrap().capabilities == 1);
    {
        let state = subscription.shared.counters.lock().unwrap();
        assert!(state.connected);
        assert_eq!((state.epoch, state.workflow), (1, 1));
    }
    // Idle read timeouts keep this connection rather than repeatedly reconnecting.
    std::thread::sleep(Duration::from_millis(600));
    assert_eq!(subscription.shared.counters.lock().unwrap().epoch, 1);
    socket.send(Message::Ping(vec![1, 2, 3])).unwrap();
    assert_eq!(socket.read().unwrap(), Message::Pong(vec![1, 2, 3]));
    let start = Instant::now();
    subscription.stop();
    assert!(start.elapsed() < Duration::from_secs(2));
    assert!(!subscription.shared.counters.lock().unwrap().connected);
    assert!(subscription.worker.is_none());
}

#[test]
fn failed_handshake_never_subscribes_or_marks_connected() {
    let fixture = Fixture::new();
    let shared = Arc::new(Shared::default());
    let task = shared.clone();
    let path = fixture.manifest.clone();
    let worker = std::thread::spawn(move || session::run(&path, &task));
    let mut socket = fixture.accept();
    assert_eq!(receive(&mut socket)["method"], "loom.hook.handshake");
    send(
        &mut socket,
        json!({"protocolVersion":"legacy", "sessionId":"bad", "transport":"websocket"}),
    );
    assert!(worker.join().unwrap().is_err());
    assert!(!shared.counters.lock().unwrap().connected);
    shared.disconnect();
    assert!(socket.read().is_err());
}

#[test]
fn missing_manifest_worker_stops_during_retry_and_drop_joins() {
    let fixture = Fixture::new();
    let missing = fixture.manifest.with_extension("absent");
    let mut subscription = Subscription::start(missing).unwrap();
    let start = Instant::now();
    subscription.stop();
    subscription.stop();
    assert!(start.elapsed() < Duration::from_secs(1));
}

#[test]
fn reconnect_reads_rotated_manifest_and_advances_epoch() {
    let fixture = Fixture::new();
    let mut subscription = Subscription::start(fixture.manifest.clone()).unwrap();
    let mut first = fixture.accept();
    confirm(&mut first);
    wait_until(|| subscription.shared.counters.lock().unwrap().epoch == 1);
    let replacement = Fixture::new();
    std::fs::write(
        &fixture.manifest,
        std::fs::read(&replacement.manifest).unwrap(),
    )
    .unwrap();
    drop(first);
    let mut second = replacement.accept();
    confirm(&mut second);
    wait_until(|| subscription.shared.counters.lock().unwrap().epoch == 2);
    subscription.stop();
}
