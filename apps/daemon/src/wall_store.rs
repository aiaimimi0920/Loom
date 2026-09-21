//! Durable wall configuration and volatile endpoint presence have separate lifetimes.
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use fs2::FileExt;
use loom_protocol::wall::{
    validate_tile_endpoint, validate_wall_layout, TileEndpoint, WallLayout, WALL_MAX_REVISION,
    WALL_PROTOCOL_VERSION,
};
use serde::{Deserialize, Serialize};

mod catalog;
mod identification;
mod images;
mod input;
mod leases;
mod live;
mod persistence;
mod presentation;
mod surface_requests;
mod surfaces;
#[cfg(test)]
mod tests;
mod timing;

pub(crate) type SharedWallStore = Arc<WallStore>;
pub(crate) use identification::{WallIdentification, WallIdentificationOutcome};
pub(crate) use input::{WallInputBinding, WallInputTarget};
pub(crate) use presentation::{
    WallPresentation, WallPresentationMode, WallPresentationOutcome, WallPresentationReport,
};
use surface_requests::WallSurfaceRequests;
pub(crate) use surfaces::WallSurfaceLink;
pub(crate) use timing::{WallSceneReport, WallTiming};
type WallResult<T> = Result<T, WallStoreError>;
const MAX_ENDPOINTS: usize = 256;
const MAX_WALLS: usize = 64;
const MAX_DOCUMENT_BYTES: usize = 8 * 1024 * 1024;
const LEASE_TTL: Duration = Duration::from_secs(15);

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub(crate) struct WallStoreError {
    pub status: u16,
    pub code: &'static str,
    pub message: &'static str,
}

impl WallStoreError {
    pub(crate) fn new(status: u16, code: &'static str, message: &'static str) -> Self {
        Self {
            status,
            code,
            message,
        }
    }

    fn invalid(message: &'static str) -> Self {
        Self::new(400, "wall_invalid", message)
    }

    fn conflict(message: &'static str) -> Self {
        Self::new(409, "wall_conflict", message)
    }

    fn unavailable() -> Self {
        Self::new(
            503,
            "wall_store_unavailable",
            "wall storage requires recovery",
        )
    }
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WallDocument {
    storage_version: u32,
    revision: u64,
    endpoints: Vec<TileEndpoint>,
    layouts: Vec<WallLayout>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    presentations: Vec<WallPresentation>,
}

pub(crate) struct WallStore {
    path: PathBuf,
    state: Mutex<WallState>,
    surface_links: Mutex<BTreeMap<(String, String), WallSurfaceLink>>,
    // The OS releases this exclusive writer lock on normal exit and process failure.
    _writer_lock: File,
}

struct WallState {
    document: WallDocument,
    timeline: timing::WallTimeline,
    leases: BTreeMap<String, EndpointLease>,
    write_failed: bool,
}

struct EndpointLease {
    id: String,
    deadline: Instant,
    sequence: u64,
    applied_revision: Option<u64>,
    scene: Option<WallSceneReport>,
    presentation: Option<WallPresentationReport>,
    identification: Option<identification::IdentificationControl>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WallStateSnapshot {
    protocol_version: &'static str,
    pub revision: u64,
    pub endpoints: Vec<EndpointStatus>,
    pub layouts: Vec<WallLayout>,
    pub timing: WallTiming,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub presentations: Vec<WallPresentation>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EndpointStatus {
    pub endpoint: TileEndpoint,
    pub online: bool,
    pub applied_revision: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene: Option<WallSceneReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presentation: Option<WallPresentationReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identification: Option<WallIdentification>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EndpointLeaseResponse {
    protocol_version: &'static str,
    pub lease_id: String,
    lease_ttl_ms: u64,
}

impl WallStore {
    fn lock(&self) -> WallResult<MutexGuard<'_, WallState>> {
        let state = self
            .state
            .lock()
            .map_err(|_| WallStoreError::unavailable())?;
        if state.write_failed {
            return Err(WallStoreError::unavailable());
        }
        Ok(state)
    }
}
