use serde::{Deserialize, Serialize};

use super::{
    LiveFrameMetadata, LiveInputEvent, LiveObservation, LiveScreenshotSession, LiveTriggerAudit,
    LiveTriggerCondition, LiveVisibilityState,
};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveControlEnvelope {
    pub protocol_version: String,
    pub session_id: String,
    pub epoch: u64,
    pub sequence: u64,
    #[serde(flatten)]
    pub message: LiveControlMessage,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "messageType", content = "payload", rename_all = "snake_case")]
pub enum LiveControlMessage {
    SessionStart(LiveSessionStart),
    SessionAck(LiveSessionAck),
    SessionState(LiveSessionState),
    FrameNotice(LiveFrameNotice),
    InputEvent(LiveInputEvent),
    Observation(LiveObservation),
    TriggerCondition(LiveTriggerCondition),
    TriggerEvent(LiveTriggerAudit),
    ControlTransfer(LiveControlTransfer),
    ResumeRequest(LiveResumeRequest),
    SessionEnd(LiveSessionEnd),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveSessionStart {
    pub session: LiveScreenshotSession,
    pub requested_by_device_id: String,
    pub request_nonce: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveSessionAck {
    pub accepted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub responder_device_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveSessionState {
    pub revision: u64,
    pub visibility: LiveVisibilityState,
    pub viewers: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub controller_device_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveFrameNotice {
    pub metadata: LiveFrameMetadata,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveControlTransfer {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_controller_device_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub controller_device_id: Option<String>,
    pub authority_revision: u64,
    pub expires_at_ms: u64,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveResumeRequest {
    pub last_control_sequence: u64,
    pub last_frame_id: u64,
    pub last_input_sequence: u64,
    pub requester_device_id: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveSessionEndReason {
    Closed,
    Revoked,
    SourceClosed,
    PermissionDenied,
    TimedOut,
    Error,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveSessionEnd {
    pub reason: LiveSessionEndReason,
    pub ended_by_device_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}
