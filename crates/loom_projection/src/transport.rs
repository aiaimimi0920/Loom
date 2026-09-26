use crate::{error, validation, Identity, Peer, Result, MAX_PNG_BYTES, PROTOCOL};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use iroh::{
    endpoint::presets, Endpoint, EndpointAddr, RelayMode, RelayUrl, SecretKey, TransportAddr,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    str::FromStr,
    sync::Arc,
    time::Duration,
};
use tokio::sync::Mutex;

#[path = "transport_pull.rs"]
mod pull;
pub use pull::{PullRequest, SnapshotGrant, SnapshotProvider, TransferPath};

pub const ALPN: &[u8] = b"neuro/qr-projection/2";
const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_ACK_BYTES: usize = 64;
const MAX_FRAME_BYTES: usize = MAX_PNG_BYTES + MAX_HEADER_BYTES + 4;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Snapshot {
    pub projection_id: String,
    pub source_session_id: String,
    pub revision: u64,
    pub digest: String,
    pub width: u32,
    pub height: u32,
    #[serde(with = "crate::runtime_model::png_base64")]
    pub png: Vec<u8>,
}

impl Snapshot {
    pub fn validate(&self) -> Result<()> {
        if !validation::projection_id(&self.projection_id)
            || !validation::identifier(&self.source_session_id)
            || !validation::revision(self.revision)
            || !validation::hex(&self.digest, 64)
            || self.png.is_empty()
            || self.png.len() > MAX_PNG_BYTES
            || self.width == 0
            || self.height == 0
            || self.width > 8192
            || self.height > 8192
            || u64::from(self.width) * u64::from(self.height) > 16_777_216
        {
            return Err(error(400, "projection_frame_invalid"));
        }
        let digest = format!("{:x}", Sha256::digest(&self.png));
        if digest != self.digest {
            return Err(error(422, "projection_image_digest_mismatch"));
        }
        let mut reader = image::ImageReader::with_format(
            std::io::Cursor::new(&self.png),
            image::ImageFormat::Png,
        );
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(8192);
        limits.max_image_height = Some(8192);
        limits.max_alloc = Some(80 * 1024 * 1024);
        reader.limits(limits);
        let decoded = reader
            .decode()
            .map_err(|_| error(422, "projection_invalid_image"))?;
        if decoded.width() != self.width || decoded.height() != self.height {
            return Err(error(422, "projection_dimensions_mismatch"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FrameHeader {
    protocol: String,
    projection_id: String,
    source_session_id: String,
    revision: u64,
    digest: String,
    width: u32,
    height: u32,
    png_length: u32,
}

impl From<&Snapshot> for FrameHeader {
    fn from(snapshot: &Snapshot) -> Self {
        Self {
            protocol: PROTOCOL.to_owned(),
            projection_id: snapshot.projection_id.clone(),
            source_session_id: snapshot.source_session_id.clone(),
            revision: snapshot.revision,
            digest: snapshot.digest.clone(),
            width: snapshot.width,
            height: snapshot.height,
            png_length: snapshot.png.len() as u32,
        }
    }
}

impl FrameHeader {
    fn into_snapshot(self, png: Vec<u8>) -> Snapshot {
        Snapshot {
            projection_id: self.projection_id,
            source_session_id: self.source_session_id,
            revision: self.revision,
            digest: self.digest,
            width: self.width,
            height: self.height,
            png,
        }
    }
}

#[derive(Clone)]
pub struct Transport {
    endpoint: Endpoint,
    send_gate: Arc<Mutex<()>>,
}

impl Transport {
    pub async fn bind(identity: &Identity, relay_urls: &[String]) -> Result<Self> {
        Self::bind_inner(identity, relay_urls, false).await
    }

    #[cfg(test)]
    pub(crate) async fn bind_loopback_for_test(identity: &Identity) -> Result<Self> {
        Self::bind_inner(identity, &[], true).await
    }

    async fn bind_inner(
        identity: &Identity,
        relay_urls: &[String],
        loopback: bool,
    ) -> Result<Self> {
        let mut relays = Vec::with_capacity(relay_urls.len());
        for relay in relay_urls {
            let parsed =
                RelayUrl::from_str(relay).map_err(|_| error(400, "projection_invalid_endpoint"))?;
            relays.push(parsed);
        }
        let relay_mode = if relays.is_empty() {
            RelayMode::Disabled
        } else {
            RelayMode::custom(relays)
        };
        let secret_key = SecretKey::from_bytes(&identity.key.to_bytes());
        let mut builder = Endpoint::builder(presets::Minimal)
            .secret_key(secret_key)
            .relay_mode(relay_mode)
            .alpns(vec![ALPN.to_vec()]);
        if loopback {
            builder = builder
                .clear_ip_transports()
                .bind_addr((Ipv4Addr::LOCALHOST, 0))
                .map_err(|_| error(503, "projection_transport_unavailable"))?;
        }
        let endpoint = builder
            .bind()
            .await
            .map_err(|_| error(503, "projection_transport_unavailable"))?;
        Ok(Self {
            endpoint,
            send_gate: Arc::new(Mutex::new(())),
        })
    }

    pub fn endpoint_address(&self) -> crate::EndpointAddress {
        let addr = self.endpoint.addr();
        let relay_url = addr.relay_urls().next().map(ToString::to_string);
        crate::EndpointAddress {
            endpoint_id: hex(self.endpoint.id().as_bytes()),
            addresses: addr
                .ip_addrs()
                .take(8)
                .map(|value| crate::Address {
                    ip: value.ip().to_string(),
                    port: value.port(),
                })
                .collect(),
            relay_url,
        }
    }

    pub async fn send_snapshot(&self, peer: &Peer, snapshot: Snapshot) -> Result<()> {
        snapshot.validate()?;
        let _gate = self.send_gate.lock().await;
        let address = endpoint_address(peer)?;
        let connection = tokio::time::timeout(
            Duration::from_secs(10),
            self.endpoint.connect(address, ALPN),
        )
        .await
        .map_err(|_| error(504, "projection_transport_unavailable"))?
        .map_err(|_| error(503, "projection_transport_unavailable"))?;
        verify_remote(&connection, peer)?;
        let (mut send, mut recv) = connection
            .open_bi()
            .await
            .map_err(|_| error(503, "projection_transport_unavailable"))?;
        write_frame(&mut send, &snapshot).await?;
        send.finish()
            .map_err(|_| error(503, "projection_transport_unavailable"))?;
        let ack = recv
            .read_to_end(MAX_ACK_BYTES)
            .await
            .map_err(|_| error(503, "projection_transport_unavailable"))?;
        if ack.as_slice() != b"ok" {
            return Err(error(502, "projection_frame_invalid"));
        }
        Ok(())
    }

    pub async fn receive_snapshot(
        &self,
        peer: &Peer,
        projection_id: &str,
        source_session_id: &str,
        minimum_revision: u64,
    ) -> Result<Snapshot> {
        if !validation::projection_id(projection_id)
            || !validation::identifier(source_session_id)
            || !validation::revision(minimum_revision)
        {
            return Err(error(400, "projection_frame_invalid"));
        }
        let incoming = self
            .endpoint
            .accept()
            .await
            .ok_or(error(503, "projection_transport_unavailable"))?;
        let connection = incoming
            .await
            .map_err(|_| error(503, "projection_transport_unavailable"))?;
        verify_remote(&connection, peer)?;
        let (mut send, mut recv) = connection
            .accept_bi()
            .await
            .map_err(|_| error(503, "projection_transport_unavailable"))?;
        let snapshot = read_frame(&mut recv).await?;
        if snapshot.projection_id != projection_id
            || snapshot.source_session_id != source_session_id
            || snapshot.revision < minimum_revision
        {
            return Err(error(409, "projection_revision_conflict"));
        }
        snapshot.validate()?;
        send.write_all(b"ok")
            .await
            .map_err(|_| error(503, "projection_transport_unavailable"))?;
        send.finish()
            .map_err(|_| error(503, "projection_transport_unavailable"))?;
        let _ = tokio::time::timeout(Duration::from_secs(5), connection.closed()).await;
        Ok(snapshot)
    }

    pub async fn close(&self) {
        self.endpoint.close().await;
    }
}

async fn write_frame(send: &mut iroh::endpoint::SendStream, snapshot: &Snapshot) -> Result<()> {
    let header = serde_json::to_vec(&FrameHeader::from(snapshot))
        .map_err(|_| error(400, "projection_frame_invalid"))?;
    if header.is_empty()
        || header.len() > MAX_HEADER_BYTES
        || snapshot.png.len() > u32::MAX as usize
    {
        return Err(error(413, "projection_request_budget"));
    }
    let frame_length = 4usize
        .checked_add(header.len())
        .and_then(|length| length.checked_add(snapshot.png.len()))
        .ok_or(error(413, "projection_request_budget"))?;
    if frame_length > MAX_FRAME_BYTES {
        return Err(error(413, "projection_request_budget"));
    }
    send.write_all(&(header.len() as u32).to_be_bytes())
        .await
        .map_err(|_| error(503, "projection_transport_unavailable"))?;
    send.write_all(&header)
        .await
        .map_err(|_| error(503, "projection_transport_unavailable"))?;
    send.write_all(&snapshot.png)
        .await
        .map_err(|_| error(503, "projection_transport_unavailable"))?;
    Ok(())
}

async fn read_frame(recv: &mut iroh::endpoint::RecvStream) -> Result<Snapshot> {
    let mut length = [0u8; 4];
    recv.read_exact(&mut length)
        .await
        .map_err(|_| error(502, "projection_frame_invalid"))?;
    let header_length = u32::from_be_bytes(length) as usize;
    if header_length == 0 || header_length > MAX_HEADER_BYTES {
        return Err(error(413, "projection_request_budget"));
    }
    let mut header_bytes = vec![0u8; header_length];
    recv.read_exact(&mut header_bytes)
        .await
        .map_err(|_| error(502, "projection_frame_invalid"))?;
    let header: FrameHeader = serde_json::from_slice(&header_bytes)
        .map_err(|_| error(502, "projection_frame_invalid"))?;
    let png_length = header.png_length as usize;
    if png_length == 0 || png_length > MAX_PNG_BYTES {
        return Err(error(413, "projection_request_budget"));
    }
    let png = recv
        .read_to_end(png_length)
        .await
        .map_err(|_| error(413, "projection_request_budget"))?;
    if png.len() != png_length {
        return Err(error(502, "projection_frame_invalid"));
    }
    if header.protocol != PROTOCOL {
        return Err(error(400, "projection_frame_invalid"));
    }
    Ok(header.into_snapshot(png))
}

fn endpoint_address(peer: &Peer) -> Result<EndpointAddr> {
    let endpoint = peer
        .endpoint
        .as_ref()
        .ok_or(error(409, "projection_peer_unavailable"))?;
    let key_bytes = STANDARD
        .decode(&peer.public_key)
        .map_err(|_| error(400, "projection_invalid_endpoint"))?;
    let key_bytes: [u8; 32] = key_bytes
        .try_into()
        .map_err(|_| error(400, "projection_invalid_endpoint"))?;
    let key = iroh::PublicKey::from_bytes(&key_bytes)
        .map_err(|_| error(400, "projection_invalid_endpoint"))?;
    if endpoint.endpoint_id != hex(&key_bytes) {
        return Err(error(400, "projection_invalid_endpoint"));
    }
    let mut addresses = Vec::with_capacity(endpoint.addresses.len() + 1);
    for address in &endpoint.addresses {
        let ip =
            IpAddr::from_str(&address.ip).map_err(|_| error(400, "projection_invalid_endpoint"))?;
        addresses.push(TransportAddr::Ip(SocketAddr::new(ip, address.port)));
    }
    if let Some(relay_url) = &endpoint.relay_url {
        let relay =
            RelayUrl::from_str(relay_url).map_err(|_| error(400, "projection_invalid_endpoint"))?;
        addresses.push(TransportAddr::Relay(relay));
    }
    if addresses.is_empty() {
        return Err(error(409, "projection_peer_unavailable"));
    }
    Ok(EndpointAddr::from_parts(key, addresses))
}

fn verify_remote(connection: &iroh::endpoint::Connection, peer: &Peer) -> Result<()> {
    let key_bytes = STANDARD
        .decode(&peer.public_key)
        .map_err(|_| error(400, "projection_invalid_endpoint"))?;
    let key_bytes: [u8; 32] = key_bytes
        .try_into()
        .map_err(|_| error(400, "projection_invalid_endpoint"))?;
    let expected = iroh::PublicKey::from_bytes(&key_bytes)
        .map_err(|_| error(400, "projection_invalid_endpoint"))?;
    if connection.remote_id() != expected {
        connection.close(0x100u32.into(), b"peer identity mismatch");
        return Err(error(403, "projection_access_denied"));
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
