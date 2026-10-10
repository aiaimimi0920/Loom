use crate::{websocket_config, BridgeDiscovery, DeadlineStream, HANDSHAKE_TIMEOUT};
use anyhow::{ensure, Context, Result};
use rustls::{
    pki_types::{CertificateDer, ServerName},
    ClientConfig, ClientConnection, RootCertStore, StreamOwned,
};
use std::{
    net::TcpStream,
    sync::Arc,
    time::{Duration, Instant},
};
use tungstenite::{client::IntoClientRequest, http::HeaderValue, WebSocket};

pub type ClientSocket = WebSocket<StreamOwned<ClientConnection, DeadlineStream>>;

pub fn connect(discovery: &BridgeDiscovery, io_timeout: Duration) -> Result<ClientSocket> {
    let (address, der) = discovery.validate()?;
    let mut roots = RootCertStore::empty();
    roots
        .add(CertificateDer::from(der.clone()))
        .context("Invalid pinned bridge root")?;
    let config =
        ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_protocol_versions(&[&rustls::version::TLS13])?
            .with_root_certificates(roots)
            .with_no_client_auth();
    let deadline = Instant::now() + HANDSHAKE_TIMEOUT;
    let tcp =
        TcpStream::connect_timeout(&address, HANDSHAKE_TIMEOUT).context("Connect local bridge")?;
    let mut stream = DeadlineStream::new(tcp, deadline)?;
    let mut tls = ClientConnection::new(Arc::new(config), ServerName::try_from("localhost")?)?;
    while tls.is_handshaking() {
        tls.complete_io(&mut stream)
            .context("Authenticate bridge TLS peer")?;
    }
    // Standard WebPKI validates certificate usage/name and CertificateVerify signatures.
    // The exact leaf check additionally prevents chain substitution under the pinned root.
    ensure!(
        tls.peer_certificates()
            .and_then(|chain| chain.first())
            .is_some_and(|leaf| leaf.as_ref() == der),
        "Bridge leaf certificate mismatch"
    );
    // Never pass the long-lived token to tungstenite: its TRACE logs serialize headers.
    // Exporter output is bound to this TLS session and cannot authenticate another one.
    let proof = tls.export_keying_material(
        [0u8; 32],
        crate::AUTH_EXPORTER_LABEL,
        Some(&discovery.proof_context()),
    )?;
    let stream = StreamOwned::new(tls, stream);
    let mut request = discovery.endpoint.as_str().into_client_request()?;
    let mut auth = HeaderValue::from_str(&format!("LoomBridgeProof {}", crate::proof_hex(&proof)))?;
    auth.set_sensitive(true);
    request.headers_mut().insert("authorization", auth);
    request.headers_mut().insert(
        "x-loom-bridge-instance",
        HeaderValue::from_str(&discovery.instance_id)?,
    );
    // Never format handshake errors: HTTP requests/responses may contain credentials.
    let (mut socket, _) =
        tungstenite::client::client_with_config(request, stream, Some(websocket_config()))
            .map_err(|_| anyhow::anyhow!("Bridge WebSocket authentication failed"))?;
    socket.get_mut().sock.finish_handshake(io_timeout)?;
    Ok(socket)
}
