//! Separate opt-in LAN HTTPS identity; never reuse the private local bridge token.
use crate::DeadlineStream;
use anyhow::{ensure, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use rustls::{
    pki_types::{CertificateDer, PrivatePkcs8KeyDer},
    ServerConfig, ServerConnection, StreamOwned,
};
use serde::{Deserialize, Serialize};
use std::{
    net::{Ipv4Addr, TcpStream},
    sync::Arc,
    time::{Duration, Instant},
};

pub type LanStream = StreamOwned<ServerConnection, DeadlineStream>;

// Intentionally no Debug: the persisted document contains a private key.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LanIdentityDocument {
    pub certificate: String,
    private_key: String,
}

impl LanIdentityDocument {
    pub fn generate(host: Ipv4Addr) -> Result<Self> {
        let rcgen::CertifiedKey { cert, signing_key } =
            rcgen::generate_simple_self_signed(vec![host.to_string()])?;
        Ok(Self {
            certificate: STANDARD.encode(cert.der()),
            private_key: STANDARD.encode(signing_key.serialize_der()),
        })
    }

    pub fn server(&self) -> Result<Arc<ServerConfig>> {
        ensure!(
            self.certificate.len() <= 8192 && self.private_key.len() <= 8192,
            "LAN identity exceeds limit"
        );
        let cert = STANDARD
            .decode(&self.certificate)
            .map_err(|_| anyhow::anyhow!("Invalid LAN certificate"))?;
        let key = STANDARD
            .decode(&self.private_key)
            .map_err(|_| anyhow::anyhow!("Invalid LAN key"))?;
        let config =
            ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_protocol_versions(&[&rustls::version::TLS13])?
                .with_no_client_auth()
                .with_single_cert(
                    vec![CertificateDer::from(cert)],
                    PrivatePkcs8KeyDer::from(key).into(),
                )
                .map_err(|_| anyhow::anyhow!("Invalid LAN TLS identity"))?;
        Ok(Arc::new(config))
    }
}

pub fn accept(identity: &LanIdentityDocument, tcp: TcpStream) -> Result<LanStream> {
    let mut stream = DeadlineStream::new(tcp, Instant::now() + Duration::from_secs(5))?;
    let mut tls = ServerConnection::new(identity.server()?)?;
    while tls.is_handshaking() {
        tls.complete_io(&mut stream)?;
    }
    // The same absolute deadline also covers the complete incoming HTTP request.
    Ok(StreamOwned::new(tls, stream))
}
