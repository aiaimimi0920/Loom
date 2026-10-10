use crate::{websocket_config, BridgeDiscovery, DeadlineStream, HANDSHAKE_TIMEOUT};
use anyhow::{Context, Result};
use base64::Engine;
use rand_core::{OsRng, RngCore};
use rustls::{
    pki_types::{CertificateDer, PrivatePkcs8KeyDer},
    ServerConfig, ServerConnection, StreamOwned,
};
use sha2::{Digest, Sha256};
use std::{
    net::TcpStream,
    sync::Arc,
    time::{Duration, Instant},
};
use subtle::ConstantTimeEq;
use tungstenite::{
    handshake::server::{ErrorResponse, Request, Response},
    WebSocket,
};

pub type ServerSocket = WebSocket<StreamOwned<ServerConnection, DeadlineStream>>;

fn build_server_config(
    cert: CertificateDer<'static>,
    key: PrivatePkcs8KeyDer<'static>,
) -> Result<ServerConfig> {
    // Errors from key-bearing configuration must not enter HTTP responses, logs,
    // or assertion output through the daemon's general anyhow error chain.
    ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|_| anyhow::anyhow!("Bridge TLS 1.3 configuration unavailable"))?
        .with_no_client_auth()
        .with_single_cert(vec![cert], key.into())
        .map_err(|_| anyhow::anyhow!("Bridge TLS identity configuration failed"))
}

fn single_header<'a>(request: &'a Request, name: &str) -> Option<&'a str> {
    let mut values = request.headers().get_all(name).iter();
    let value = values.next()?.to_str().ok()?;
    values.next().is_none().then_some(value)
}

/// Key material exists only in native memory; each start creates a new identity.
pub struct ServerIdentity {
    config: Arc<ServerConfig>,
    discovery: BridgeDiscovery,
}

impl ServerIdentity {
    pub fn generate(port: u16) -> Result<Self> {
        anyhow::ensure!(port != 0, "Bridge port is not assigned");
        let rcgen::CertifiedKey { cert, signing_key } =
            rcgen::generate_simple_self_signed(vec!["localhost".into()])?;
        let der = cert.der().clone();
        let key = PrivatePkcs8KeyDer::from(signing_key.serialize_der());
        let config = build_server_config(der.clone(), key)?;
        let mut random = [0u8; 32];
        OsRng
            .try_fill_bytes(&mut random)
            .context("Generate bridge credential")?;
        let token = random
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        Ok(Self {
            config: Arc::new(config),
            discovery: BridgeDiscovery {
                protocol: "loom.local-bridge.v1".into(),
                instance_id: uuid::Uuid::new_v4().to_string(),
                endpoint: format!("wss://127.0.0.1:{port}/"),
                certificate_der_base64: base64::engine::general_purpose::STANDARD.encode(&der),
                certificate_sha256: format!("{:x}", Sha256::digest(&der)),
                auth_token: token,
            },
        })
    }

    pub fn discovery(&self) -> &BridgeDiscovery {
        &self.discovery
    }

    pub fn accept(&self, tcp: TcpStream) -> Result<ServerSocket> {
        anyhow::ensure!(tcp.peer_addr()?.ip().is_loopback(), "Nonlocal bridge peer");
        let expected_host = tcp.local_addr()?.to_string();
        let mut stream = DeadlineStream::new(tcp, Instant::now() + HANDSHAKE_TIMEOUT)?;
        let mut tls = ServerConnection::new(Arc::clone(&self.config))?;
        while tls.is_handshaking() {
            tls.complete_io(&mut stream).context("Accept bridge TLS")?;
        }
        let proof = tls.export_keying_material(
            [0u8; 32],
            crate::AUTH_EXPORTER_LABEL,
            Some(&self.discovery.proof_context()),
        )?;
        let proof_hash: [u8; 32] = Sha256::digest(crate::proof_hex(&proof).as_bytes()).into();
        let stream = StreamOwned::new(tls, stream);
        let callback = |request: &Request,
                        response: Response|
         -> std::result::Result<Response, ErrorResponse> {
            let proof = single_header(request, "authorization")
                .and_then(|s| s.strip_prefix("LoomBridgeProof "))
                .filter(|s| s.len() == 64);
            let authorized = proof.is_some_and(|proof| {
                let supplied: [u8; 32] = Sha256::digest(proof.as_bytes()).into();
                bool::from(supplied.ct_eq(&proof_hash))
            });
            let instance = single_header(request, "x-loom-bridge-instance");
            if !authorized
                || instance != Some(self.discovery.instance_id.as_str())
                || single_header(request, "host") != Some(expected_host.as_str())
                || request.headers().contains_key("sec-websocket-protocol")
                || request.headers().contains_key("origin")
                || request.uri().authority().is_some()
                || request.uri().path() != "/"
                || request.uri().query().is_some()
            {
                return Err(tungstenite::http::Response::builder()
                    .status(401)
                    .body(Some("Bridge authentication required".into()))
                    .expect("static response"));
            }
            Ok(response)
        };
        let mut socket =
            tungstenite::accept_hdr_with_config(stream, callback, Some(websocket_config()))
                .map_err(|_| anyhow::anyhow!("Bridge TLS or WebSocket authentication failed"))?;
        socket
            .get_mut()
            .sock
            .finish_handshake(Duration::from_millis(100))?;
        Ok(socket)
    }
}

#[cfg(test)]
mod configuration_tests {
    use super::*;

    #[test]
    fn malformed_identity_returns_only_a_fixed_error_without_a_source_chain() {
        let rcgen::CertifiedKey { cert, signing_key } =
            rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let cases = [
            (
                CertificateDer::from(b"private-certificate-sentinel".to_vec()),
                PrivatePkcs8KeyDer::from(signing_key.serialize_der()),
            ),
            (
                cert.der().clone(),
                PrivatePkcs8KeyDer::from(b"private-key-sentinel".to_vec()),
            ),
        ];
        for (cert, key) in cases {
            let error = build_server_config(cert, key)
                .err()
                .expect("invalid identity must fail");
            assert_eq!(
                error.to_string(),
                "Bridge TLS identity configuration failed"
            );
            assert_eq!(error.chain().count(), 1);
            assert!(!format!("{error:?}").contains("sentinel"));
        }
    }
}
