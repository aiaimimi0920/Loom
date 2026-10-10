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
