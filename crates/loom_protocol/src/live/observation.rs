use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveObservationSource {
    UiAutomation,
    AppAdapter,
    Vision,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveObservationConfidence {
    Exact,
    High,
    Medium,
    Low,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveObservationState {
    Unknown,
    Detected,
    Observing,
    Stable,
    Triggered,
    Stale,
    Error,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveElementLocator {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub automation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub control_type: String,
    pub ancestor_path: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_id: Option<Vec<i32>>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveObservation {
    pub observation_id: String,
    pub sequence: u64,
    pub state: LiveObservationState,
    pub source: LiveObservationSource,
    pub confidence: LiveObservationConfidence,
    pub observed_at_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stable_since_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locator: Option<LiveElementLocator>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveConditionOperator {
    Equals,
    NotEquals,
    GreaterThan,
    GreaterOrEqual,
    LessThan,
    LessOrEqual,
    Contains,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveTriggerCondition {
    pub condition_id: String,
    pub revision: u64,
    pub observation_id: String,
    pub operator: LiveConditionOperator,
    pub operand: Value,
    pub stable_for_ms: u32,
    pub rising_edge: bool,
    pub rearm: bool,
    pub minimum_confidence: LiveObservationConfidence,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveTriggerOutcome {
    Fired,
    Skipped,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveTriggerAudit {
    pub trigger_id: String,
    pub binding_id: String,
    pub condition_revision: u64,
    pub observation_id: String,
    pub observation_sequence: u64,
    pub source_device_id: String,
    pub observation_source: LiveObservationSource,
    pub idempotency_key: String,
    pub outcome: LiveTriggerOutcome,
    pub evaluated_at_ms: u64,
    pub authorized_by: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

pub type LiveTriggerEvent = LiveTriggerAudit;
