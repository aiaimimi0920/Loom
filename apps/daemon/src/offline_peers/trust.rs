//! Private local trust state. Remote messages never add or replace trusted peers.
use super::*;
use sha2::{Digest, Sha256};

pub(super) const MAX_PEERS: usize = 16;
// Sixteen peers may each carry an 8 KiB public LAN certificate plus bounded metadata.
const MAX_DOCUMENT_BYTES: u64 = 192 * 1024;

#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Peer {
    pub peer_id: String,
    pub name: String,
    pub origin: String,
    pub public_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub certificate: Option<String>,
    pub enabled: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Document {
    schema_version: u32,
    pub revision: u64,
    pub identity: SigningKeyDocument,
    pub peers: BTreeMap<String, Peer>,
}

pub(super) struct State {
    pub document: Document,
    pub nonces: BTreeMap<String, u64>,
}

pub(super) fn peer_id(public_key: &str) -> PeerResult<String> {
    if public_key.len() != 44 {
        return Err(failure(400, "peer_invalid_key"));
    }
    let bytes = BASE64
        .decode(public_key)
        .map_err(|_| failure(400, "peer_invalid_key"))?;
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| failure(400, "peer_invalid_key"))?;
    let key = VerifyingKey::from_bytes(&bytes).map_err(|_| failure(400, "peer_invalid_key"))?;
    if key.is_weak() || BASE64.encode(bytes) != public_key {
        return Err(failure(400, "peer_invalid_key"));
    }
    Ok(format!("loom-{:x}", Sha256::digest(bytes)))
}

impl Peer {
    pub fn tls_certificate(&self) -> PeerResult<Option<reqwest::Certificate>> {
        let Some(encoded) = &self.certificate else {
            return Ok(None);
        };
        if encoded.is_empty() || encoded.len() > 8192 || !self.origin.starts_with("https://") {
            return Err(failure(400, "peer_invalid_certificate"));
        }
        let bytes = BASE64
            .decode(encoded)
            .map_err(|_| failure(400, "peer_invalid_certificate"))?;
        if BASE64.encode(&bytes) != *encoded {
            return Err(failure(400, "peer_invalid_certificate"));
        }
        let certificate = reqwest::Certificate::from_der(&bytes)
            .map_err(|_| failure(400, "peer_invalid_certificate"))?;
        Ok(Some(certificate))
    }

    pub fn validate(&self, local_id: &str) -> PeerResult<()> {
        if self.peer_id != peer_id(&self.public_key)?
            || self.peer_id == local_id
            || self.name.trim().is_empty()
            || self.name.len() > 128
            || self.name.chars().any(char::is_control)
            || !loom_protocol::projection::projection_origin_valid(&self.origin)
        {
            return Err(failure(400, "peer_invalid_configuration"));
        }
        if let Some(certificate) = self.tls_certificate()? {
            // from_der only wraps bytes with rustls; building validates the X.509 trust anchor.
            reqwest::blocking::Client::builder()
                .no_proxy()
                .tls_built_in_root_certs(false)
                .add_root_certificate(certificate)
                .build()
                .map_err(|_| failure(400, "peer_invalid_certificate"))?;
        }
        Ok(())
    }
}

impl Document {
    pub fn identity_view(&self) -> Value {
        json!({"peerId": self.identity.key_id, "publicKey": self.identity.public_key})
    }

    pub fn view(&self) -> Value {
        json!({"identity": self.identity_view(), "revision": self.revision,
            "peers": self.peers.values().collect::<Vec<_>>(), "deliveryAvailable": false})
    }

    fn validate(&self) -> PeerResult<()> {
        if self.schema_version != 1
            || self.peers.len() > MAX_PEERS
            || self.identity.key_id != peer_id(&self.identity.public_key)?
        {
            return Err(failure(503, "peer_storage_invalid"));
        }
        // Also check that the stored private key actually matches the advertised public key.
        sign_message(&self.identity, b"loom.offline-peer.key-check")
            .map_err(|_| failure(503, "peer_storage_invalid"))?;
        for (id, peer) in &self.peers {
            peer.validate(&self.identity.key_id)?;
            if id != &peer.peer_id {
                return Err(failure(503, "peer_storage_invalid"));
            }
        }
        Ok(())
    }
}

pub(super) fn open(path: &Path) -> anyhow::Result<State> {
    let document = match fs::File::open(path) {
        Ok(file) => {
            let mut bytes = Vec::new();
            file.take(MAX_DOCUMENT_BYTES + 1).read_to_end(&mut bytes)?;
            anyhow::ensure!(
                bytes.len() as u64 <= MAX_DOCUMENT_BYTES,
                "offline peer document too large"
            );
            let document: Document = serde_json::from_slice(&bytes)
                .context("parse offline peer trust; refusing to reset existing identity")?;
            document
                .validate()
                .map_err(|_| anyhow::anyhow!("invalid offline peer trust document"))?;
            document
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {
            let mut identity = generate_signing_key("offline-peer");
            identity.key_id = peer_id(&identity.public_key)
                .map_err(|_| anyhow::anyhow!("invalid generated offline peer identity"))?;
            let document = Document {
                schema_version: 1,
                revision: 0,
                identity,
                peers: BTreeMap::new(),
            };
            write_json_atomically(path, &document)?;
            document
        }
        Err(error) => return Err(error.into()),
    };
    Ok(State {
        document,
        nonces: BTreeMap::new(),
    })
}

pub(super) fn commit(path: &Path, state: &mut State, mut next: Document) -> PeerResult<()> {
    next.revision = state
        .document
        .revision
        .checked_add(1)
        .ok_or_else(|| failure(409, "peer_revision_exhausted"))?;
    next.validate()?;
    let mut bytes =
        serde_json::to_vec_pretty(&next).map_err(|_| failure(503, "peer_storage_unavailable"))?;
    bytes.push(b'\n');
    if bytes.len() as u64 > MAX_DOCUMENT_BYTES {
        return Err(failure(413, "peer_storage_too_large"));
    }
    write_bytes_atomically(path, &bytes, AtomicWritePermissions::Restrict)
        .map_err(|_| failure(503, "peer_storage_unavailable"))?;
    state.document = next;
    Ok(())
}
