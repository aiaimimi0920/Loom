use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use loom_protocol::{SurfaceResourceDescriptor, SurfaceResourceLease};
use serde::{Deserialize, Serialize};
use thiserror::Error;

mod content;
mod garbage_collection;
mod leases;
mod persistence;

#[cfg(test)]
mod tests;

pub(crate) const MAX_SURFACE_RESOURCE_BYTES: usize = 16 * 1024 * 1024;
const DEFAULT_RESOURCE_LEASE_MILLIS: u64 = 15 * 60 * 1000;
const MAX_RESOURCE_LEASE_MILLIS: u64 = 60 * 60 * 1000;
/// Upper bound on live leases. The record carries no instance id, so the cap is global.
const MAX_ACTIVE_RESOURCE_LEASES: usize = 512;
/// Lease additions may be batched because every persistence pass rewrites and syncs the table.
const LEASE_PERSIST_DEBOUNCE_MILLIS: u64 = 250;
/// A fresh object exists briefly before its caller can persist the Surface snapshot referencing it.
pub(crate) const DEFAULT_RESOURCE_GC_MIN_AGE_MILLIS: u64 = 10 * 60 * 1000;
pub(crate) const MIN_RESOURCE_GC_AGE_MILLIS: u64 = 10 * 60 * 1000;

/// Complete result of one garbage-collection pass, including partial I/O failures.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SurfaceResourceGcOutcome {
    pub removed_objects: usize,
    pub removed_bytes: u64,
    pub removed_orphan_files: usize,
    pub retained_objects: usize,
    pub failures: usize,
}

pub(crate) type SharedSurfaceResourceStore = Arc<Mutex<SurfaceResourceStore>>;

#[derive(Debug, Error)]
pub(crate) enum SurfaceResourceStoreError {
    #[error("Surface resource I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("Surface resource metadata is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid Surface resource: {0}")]
    Invalid(String),
    #[error("Surface resource was not found: {0}")]
    NotFound(String),
    #[error("Surface resource lease was rejected: {0}")]
    LeaseRejected(String),
}

impl SurfaceResourceStoreError {
    pub(crate) fn status_code(&self) -> u16 {
        match self {
            Self::Invalid(_) | Self::Json(_) => 400,
            Self::LeaseRejected(_) => 403,
            Self::NotFound(_) => 404,
            Self::Io(_) => 500,
        }
    }

    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::Invalid(_) | Self::Json(_) => "invalid_surface_resource",
            Self::LeaseRejected(_) => "surface_resource_lease_rejected",
            Self::NotFound(_) => "surface_resource_not_found",
            Self::Io(_) => "surface_resource_io_failed",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredSurfaceResource {
    descriptor: SurfaceResourceDescriptor,
    created_at_ms: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct SurfaceResourcePayload {
    pub descriptor: SurfaceResourceDescriptor,
    pub bytes: Vec<u8>,
}

/// Content-addressed payload metadata, active leases, and verification stamps.
pub(crate) struct SurfaceResourceStore {
    root: PathBuf,
    resources: BTreeMap<String, StoredSurfaceResource>,
    leases: BTreeMap<String, SurfaceResourceLease>,
    /// Payload identity at the last successful hash verification in this process.
    verified: BTreeMap<String, PayloadStamp>,
    leases_dirty: bool,
    leases_persisted_at_ms: u64,
    /// Production construction enforces `MIN_RESOURCE_GC_AGE_MILLIS`.
    gc_min_age_ms: u64,
}

/// Size plus modification time. A missing stamp always forces a full payload re-hash.
type PayloadStamp = (u64, u128);
