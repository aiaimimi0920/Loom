use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveDeviceRole {
    Source,
    Viewer,
    Controller,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveRegionAnchor {
    Window,
    Control,
    Screen,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveSourceKind {
    Window,
    Display,
    Region,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveMediaTransport {
    WebsocketBinary,
    SharedMemory,
    CloudRelay,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveCodec {
    RawBgra,
    H264,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveColorSpace {
    Srgb,
    Hdr10,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveVisibilityState {
    Visible,
    Unfocused,
    LogicalMinimized,
    LogicalHidden,
    Restoring,
    CaptureRecovering,
    CaptureFailed,
    Closed,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveCaptureStrategy {
    PersistentWindowWgc,
    PersistentDisplayWgc,
    Adapter,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveRenderPreservationStrategy {
    Visible,
    VisibleOffscreen,
    HiddenWorkspace,
    ApplicationAdapter,
    Unsupported,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveInteractionCapability {
    PointerMove,
    PointerButton,
    DoubleClick,
    Drag,
    Wheel,
    Keyboard,
    Text,
    Focus,
    Cancel,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveObservationCapability {
    UiaTree,
    Invoke,
    RangeValue,
    Value,
    Toggle,
    Scroll,
    Adapter,
    Vision,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveWindowIdentity {
    pub window_id: String,
    pub process_id: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_started_at_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveFrameStreamDescriptor {
    pub stream_id: String,
    pub transport: LiveMediaTransport,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    pub codec: LiveCodec,
    pub color_space: LiveColorSpace,
    pub width: u32,
    pub height: u32,
    pub target_fps: u16,
    pub max_buffered_frames: u8,
    pub keyframe_interval: u16,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveTriggerBinding {
    pub binding_id: String,
    pub observation_id: String,
    pub condition_revision: u64,
    pub authorized_by: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveScreenshotSession {
    pub protocol_version: String,
    pub session_id: String,
    pub source_device_id: String,
    pub source_hook_id: String,
    pub source_kind: LiveSourceKind,
    pub source_window_identity: LiveWindowIdentity,
    pub source_region: LiveRect,
    pub region_anchor: LiveRegionAnchor,
    pub frame_stream: LiveFrameStreamDescriptor,
    pub interaction_capabilities: Vec<LiveInteractionCapability>,
    pub observation_capabilities: Vec<LiveObservationCapability>,
    pub trigger_bindings: Vec<LiveTriggerBinding>,
    pub viewer_devices: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub controller_device: Option<String>,
    pub visibility_state: LiveVisibilityState,
    pub capture_strategy: LiveCaptureStrategy,
    pub render_preservation_strategy: LiveRenderPreservationStrategy,
    pub revision: u64,
    pub created_at_ms: u64,
    pub last_seen_at_ms: u64,
}
