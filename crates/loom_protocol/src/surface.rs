//! Stable contracts for distributed Art surfaces.
//!
//! Surface v1 deliberately describes UI state, events, resources, and formal
//! results without exposing Hook's internal frontend framework. Art packages
//! may author TypeScript, JavaScript, or declarative scenes, but hosts exchange
//! only these language-neutral envelopes.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

pub const SURFACE_PROTOCOL_VERSION: &str = "loom.surface.v1";
pub const SURFACE_API_VERSION: &str = "1.0";
pub const SURFACE_EVENT_SNAPSHOT: &str = "loom.surface.snapshot";
pub const SURFACE_EVENT_PATCH: &str = "loom.surface.patch";
pub const SURFACE_EVENT_GENERATION: &str = "loom.surface.generation";
pub const SURFACE_EVENT_ACTION_ACK: &str = "loom.surface.action.ack";
pub const SURFACE_EVENT_CONFIRMATION_REQUEST: &str = "loom.surface.confirmation.request";
pub const SURFACE_EVENT_ACTION_PROGRESS: &str = "loom.surface.action.progress";
pub const SURFACE_EVENT_PREVIEW: &str = "loom.surface.preview";
pub const SURFACE_EVENT_RESULT: &str = "loom.surface.result";
pub const SURFACE_EVENT_FAILURE: &str = "loom.surface.failure";
pub const SURFACE_EVENT_LIFECYCLE: &str = "loom.surface.lifecycle";
pub const SURFACE_EVENT_DISPOSE: &str = "loom.surface.dispose";

