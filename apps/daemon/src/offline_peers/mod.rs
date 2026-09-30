//! Administrator-owned offline peer configuration and authenticated connectivity checks.
//! Raster checks prove transport only and grant no Hook delivery or delegation authority.
use super::*;

mod catalog;
mod handshake;
mod raster;
mod transfers;
mod transport;
mod trust;
use trust::{Peer, State};

pub(crate) struct OfflinePeers {
    path: PathBuf,
    state: Mutex<State>,
    probing: AtomicBool,
    catalog_fetching: AtomicBool,
    raster_receiving: AtomicBool,
    transfers: Mutex<transfers::Store>,
    transfer_work: AtomicBool,
    transfer_receiving: AtomicBool,
    _owner: fs::File,
}

pub(crate) struct PeerError {
    pub status: u16,
    pub code: &'static str,
}
type PeerResult<T> = std::result::Result<T, PeerError>;
fn failure(status: u16, code: &'static str) -> PeerError {
    PeerError { status, code }
}

fn parse<T: serde::de::DeserializeOwned>(body: &str) -> PeerResult<T> {
    if body.len() > 16_384 {
        return Err(failure(413, "peer_body_too_large"));
    }
    serde_json::from_str(body).map_err(|_| failure(400, "peer_invalid_request"))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PutPeer {
    expected_revision: u64,
    peer: Peer,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RemovePeer {
    expected_revision: u64,
    peer_id: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProbePeer {
    peer_id: String,
}

impl OfflinePeers {
    pub fn new(root: &Path) -> anyhow::Result<Self> {
        let path = root.join("settings").join("offline-projection-peers.json");
        fs::create_dir_all(path.parent().expect("settings parent"))?;
        let owner = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path.with_extension("lock"))?;
        owner
            .try_lock_exclusive()
            .context("offline peer trust is already owned by another daemon")?;
        let state = trust::open(&path)?;
        Ok(Self {
            path,
            state: Mutex::new(state),
            probing: AtomicBool::new(false),
            catalog_fetching: AtomicBool::new(false),
            raster_receiving: AtomicBool::new(false),
            transfers: Mutex::new(transfers::Store::open(root)?),
            transfer_work: AtomicBool::new(false),
            transfer_receiving: AtomicBool::new(false),
            _owner: owner,
        })
    }

    pub fn handles(path: &str) -> bool {
        transfers::ROUTES.contains(&path)
            || matches!(
                path,
                "/v1/projection-peers"
                    | "/v1/projection-peers/probe"
                    | "/v1/projection-peers/raster-check"
                    | "/v1/projection-peer/raster-check"
                    | "/v1/projection-peer/handshake"
                    | "/v1/projection-peer/catalog"
                    | "/v1/projection-peer/transfer"
            )
    }

    pub fn handle(
        &self,
        request: &ParsedHttpRequest,
        admin: bool,
        registry: &SharedDeviceRegistryStore,
    ) -> PeerResult<Value> {
        let path = request.path.split('?').next().unwrap_or_default();
        if transfers::ROUTES.contains(&path) {
            return self.local_transfer(request, registry);
        }
        if path == "/v1/projection-peer/transfer" {
            if request.method != "POST" {
                return Err(failure(405, "peer_method_not_allowed"));
            }
            return self.accept_transfer(&request.body, registry);
        }
        if path == "/v1/projection-peer/raster-check" {
            if request.method != "POST" {
                return Err(failure(405, "peer_method_not_allowed"));
            }
            return self.accept_raster(&request.body);
        }
        if path == "/v1/projection-peer/catalog" {
            if request.method != "POST" {
                return Err(failure(405, "peer_method_not_allowed"));
            }
            return self.accept_catalog(parse(&request.body)?, registry);
        }
        if path == "/v1/projection-peer/handshake" {
            if request.method != "POST" {
                return Err(failure(405, "peer_method_not_allowed"));
            }
            return self.accept_handshake(parse(&request.body)?);
        }
        if !admin {
            return Err(failure(403, "peer_admin_required"));
        }
        if path == "/v1/projection-peers/raster-check" {
            if request.method != "POST" {
                return Err(failure(405, "peer_method_not_allowed"));
            }
            return self.probe_raster(&request.body);
        }
        if path == "/v1/projection-peers/probe" {
            if request.method != "POST" {
                return Err(failure(405, "peer_method_not_allowed"));
            }
            let input: ProbePeer = parse(&request.body)?;
            return self.probe(&input.peer_id);
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| failure(503, "peer_unavailable"))?;
        match request.method.as_str() {
            "GET" => Ok(state.document.view()),
            "PUT" => {
                let input: PutPeer = parse(&request.body)?;
                if input.expected_revision != state.document.revision {
                    return Err(failure(409, "peer_revision_conflict"));
                }
                input.peer.validate(&state.document.identity.key_id)?;
                let mut next = state.document.clone();
                if next.peers.len() >= trust::MAX_PEERS
                    && !next.peers.contains_key(&input.peer.peer_id)
                {
                    return Err(failure(429, "peer_limit_reached"));
                }
                next.peers.insert(input.peer.peer_id.clone(), input.peer);
                trust::commit(&self.path, &mut state, next)?;
                Ok(state.document.view())
            }
            "DELETE" => {
                let input: RemovePeer = parse(&request.body)?;
                if input.expected_revision != state.document.revision {
                    return Err(failure(409, "peer_revision_conflict"));
                }
                let mut next = state.document.clone();
                if next.peers.remove(&input.peer_id).is_none() {
                    return Err(failure(404, "peer_not_found"));
                }
                trust::commit(&self.path, &mut state, next)?;
                Ok(state.document.view())
            }
            _ => Err(failure(405, "peer_method_not_allowed")),
        }
    }
}
