//! Surface action definitions, lifecycle messages, and formal result envelopes.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::default_surface_protocol_version;
use super::resources::{
    SurfacePortValue, SurfaceResourceDescriptor, SurfaceResourceKind, SurfaceResourceLease,
};
use super::scene::{SurfaceEventClass, SurfacePatchOperation};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceActionRisk {
    Low,
    Medium,
    High,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceOfflinePolicy {
    Reject,
    Queue,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceActionConcurrency {
    ReplaceLatest,
    Serial,
    Parallel,
    RejectWhileRunning,
    Coalesce,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceActionDefinition {
    pub id: String,
    #[serde(default)]
    pub input_schema: Value,
    pub risk: SurfaceActionRisk,
    pub offline_policy: SurfaceOfflinePolicy,
    pub concurrency: SurfaceActionConcurrency,
    #[serde(default)]
    pub idempotent: bool,
    #[serde(default)]
    pub confirmation: bool,
    #[serde(default)]
    pub cancelable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub progress: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceActionStatus {
    Accepted,
    AwaitingConfirmation,
    Queued,
    Running,
    CancelRequested,
    Cancelled,
    Succeeded,
    Failed,
    Interrupted,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceActionAck {
    #[serde(default = "default_surface_protocol_version")]
    pub protocol_version: String,
    pub instance_id: String,
    pub event_id: String,
    pub request_id: String,
    pub accepted: bool,
    pub status: SurfaceActionStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<SurfaceExecutionError>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceConfirmationRequest {
    #[serde(default = "default_surface_protocol_version")]
    pub protocol_version: String,
    pub confirmation_id: String,
    pub instance_id: String,
    pub attachment_id: String,
    pub device_id: String,
    pub hook_node_id: String,
    pub event_id: String,
    pub request_id: String,
    pub action_id: String,
    pub risk: SurfaceActionRisk,
    pub expires_at_ms: u64,
    #[serde(default)]
    pub payload: Value,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceConfirmationDecision {
    #[serde(default = "default_surface_protocol_version")]
    pub protocol_version: String,
    pub confirmation_id: String,
    pub instance_id: String,
    pub attachment_id: String,
    pub device_id: String,
    pub approved: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceActionCancelRequest {
    #[serde(default = "default_surface_protocol_version")]
    pub protocol_version: String,
    pub instance_id: String,
    pub request_id: String,
    pub device_id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceActionProgress {
    #[serde(default = "default_surface_protocol_version")]
    pub protocol_version: String,
    pub instance_id: String,
    pub request_id: String,
    pub generation: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_key: Option<String>,
}

/// Immutable input delivered to an Art runtime for one declared Surface action.
///
/// The runtime never receives Hook internals or credentials through this
/// envelope. Framework credential grants continue to use the existing broker.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceActionInvocation {
    #[serde(default = "default_surface_protocol_version")]
    pub protocol_version: String,
    pub instance_id: String,
    pub attachment_id: String,
    pub request_id: String,
    pub event_id: String,
    pub action_id: String,
    pub event_class: SurfaceEventClass,
    pub generation: u64,
    pub base_revision: u64,
    #[serde(default)]
    pub payload: Value,
    #[serde(default)]
    pub authoritative_state: Value,
}

/// A revision-free scene/state update returned by an Art runtime.
///
/// Loom chooses the target's current base revision and the next revision while
/// holding the instance-store lock, so package code cannot forge or skip the
/// authoritative revision sequence.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceActionPatchUpdate {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachment_id: Option<String>,
    #[serde(default)]
    pub operations: Vec<SurfacePatchOperation>,
    #[serde(default)]
    pub state_patch: Value,
    #[serde(default)]
    pub resources: Vec<SurfaceResourceDescriptor>,
    #[serde(default)]
    pub resource_leases: Vec<SurfaceResourceLease>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceActionPreviewUpdate {
    pub port_id: String,
    pub value: SurfacePortValue,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceActionResultUpdate {
    #[serde(default)]
    pub outputs: BTreeMap<String, SurfacePortValue>,
    #[serde(default)]
    pub state_patch: Value,
}

/// Package-to-host resource upload used only inside a trusted Surface action
/// response. Runtime patches reference it as `surface-upload:<id>`; Loom stores
/// and leases the bytes, then replaces the placeholder before anything crosses
/// the Hook control channel.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceActionResourceUpload {
    pub id: String,
    pub kind: SurfaceResourceKind,
    pub mime: String,
    pub data_base64: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease_millis: Option<u64>,
}

/// Strict response body expected under the tool output's `surfaceAction` key.
/// Runtimes can update the UI, publish a preview, and atomically publish formal
/// outputs in one action, but they cannot directly choose revisions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceActionResponse {
    #[serde(default = "default_surface_protocol_version")]
    pub protocol_version: String,
    #[serde(default)]
    pub patches: Vec<SurfaceActionPatchUpdate>,
    #[serde(default)]
    pub resource_uploads: Vec<SurfaceActionResourceUpload>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<SurfaceActionPreviewUpdate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<SurfaceActionResultUpdate>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfacePreviewCommit {
    #[serde(default = "default_surface_protocol_version")]
    pub protocol_version: String,
    pub instance_id: String,
    pub request_id: String,
    pub generation: u64,
    pub preview_revision: u64,
    pub port_id: String,
    pub value: SurfacePortValue,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceResultCommit {
    #[serde(default = "default_surface_protocol_version")]
    pub protocol_version: String,
    pub instance_id: String,
    pub request_id: String,
    pub generation: u64,
    pub result_revision: u64,
    #[serde(default)]
    pub outputs: BTreeMap<String, SurfacePortValue>,
    #[serde(default)]
    pub state_patch: Value,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceExecutionError {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceExecutionFailure {
    #[serde(default = "default_surface_protocol_version")]
    pub protocol_version: String,
    pub instance_id: String,
    pub request_id: String,
    pub generation: u64,
    pub error: SurfaceExecutionError,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_successful_result_revision: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceLifecycleState {
    Created,
    Mounted,
    Active,
    Inactive,
    Suspended,
    Disposed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceLifecycleEvent {
    #[serde(default = "default_surface_protocol_version")]
    pub protocol_version: String,
    pub instance_id: String,
    pub attachment_id: String,
    pub state: SurfaceLifecycleState,
    pub revision: u64,
}