pub const SURFACE_EVENT_METHODS: &[&str] = &[
    SURFACE_EVENT_SNAPSHOT,
    SURFACE_EVENT_PATCH,
    SURFACE_EVENT_GENERATION,
    SURFACE_EVENT_ACTION_ACK,
    SURFACE_EVENT_CONFIRMATION_REQUEST,
    SURFACE_EVENT_ACTION_PROGRESS,
    SURFACE_EVENT_PREVIEW,
    SURFACE_EVENT_RESULT,
    SURFACE_EVENT_FAILURE,
    SURFACE_EVENT_LIFECYCLE,
    SURFACE_EVENT_DISPOSE,
];
pub const DECLARATIVE_SURFACE_NODE_TYPES: &[&str] = &[
    "view", "row", "column", "stack", "scroll", "text", "image", "icon", "button", "input",
    "textarea", "number", "slider", "switch", "select", "progress", "divider", "spacer",
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceRuntimeKind {
    Declarative,
    Javascript,
    Shader,
    LoomRemote,
}

impl Default for SurfaceRuntimeKind {
    fn default() -> Self {
        Self::Declarative
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceThemeMode {
    Host,
    Custom,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceInstanceMode {
    #[default]
    Independent,
    Shared,
}

impl Default for SurfaceThemeMode {
    fn default() -> Self {
        Self::Host
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceSizeClass {
    Compact,
    Medium,
    Expanded,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceViewDefinition {
    pub id: String,
    pub label: String,
    pub full_size: SurfaceSize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceVariant {
    pub runtime: SurfaceRuntimeKind,
    pub entry: String,
    #[serde(default)]
    pub required_capabilities: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceStateMigration {
    pub from: u32,
    pub to: u32,
    pub entry: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfacePackageManifest {
    #[serde(default = "default_surface_protocol_version")]
    pub protocol_version: String,
    #[serde(default = "default_surface_api_version")]
    pub api_version: String,
    #[serde(default)]
    pub variants: Vec<SurfaceVariant>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_scene: Option<String>,
    #[serde(default)]
    pub required_nodes: Vec<String>,
    #[serde(default)]
    pub required_capabilities: Vec<String>,
    #[serde(default)]
    pub actions: Vec<SurfaceActionDefinition>,
    #[serde(default)]
    pub instance_mode: SurfaceInstanceMode,
    #[serde(default = "default_surface_state_schema_version")]
    pub state_schema_version: u32,
    #[serde(default)]
    pub migrations: Vec<SurfaceStateMigration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_size: Option<SurfaceSize>,
    #[serde(default)]
    pub views: Vec<SurfaceViewDefinition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_view_id: Option<String>,
    #[serde(default)]
    pub theme_mode: SurfaceThemeMode,
}

const fn default_surface_state_schema_version() -> u32 {
    1
}

fn default_surface_protocol_version() -> String {
    SURFACE_PROTOCOL_VERSION.to_owned()
}

fn default_surface_api_version() -> String {
    SURFACE_API_VERSION.to_owned()
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceInputCapabilities {
    #[serde(default)]
    pub pointer: bool,
    #[serde(default)]
    pub hover: bool,
    #[serde(default)]
    pub touch: bool,
    #[serde(default)]
    pub keyboard: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceHostCapabilities {
    pub api_version: String,
    #[serde(default)]
    pub runtimes: Vec<SurfaceRuntimeKind>,
    #[serde(default)]
    pub nodes: Vec<String>,
    #[serde(default)]
    pub transports: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub input: SurfaceInputCapabilities,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceHandshake {
    #[serde(default = "default_surface_protocol_version")]
    pub protocol_version: String,
    pub client_id: String,
    pub client_version: String,
    pub platform: String,
    pub capabilities: SurfaceHostCapabilities,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceInstancePersistence {
    Temporary,
    Persistent,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceInstanceDescriptor {
    pub instance_id: String,
    pub art_id: String,
    pub art_version: String,
    pub package_digest: String,
    #[serde(default)]
    pub instance_mode: SurfaceInstanceMode,
    #[serde(default)]
    pub state_schema_version: u32,
    pub persistence: SurfaceInstancePersistence,
    #[serde(default)]
    pub generation: u64,
    #[serde(default)]
    pub surface_revision: u64,
    #[serde(default)]
    pub preview_revision: u64,
    #[serde(default)]
    pub result_revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceAttachmentDescriptor {
    pub attachment_id: String,
    pub instance_id: String,
    pub hook_node_id: String,
    pub device_id: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceNode {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: String,
    #[serde(default)]
    pub props: Value,
    #[serde(default)]
    pub layout: Value,
    #[serde(default)]
    pub style: Value,
    #[serde(default)]
    pub accessibility: Value,
    #[serde(default)]
    pub events: BTreeMap<String, String>,
    #[serde(default)]
    pub children: Vec<SurfaceNode>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceResourceKind {
    Image,
    Audio,
    Video,
    File,
    Binary,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceResourceDescriptor {
    pub resource_id: String,
    pub kind: SurfaceResourceKind,
    pub mime: String,
    pub size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceResourceTransportKind {
    SharedMemory,
    LoomResource,
    Stream,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceResourceTransport {
    pub kind: SurfaceResourceTransportKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceResourceLease {
    pub lease_id: String,
    pub resource: SurfaceResourceDescriptor,
    pub transport: SurfaceResourceTransport,
    pub expires_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceStreamDescriptor {
    pub stream_id: String,
    pub item_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime: Option<String>,
    #[serde(default)]
    pub sequence: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SurfacePortValue {
    Value { value: Value },
    Resource { resource: SurfaceResourceDescriptor },
    Stream { stream: SurfaceStreamDescriptor },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfacePortKind {
    Boolean,
    Integer,
    Number,
    String,
    Enum,
    Object,
    List,
    Table,
    Image,
    Audio,
    Video,
    File,
    Binary,
    Stream,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfacePortDefinition {
    pub id: String,
    pub label: String,
    #[serde(rename = "type")]
    pub port_type: SurfacePortKind,
    #[serde(default)]
    pub schema: Value,
    #[serde(default)]
    pub primary: bool,
    #[serde(default)]
    pub required: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceSnapshot {
    #[serde(default = "default_surface_protocol_version")]
    pub protocol_version: String,
    pub instance_id: String,
    pub attachment_id: String,
    pub art_id: String,
    pub art_version: String,
    pub revision: u64,
    #[serde(default)]
    pub runtime: SurfaceRuntimeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_resource_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view_id: Option<String>,
    pub scene: SurfaceNode,
    #[serde(default)]
    pub authoritative_state: Value,
    #[serde(default)]
    pub resources: Vec<SurfaceResourceDescriptor>,
    #[serde(default)]
    pub resource_leases: Vec<SurfaceResourceLease>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum SurfacePatchOperation {
    Set {
        node_id: String,
        path: String,
        value: Value,
    },
    Remove {
        node_id: String,
        path: String,
    },
    InsertNode {
        parent_id: String,
        index: usize,
        node: SurfaceNode,
    },
    RemoveNode {
        node_id: String,
    },
    MoveNode {
        node_id: String,
        parent_id: String,
        index: usize,
    },
    ReplaceNode {
        node_id: String,
        node: SurfaceNode,
    },
    SetVisibility {
        node_id: String,
        visible: bool,
    },
    SetBinding {
        node_id: String,
        path: String,
        binding: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfacePatch {
    #[serde(default = "default_surface_protocol_version")]
    pub protocol_version: String,
    pub instance_id: String,
    pub attachment_id: String,
    pub base_revision: u64,
    pub revision: u64,
    #[serde(default)]
    pub operations: Vec<SurfacePatchOperation>,
    #[serde(default)]
    pub state_patch: Value,
    #[serde(default)]
    pub resources: Vec<SurfaceResourceDescriptor>,
    #[serde(default)]
    pub resource_leases: Vec<SurfaceResourceLease>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceEventClass {
    Discrete,
    Continuous,
    Commit,
    Local,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceEvent {
    #[serde(default = "default_surface_protocol_version")]
    pub protocol_version: String,
    pub instance_id: String,
    pub attachment_id: String,
    pub event_id: String,
    pub node_id: String,
    pub event: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    pub class: SurfaceEventClass,
    pub generation: u64,
    pub base_revision: u64,
    #[serde(default)]
    pub payload: Value,
}

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

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SurfaceValidationError {
    #[error("unsupported Surface protocol `{0}`")]
    UnsupportedProtocol(String),
    #[error("Surface identifier is not safe: {0}")]
    UnsafeIdentifier(String),
    #[error("Surface node id is duplicated: {0}")]
    DuplicateNodeId(String),
    #[error("Surface node type is empty for node {0}")]
    EmptyNodeType(String),
    #[error("Surface patch revision {revision} does not advance base revision {base_revision}")]
    InvalidPatchRevision { base_revision: u64, revision: u64 },
    #[error("Surface resource id must use sha256 content addressing: {0}")]
    InvalidResourceId(String),
    #[error("Surface runtime entry is invalid: {0}")]
    InvalidRuntimeEntry(String),
    #[error("Surface confirmation is invalid: {0}")]
    InvalidConfirmation(String),
}

pub fn is_safe_surface_identifier(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty()
        && value.len() <= 160
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/')
        })
}

pub fn validate_surface_protocol(protocol_version: &str) -> Result<(), SurfaceValidationError> {
    if protocol_version == SURFACE_PROTOCOL_VERSION {
        Ok(())
    } else {
        Err(SurfaceValidationError::UnsupportedProtocol(
            protocol_version.to_owned(),
        ))
    }
}

pub fn validate_surface_node_tree(root: &SurfaceNode) -> Result<(), SurfaceValidationError> {
    fn visit(
        node: &SurfaceNode,
        seen: &mut BTreeSet<String>,
    ) -> Result<(), SurfaceValidationError> {
        if !is_safe_surface_identifier(&node.id) {
            return Err(SurfaceValidationError::UnsafeIdentifier(node.id.clone()));
        }
        if node.node_type.trim().is_empty() {
            return Err(SurfaceValidationError::EmptyNodeType(node.id.clone()));
        }
        if !seen.insert(node.id.clone()) {
            return Err(SurfaceValidationError::DuplicateNodeId(node.id.clone()));
        }
        for action in node.events.values() {
            if !is_safe_surface_identifier(action) {
                return Err(SurfaceValidationError::UnsafeIdentifier(action.clone()));
            }
        }
        for child in &node.children {
            visit(child, seen)?;
        }
        Ok(())
    }

    visit(root, &mut BTreeSet::new())
}

pub fn validate_surface_snapshot(snapshot: &SurfaceSnapshot) -> Result<(), SurfaceValidationError> {
    validate_surface_protocol(&snapshot.protocol_version)?;
    for id in [
        &snapshot.instance_id,
        &snapshot.attachment_id,
        &snapshot.art_id,
    ] {
        if !is_safe_surface_identifier(id) {
            return Err(SurfaceValidationError::UnsafeIdentifier((*id).clone()));
        }
    }
    if snapshot
        .view_id
        .as_deref()
        .is_some_and(|view_id| !is_safe_surface_identifier(view_id))
    {
        return Err(SurfaceValidationError::UnsafeIdentifier(
            snapshot.view_id.clone().unwrap_or_default(),
        ));
    }
    validate_surface_node_tree(&snapshot.scene)?;
    for resource in &snapshot.resources {
        validate_surface_resource(resource)?;
    }
    for lease in &snapshot.resource_leases {
        validate_surface_resource_lease(lease)?;
    }
    if snapshot.runtime == SurfaceRuntimeKind::Javascript {
        let entry_resource_id = snapshot.entry_resource_id.as_deref().ok_or_else(|| {
            SurfaceValidationError::InvalidRuntimeEntry(
                "JavaScript Surface has no entry resource".to_owned(),
            )
        })?;
        if !snapshot
            .resource_leases
            .iter()
            .any(|lease| lease.resource.resource_id == entry_resource_id)
        {
            return Err(SurfaceValidationError::InvalidRuntimeEntry(
                "JavaScript entry resource is not leased by the snapshot".to_owned(),
            ));
        }
    }
    Ok(())
}

pub fn validate_surface_patch(patch: &SurfacePatch) -> Result<(), SurfaceValidationError> {
    validate_surface_protocol(&patch.protocol_version)?;
    if patch.revision <= patch.base_revision {
        return Err(SurfaceValidationError::InvalidPatchRevision {
            base_revision: patch.base_revision,
            revision: patch.revision,
        });
    }
    for id in [&patch.instance_id, &patch.attachment_id] {
        if !is_safe_surface_identifier(id) {
            return Err(SurfaceValidationError::UnsafeIdentifier((*id).clone()));
        }
    }
    for resource in &patch.resources {
        validate_surface_resource(resource)?;
    }
    for lease in &patch.resource_leases {
        validate_surface_resource_lease(lease)?;
    }
    Ok(())
}

pub fn validate_surface_event(event: &SurfaceEvent) -> Result<(), SurfaceValidationError> {
    validate_surface_protocol(&event.protocol_version)?;
    for id in [
        &event.instance_id,
        &event.attachment_id,
        &event.event_id,
        &event.node_id,
        &event.event,
    ] {
        if !is_safe_surface_identifier(id) {
            return Err(SurfaceValidationError::UnsafeIdentifier((*id).clone()));
        }
    }
    if let Some(action) = &event.action {
        if !is_safe_surface_identifier(action) {
            return Err(SurfaceValidationError::UnsafeIdentifier(action.clone()));
        }
    }
    Ok(())
}

pub fn validate_surface_action_invocation(
    invocation: &SurfaceActionInvocation,
) -> Result<(), SurfaceValidationError> {
    validate_surface_protocol(&invocation.protocol_version)?;
    for id in [
        &invocation.instance_id,
        &invocation.attachment_id,
        &invocation.request_id,
        &invocation.event_id,
        &invocation.action_id,
    ] {
        if !is_safe_surface_identifier(id) {
            return Err(SurfaceValidationError::UnsafeIdentifier((*id).clone()));
        }
    }
    Ok(())
}

pub fn validate_surface_confirmation_request(
    request: &SurfaceConfirmationRequest,
) -> Result<(), SurfaceValidationError> {
    validate_surface_protocol(&request.protocol_version)?;
    for id in [
        &request.confirmation_id,
        &request.instance_id,
        &request.attachment_id,
        &request.device_id,
        &request.hook_node_id,
        &request.event_id,
        &request.request_id,
        &request.action_id,
    ] {
        if !is_safe_surface_identifier(id) {
            return Err(SurfaceValidationError::UnsafeIdentifier((*id).clone()));
        }
    }
    if request.expires_at_ms == 0 {
        return Err(SurfaceValidationError::InvalidConfirmation(
            "confirmation expiry must be positive".to_owned(),
        ));
    }
    Ok(())
}

pub fn validate_surface_confirmation_decision(
    decision: &SurfaceConfirmationDecision,
) -> Result<(), SurfaceValidationError> {
    validate_surface_protocol(&decision.protocol_version)?;
    for id in [
        &decision.confirmation_id,
        &decision.instance_id,
        &decision.attachment_id,
        &decision.device_id,
    ] {
        if !is_safe_surface_identifier(id) {
            return Err(SurfaceValidationError::UnsafeIdentifier((*id).clone()));
        }
    }
    Ok(())
}

pub fn validate_surface_action_cancel_request(
    request: &SurfaceActionCancelRequest,
) -> Result<(), SurfaceValidationError> {
    validate_surface_protocol(&request.protocol_version)?;
    for id in [
        &request.instance_id,
        &request.request_id,
        &request.device_id,
    ] {
        if !is_safe_surface_identifier(id) {
            return Err(SurfaceValidationError::UnsafeIdentifier((*id).clone()));
        }
    }
    Ok(())
}

pub fn validate_surface_resource(
    resource: &SurfaceResourceDescriptor,
) -> Result<(), SurfaceValidationError> {
    let digest = resource.resource_id.strip_prefix("sha256:");
    if !digest.is_some_and(|digest| {
        digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    }) {
        return Err(SurfaceValidationError::InvalidResourceId(
            resource.resource_id.clone(),
        ));
    }
    Ok(())
}

pub fn validate_surface_resource_lease(
    lease: &SurfaceResourceLease,
) -> Result<(), SurfaceValidationError> {
    if !is_safe_surface_identifier(&lease.lease_id) {
        return Err(SurfaceValidationError::UnsafeIdentifier(
            lease.lease_id.clone(),
        ));
    }
    validate_surface_resource(&lease.resource)?;
    if let Some(stream_id) = lease.transport.stream_id.as_deref() {
        if !is_safe_surface_identifier(stream_id) {
            return Err(SurfaceValidationError::UnsafeIdentifier(
                stream_id.to_owned(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str) -> SurfaceNode {
        SurfaceNode {
            id: id.to_owned(),
            node_type: "text".to_owned(),
            ..SurfaceNode::default()
        }
    }

    #[test]
    fn snapshot_round_trip_preserves_stable_wire_names() {
        let snapshot = SurfaceSnapshot {
            protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
            instance_id: "instance:stock-01".to_owned(),
            attachment_id: "attachment:hook-01".to_owned(),
            art_id: "NA00000000001".to_owned(),
            art_version: "1.2.3".to_owned(),
            revision: 7,
            runtime: SurfaceRuntimeKind::Declarative,
            entry_resource_id: None,
            view_id: Some("full".to_owned()),
            scene: node("root"),
            authoritative_state: serde_json::json!({"price": 221.18}),
            resources: Vec::new(),
            resource_leases: Vec::new(),
        };

        validate_surface_snapshot(&snapshot).expect("valid snapshot");
        let value = serde_json::to_value(&snapshot).expect("serialize snapshot");
        assert_eq!(value["protocolVersion"], SURFACE_PROTOCOL_VERSION);
        assert_eq!(value["instanceId"], "instance:stock-01");
        assert_eq!(value["viewId"], "full");
        assert_eq!(value["scene"]["type"], "text");
        assert_eq!(
            serde_json::from_value::<SurfaceSnapshot>(value).expect("deserialize snapshot"),
            snapshot
        );
    }

    #[test]
    fn duplicate_scene_ids_are_rejected() {
        let mut root = node("root");
        root.children = vec![node("price"), node("price")];
        assert_eq!(
            validate_surface_node_tree(&root),
            Err(SurfaceValidationError::DuplicateNodeId("price".to_owned()))
        );
    }

    #[test]
    fn patch_must_advance_revision() {
        let patch = SurfacePatch {
            protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
            instance_id: "instance:one".to_owned(),
            attachment_id: "attachment:one".to_owned(),
            base_revision: 4,
            revision: 4,
            operations: Vec::new(),
            state_patch: Value::Null,
            resources: Vec::new(),
            resource_leases: Vec::new(),
        };
        assert_eq!(
            validate_surface_patch(&patch),
            Err(SurfaceValidationError::InvalidPatchRevision {
                base_revision: 4,
                revision: 4,
            })
        );
    }

    #[test]
    fn surface_action_contract_round_trips_without_runtime_owned_revisions() {
        let invocation = SurfaceActionInvocation {
            protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
            instance_id: "instance:stock".to_owned(),
            attachment_id: "attachment:hook".to_owned(),
            request_id: "request:refresh".to_owned(),
            event_id: "event:refresh".to_owned(),
            action_id: "refresh_price".to_owned(),
            event_class: SurfaceEventClass::Discrete,
            generation: 4,
            base_revision: 9,
            payload: serde_json::json!({"symbol": "MSFT"}),
            authoritative_state: serde_json::json!({"price": 100}),
        };
        validate_surface_action_invocation(&invocation).expect("valid invocation");
        let encoded = serde_json::to_value(&invocation).expect("serialize invocation");
        assert_eq!(encoded["actionId"], "refresh_price");
        assert_eq!(encoded["eventClass"], "discrete");

        let response: SurfaceActionResponse = serde_json::from_value(serde_json::json!({
            "protocolVersion": SURFACE_PROTOCOL_VERSION,
            "patches": [{
                "operations": [{
                    "op": "set",
                    "nodeId": "price",
                    "path": "/props/text",
                    "value": "101"
                }],
                "statePatch": {"price": 101}
            }],
            "result": {
                "outputs": {
                    "price": {"kind": "value", "value": 101}
                },
                "statePatch": {"price": 101}
            }
        }))
        .expect("deserialize action response");
        assert_eq!(response.patches.len(), 1);
        assert!(response.result.is_some());
        let wire = serde_json::to_value(response).expect("serialize action response");
        assert!(wire.get("resultRevision").is_none());
        assert!(wire["patches"][0].get("revision").is_none());
    }

    #[test]
    fn confirmation_contract_binds_host_device_and_attachment_identity() {
        let request = SurfaceConfirmationRequest {
            protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
            confirmation_id: "confirmation:one".to_owned(),
            instance_id: "instance:one".to_owned(),
            attachment_id: "attachment:one".to_owned(),
            device_id: "device-000-local".to_owned(),
            hook_node_id: "hook-node:one".to_owned(),
            event_id: "event:one".to_owned(),
            request_id: "request:one".to_owned(),
            action_id: "submit_order".to_owned(),
            risk: SurfaceActionRisk::High,
            expires_at_ms: 42,
            payload: serde_json::json!({"quantity": 1}),
        };
        validate_surface_confirmation_request(&request).expect("valid confirmation request");
        let wire = serde_json::to_value(&request).expect("serialize confirmation request");
        assert_eq!(wire["risk"], "high");
        assert_eq!(wire["deviceId"], "device-000-local");

        let decision = SurfaceConfirmationDecision {
            protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
            confirmation_id: request.confirmation_id,
            instance_id: request.instance_id,
            attachment_id: request.attachment_id,
            device_id: request.device_id,
            approved: true,
        };
        validate_surface_confirmation_decision(&decision).expect("valid confirmation decision");
        assert_eq!(
            serde_json::to_value(decision).expect("serialize confirmation decision")["approved"],
            true
        );
    }

    #[test]
    fn cancellation_contract_binds_request_to_its_device_and_instance() {
        let request = SurfaceActionCancelRequest {
            protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
            instance_id: "instance:one".to_owned(),
            request_id: "request:one".to_owned(),
            device_id: "device-000-local".to_owned(),
        };
        validate_surface_action_cancel_request(&request).expect("valid cancel request");
        let wire = serde_json::to_value(&request).expect("serialize cancel request");
        assert_eq!(wire["instanceId"], "instance:one");
        assert_eq!(wire["requestId"], "request:one");
        assert_eq!(wire["deviceId"], "device-000-local");
        assert!(wire.get("eventId").is_none());

        let mut invalid = request;
        invalid.device_id = "device with spaces".to_owned();
        assert!(validate_surface_action_cancel_request(&invalid).is_err());
    }

    #[test]
    fn patch_operation_uses_camel_case_wire_fields() {
        let operation = SurfacePatchOperation::Set {
            node_id: "price".to_owned(),
            path: "/props/text".to_owned(),
            value: serde_json::json!("101"),
        };
        let value = serde_json::to_value(&operation).expect("serialize operation");
        assert_eq!(value["op"], "set");
        assert_eq!(value["nodeId"], "price");
        assert!(value.get("node_id").is_none());
        assert_eq!(
            serde_json::from_value::<SurfacePatchOperation>(value).expect("deserialize operation"),
            operation
        );
    }

    #[test]
    fn result_commit_is_atomic_and_preview_is_separate() {
        let resource = SurfaceResourceDescriptor {
            resource_id: "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_owned(),
            kind: SurfaceResourceKind::Image,
            mime: "image/webp".to_owned(),
            size: 128,
            width: Some(8),
            height: Some(8),
        };
        let preview = SurfacePreviewCommit {
            protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
            instance_id: "instance:one".to_owned(),
            request_id: "request:one".to_owned(),
            generation: 2,
            preview_revision: 3,
            port_id: "preview".to_owned(),
            value: SurfacePortValue::Resource {
                resource: resource.clone(),
            },
        };
        let result = SurfaceResultCommit {
            protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
            instance_id: "instance:one".to_owned(),
            request_id: "request:one".to_owned(),
            generation: 2,
            result_revision: 4,
            outputs: BTreeMap::from([
                (
                    "output_image".to_owned(),
                    SurfacePortValue::Resource { resource },
                ),
                (
                    "output_size".to_owned(),
                    SurfacePortValue::Value {
                        value: serde_json::json!(128),
                    },
                ),
            ]),
            state_patch: serde_json::json!({"status": "completed"}),
        };

        let preview_json = serde_json::to_value(preview).expect("preview JSON");
        let result_json = serde_json::to_value(result).expect("result JSON");
        assert!(preview_json.get("outputs").is_none());
        assert_eq!(result_json["outputs"]["output_size"]["kind"], "value");
        assert_eq!(result_json["resultRevision"], 4);
    }

    #[test]
    fn content_addressed_resource_requires_sha256() {
        let resource = SurfaceResourceDescriptor {
            resource_id: "file:C:/private/image.png".to_owned(),
            kind: SurfaceResourceKind::Image,
            mime: "image/png".to_owned(),
            size: 1,
            width: None,
            height: None,
        };
        assert!(matches!(
            validate_surface_resource(&resource),
            Err(SurfaceValidationError::InvalidResourceId(_))
        ));
    }

    #[test]
    fn surface_event_requires_safe_wire_identities() {
        let event = SurfaceEvent {
            protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
            instance_id: "instance:one".to_owned(),
            attachment_id: "attachment:one".to_owned(),
            event_id: "event:one".to_owned(),
            node_id: "button".to_owned(),
            event: "click".to_owned(),
            action: Some("refresh".to_owned()),
            class: SurfaceEventClass::Discrete,
            generation: 1,
            base_revision: 2,
            payload: Value::Null,
        };
        validate_surface_event(&event).expect("valid event");
        assert!(matches!(
            validate_surface_event(&SurfaceEvent {
                event_id: "event with spaces".to_owned(),
                ..event
            }),
            Err(SurfaceValidationError::UnsafeIdentifier(_))
        ));
    }
}
