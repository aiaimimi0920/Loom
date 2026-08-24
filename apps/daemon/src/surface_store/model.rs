// Shared Surface store types, persisted schema, limits, and error contracts.
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read as _, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use loom_protocol::{
    is_safe_surface_identifier, validate_surface_action_cancel_request,
    validate_surface_confirmation_decision, validate_surface_confirmation_request,
    validate_surface_event, validate_surface_patch, validate_surface_protocol,
    validate_surface_resource, validate_surface_snapshot, SurfaceActionAck,
    SurfaceActionCancelRequest, SurfaceActionRisk, SurfaceActionStatus,
    SurfaceAttachmentDescriptor, SurfaceConfirmationDecision, SurfaceConfirmationRequest,
    SurfaceEvent, SurfaceEventClass, SurfaceExecutionError, SurfaceExecutionFailure,
    SurfaceHostCapabilities, SurfaceInstanceDescriptor, SurfaceInstanceMode,
    SurfaceInstancePersistence, SurfaceLifecycleEvent, SurfaceLifecycleState, SurfaceNode,
    SurfacePatch, SurfacePatchOperation, SurfacePortValue, SurfacePreviewCommit,
    SurfaceResultCommit, SurfaceSnapshot,
};
use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

use super::{create_sensitive_temporary, replace_sensitive_file, unix_time_millis};

const SURFACE_STORE_SCHEMA_VERSION: u32 = 1;
const MAX_SURFACE_STORE_BYTES: usize = 64 * 1024 * 1024;
const MAX_SURFACE_STORE_JSON_DEPTH: usize = 32;
const MAX_PENDING_SURFACE_EVENTS: usize = 1024;
const MAX_PENDING_SURFACE_CONFIRMATIONS: usize = 64;
const SURFACE_CONFIRMATION_TTL_MILLIS: u64 = 2 * 60 * 1000;

pub(crate) type SharedSurfaceInstanceStore = Arc<Mutex<SurfaceInstanceStore>>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SurfaceAttachmentRecord {
    pub descriptor: SurfaceAttachmentDescriptor,
    pub lifecycle: SurfaceLifecycleState,
    #[serde(default)]
    pub lifecycle_revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_capabilities: Option<SurfaceHostCapabilities>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<SurfaceSnapshot>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SurfaceInstanceRecord {
    pub descriptor: SurfaceInstanceDescriptor,
    #[serde(default)]
    pub attachments: BTreeMap<String, SurfaceAttachmentRecord>,
    #[serde(default)]
    pub authoritative_state: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_preview: Option<SurfacePreviewCommit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_result: Option<SurfaceResultCommit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_failure: Option<SurfaceExecutionFailure>,
    #[serde(default)]
    pub pending_events: Vec<SurfaceEvent>,
    #[serde(default)]
    pub event_acks: BTreeMap<String, SurfaceActionAck>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub pending_confirmations: BTreeMap<String, SurfacePendingConfirmation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub migration_history: Vec<SurfaceMigrationCheckpoint>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SurfaceMigrationCheckpoint {
    pub art_version: String,
    pub package_digest: String,
    pub state_schema_version: u32,
    #[serde(default)]
    pub authoritative_state: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_preview: Option<SurfacePreviewCommit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_result: Option<SurfaceResultCommit>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SurfacePendingConfirmation {
    pub request: SurfaceConfirmationRequest,
    pub event: SurfaceEvent,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SurfaceConfirmationResolution {
    Approved {
        event: SurfaceEvent,
        ack: SurfaceActionAck,
    },
    Rejected {
        ack: SurfaceActionAck,
    },
    Expired {
        ack: SurfaceActionAck,
    },
}

#[derive(Debug, Error)]
pub(crate) enum SurfaceStoreError {
    #[error("Surface store I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("Surface store JSON is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported Surface store schema {0}")]
    UnsupportedSchema(u32),
    #[error("invalid Surface request: {0}")]
    Invalid(String),
    #[error("Surface resource was not found: {0}")]
    NotFound(String),
    #[error("Surface state conflict: {0}")]
    Conflict(String),
}

impl SurfaceStoreError {
    pub(crate) fn status_code(&self) -> u16 {
        match self {
            Self::Invalid(_) | Self::Json(_) => 400,
            Self::NotFound(_) => 404,
            Self::Conflict(_) => 409,
            Self::Io(_) | Self::UnsupportedSchema(_) => 500,
        }
    }

    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::Invalid(_) | Self::Json(_) => "invalid_surface_request",
            Self::NotFound(_) => "surface_not_found",
            Self::Conflict(_) => "surface_conflict",
            Self::Io(_) => "surface_store_io_failed",
            Self::UnsupportedSchema(_) => "surface_store_schema_unsupported",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SurfaceStoreDocument {
    schema_version: u32,
    #[serde(default)]
    instances: BTreeMap<String, SurfaceInstanceRecord>,
}

pub(crate) struct SurfaceInstanceStore {
    path: PathBuf,
    instances: BTreeMap<String, SurfaceInstanceRecord>,
    /// The document bytes last known to be on disk, or `None` when nothing is known to be there
    /// yet. `persist` compares the bytes it is about to write against this and returns without
    /// touching the filesystem when they are identical, which is the common case: every mutation
    /// of a temporary instance is filtered out of the document, and several of the per-event
    /// mutations rewrite state that the document does not carry.
    persisted: Option<Vec<u8>>,
}
