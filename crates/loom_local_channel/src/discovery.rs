use anyhow::{ensure, Context, Result};
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

/// Serialized only into the private discovery manifest, not an HTTP or IPC DTO.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeDiscovery {
    pub protocol: String,
    pub instance_id: String,
    pub endpoint: String,
    pub certificate_der_base64: String,
    pub certificate_sha256: String,
    pub auth_token: String,
}

impl std::fmt::Debug for BridgeDiscovery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BridgeDiscovery")
            .field("instance_id", &self.instance_id)
            .field("endpoint", &self.endpoint)
            .field("auth_token", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

impl BridgeDiscovery {
    pub(crate) fn proof_context(&self) -> Vec<u8> {
        format!("{}:{}", self.instance_id, self.auth_token).into_bytes()
    }

    pub fn validate(&self) -> Result<(SocketAddr, Vec<u8>)> {
        ensure!(
            self.protocol == "loom.local-bridge.v1",
            "Unsupported local bridge protocol"
        );
        ensure!(
            uuid::Uuid::parse_str(&self.instance_id).is_ok(),
            "Invalid bridge instance"
        );
        ensure!(
            self.auth_token.len() == 64 && self.auth_token.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid bridge credential"
        );
        // No DNS, alternate hosts, userinfo, query parameters, paths, or plaintext fallback.
        let port = self
            .endpoint
            .strip_prefix("wss://127.0.0.1:")
            .and_then(|s| s.strip_suffix('/'))
            .and_then(|s| s.parse::<u16>().ok())
            .filter(|p| *p != 0)
            .context("Invalid local bridge endpoint")?;
        ensure!(
            self.endpoint == format!("wss://127.0.0.1:{port}/"),
            "Noncanonical bridge endpoint"
        );
        ensure!(
            self.certificate_der_base64.len() <= 8192,
            "Bridge certificate exceeds limit"
        );
        let certificate = base64::engine::general_purpose::STANDARD
            .decode(&self.certificate_der_base64)
            .context("Invalid bridge certificate encoding")?;
        ensure!(
            self.certificate_sha256 == format!("{:x}", Sha256::digest(&certificate)),
            "Bridge certificate pin mismatch"
        );
        Ok((
            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)),
            certificate,
        ))
    }
}
