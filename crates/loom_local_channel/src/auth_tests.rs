use super::*;
use rustls::{
    pki_types::{CertificateDer, ServerName},
    ClientConfig, ClientConnection, RootCertStore, StreamOwned,
};
use std::{
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use tungstenite::client::IntoClientRequest;

fn pinned_tls(
    discovery: &BridgeDiscovery,
) -> (StreamOwned<ClientConnection, DeadlineStream>, String) {
    let (address, der) = discovery.validate().unwrap();
    let mut roots = RootCertStore::empty();
    roots.add(CertificateDer::from(der)).unwrap();
    let config =
        ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_protocol_versions(&[&rustls::version::TLS13])
            .unwrap()
            .with_root_certificates(roots)
            .with_no_client_auth();
    let mut socket = DeadlineStream::new(
        TcpStream::connect(address).unwrap(),
        Instant::now() + HANDSHAKE_TIMEOUT,
    )
    .unwrap();
    let mut tls =
        ClientConnection::new(Arc::new(config), ServerName::try_from("localhost").unwrap())
            .unwrap();
    while tls.is_handshaking() {
        tls.complete_io(&mut socket).unwrap();
    }
    let proof = tls
        .export_keying_material(
            [0u8; 32],
            AUTH_EXPORTER_LABEL,
            Some(&discovery.proof_context()),
        )
        .unwrap();
    (StreamOwned::new(tls, socket), proof_hex(&proof))
}

#[test]
fn captured_proof_cannot_authenticate_another_tls_session_and_origin_is_denied() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let identity = ServerIdentity::generate(listener.local_addr().unwrap().port()).unwrap();
    let discovery = identity.discovery().clone();
    let server = thread::spawn(move || {
        assert!(identity.accept(listener.accept().unwrap().0).is_ok());
        assert!(identity.accept(listener.accept().unwrap().0).is_err());
        assert!(identity.accept(listener.accept().unwrap().0).is_err());
    });
    let mut captured = String::new();
    for index in 0..3 {
        let (stream, proof) = pinned_tls(&discovery);
        let mut request = discovery.endpoint.as_str().into_client_request().unwrap();
        if index == 0 {
            captured = proof.clone();
        }
        if index == 1 {
            assert_ne!(proof, captured);
        }
        let supplied = if index == 1 { &captured } else { &proof };
        request.headers_mut().insert(
            "authorization",
            format!("LoomBridgeProof {supplied}").parse().unwrap(),
        );
        request.headers_mut().insert(
            "x-loom-bridge-instance",
            discovery.instance_id.parse().unwrap(),
        );
        if index == 2 {
            request
                .headers_mut()
                .insert("origin", "http://localhost".parse().unwrap());
        }
        let result = tungstenite::client(request, stream);
        assert_eq!(result.is_ok(), index == 0);
    }
    server.join().unwrap();
}

#[test]
fn raw_upgrade_rejects_ambiguous_headers_and_legacy_credentials() {
    use std::io::{Read, Write};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let host = listener.local_addr().unwrap().to_string();
    let identity = ServerIdentity::generate(listener.local_addr().unwrap().port()).unwrap();
    let discovery = identity.discovery().clone();
    let server = thread::spawn(move || {
        for case in 0..8 {
            assert_eq!(
                identity.accept(listener.accept().unwrap().0).is_ok(),
                case == 0
            );
        }
    });
    for case in 0..8 {
        let (mut tls, proof) = pinned_tls(&discovery);
        let authorization = format!("Authorization: LoomBridgeProof {proof}\r\n");
        let instance = format!("X-Loom-Bridge-Instance: {}\r\n", discovery.instance_id);
        let mut extra = String::new();
        match case {
            1 => extra.push_str(&authorization),
            2 => extra.push_str(&instance),
            3 => extra.push_str(&format!("Host: {host}\r\n")),
            4 => extra.push_str("Sec-WebSocket-Protocol: loom.hook.v1\r\n"),
            5 => extra.push_str("Origin: http://localhost:1423\r\n"),
            _ => {}
        }
        let request_host = if case == 6 { "example.invalid" } else { &host };
        let target = if case == 7 { "/?token=legacy" } else { "/" };
        let request = format!(
            "GET {target} HTTP/1.1\r\nHost: {request_host}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n{authorization}{instance}{extra}\r\n"
        );
        tls.write_all(request.as_bytes()).unwrap();
        tls.flush().unwrap();
        let mut response = [0u8; 4096];
        let count = tls.read(&mut response).unwrap();
        let response = std::str::from_utf8(&response[..count]).unwrap();
        assert_eq!(
            response.starts_with("HTTP/1.1 101"),
            case == 0,
            "case {case}"
        );
        if case != 0 {
            assert!(response.starts_with("HTTP/1.1 4"), "case {case}");
        }
        assert!(!response.contains(&discovery.auth_token));
        assert!(!response.contains(&proof));
    }
    server.join().unwrap();
}

struct CaptureLogger(Mutex<String>);
static LOGGER: CaptureLogger = CaptureLogger(Mutex::new(String::new()));
impl log::Log for CaptureLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.target().starts_with("tungstenite::handshake")
    }
    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            let mut text = self.0.lock().unwrap();
            if text.len() < 65536 {
                text.push_str(&format!("{}\n", record.args()));
            }
        }
    }
    fn flush(&self) {}
}

#[test]
fn dependency_trace_logs_contain_only_connection_proof_not_discovery_token() {
    log::set_logger(&LOGGER).unwrap();
    log::set_max_level(log::LevelFilter::Trace);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let identity = ServerIdentity::generate(listener.local_addr().unwrap().port()).unwrap();
    let discovery = identity.discovery().clone();
    let server = thread::spawn(move || {
        identity.accept(listener.accept().unwrap().0).unwrap();
    });
    let _socket = connect(&discovery, Duration::from_secs(1)).unwrap();
    server.join().unwrap();
    let text = LOGGER.0.lock().unwrap();
    assert!(
        text.contains("LoomBridgeProof"),
        "TRACE request path must actually be exercised"
    );
    assert!(!text.contains(&discovery.auth_token));
}
