use super::*;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    thread,
    time::{Duration, Instant},
};

fn fixture() -> (TcpListener, ServerIdentity) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let identity = ServerIdentity::generate(listener.local_addr().unwrap().port()).unwrap();
    (listener, identity)
}

#[test]
fn authenticated_tls_roundtrip() {
    let (listener, identity) = fixture();
    let discovery = identity.discovery().clone();
    let server = thread::spawn(move || {
        let (tcp, _) = listener.accept().unwrap();
        let mut socket = identity.accept(tcp).unwrap();
        socket
            .get_mut()
            .sock
            .finish_handshake(Duration::from_secs(2))
            .unwrap();
        let message = socket.read().unwrap();
        assert_eq!(message.to_text().unwrap(), "application payload");
        socket.send(message).unwrap();
    });
    let mut socket = connect(&discovery, Duration::from_secs(2)).unwrap();
    socket
        .send(tungstenite::Message::Text("application payload".into()))
        .unwrap();
    assert_eq!(
        socket.read().unwrap().to_text().unwrap(),
        "application payload"
    );
    server.join().unwrap();
}

#[test]
fn wrong_token_and_stale_instance_cannot_upgrade() {
    for stale_instance in [false, true] {
        let (listener, identity) = fixture();
        let mut discovery = identity.discovery().clone();
        if stale_instance {
            discovery.instance_id = uuid::Uuid::new_v4().to_string();
        } else {
            discovery.auth_token = "a".repeat(64);
        }
        let server = thread::spawn(move || {
            assert!(identity.accept(listener.accept().unwrap().0).is_err());
        });
        assert!(connect(&discovery, Duration::from_secs(1)).is_err());
        server.join().unwrap();
    }
}

#[test]
fn valid_but_different_certificate_cannot_impersonate_pinned_server() {
    let (listener, identity) = fixture();
    let mut discovery = ServerIdentity::generate(19820).unwrap().discovery().clone();
    discovery.endpoint = identity.discovery().endpoint.clone();
    discovery.auth_token = identity.discovery().auth_token.clone();
    discovery.instance_id = identity.discovery().instance_id.clone();
    let server = thread::spawn(move || {
        assert!(identity.accept(listener.accept().unwrap().0).is_err());
    });
    assert!(connect(&discovery, Duration::from_secs(1)).is_err());
    server.join().unwrap();
}

#[test]
fn tls_failure_never_sends_plaintext_credential_or_upgrade() {
    let (listener, identity) = fixture();
    let discovery = identity.discovery().clone();
    let token = discovery.auth_token.clone();
    let server = thread::spawn(move || {
        let (mut tcp, _) = listener.accept().unwrap();
        tcp.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
        let mut bytes = [0u8; 8192];
        let count = tcp.read(&mut bytes).unwrap();
        assert!(!bytes[..count]
            .windows(token.len())
            .any(|w| w == token.as_bytes()));
        assert!(!bytes[..count].windows(4).any(|w| w == b"GET "));
        tcp.write_all(b"HTTP/1.1 101 Switching Protocols\r\n\r\n")
            .unwrap();
    });
    assert!(connect(&discovery, Duration::from_secs(1)).is_err());
    server.join().unwrap();
}

#[test]
fn plaintext_subscribe_is_rejected_before_dispatch() {
    let (listener, identity) = fixture();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        assert!(identity.accept(listener.accept().unwrap().0).is_err());
    });
    let mut tcp = TcpStream::connect(address).unwrap();
    tcp.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\n\r\n")
        .unwrap();
    server.join().unwrap();
}

#[test]
fn slow_handshake_has_an_absolute_deadline() {
    let (listener, identity) = fixture();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let start = Instant::now();
        assert!(identity.accept(listener.accept().unwrap().0).is_err());
        assert!(start.elapsed() < HANDSHAKE_TIMEOUT + Duration::from_secs(2));
    });
    let _tcp = TcpStream::connect(address).unwrap();
    server.join().unwrap();
}

#[test]
fn discovery_is_bounded_strict_and_redacts_credentials() {
    let identity = ServerIdentity::generate(19820).unwrap();
    let discovery = identity.discovery();
    assert!(discovery.validate().is_ok());
    assert!(!format!("{discovery:?}").contains(&discovery.auth_token));
    for endpoint in [
        "ws://127.0.0.1:19820/",
        "wss://localhost:19820/",
        "wss://evil:19820/",
        "wss://127.0.0.1:19820/?token=x",
    ] {
        let mut bad = discovery.clone();
        bad.endpoint = endpoint.into();
        assert!(bad.validate().is_err());
    }
    let mut bad = discovery.clone();
    bad.certificate_sha256 = "0".repeat(64);
    assert!(bad.validate().is_err());
    bad.certificate_der_base64 = "a".repeat(8193);
    assert!(bad.validate().is_err());
    let restarted = ServerIdentity::generate(19820).unwrap();
    assert_ne!(discovery.auth_token, restarted.discovery().auth_token);
    assert_ne!(discovery.instance_id, restarted.discovery().instance_id);
    assert_ne!(
        discovery.certificate_sha256,
        restarted.discovery().certificate_sha256
    );
}
