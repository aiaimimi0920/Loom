//! 单次领取回执；Acknowledged 仅是宿主报告，不等价于 canonical 保存或远端呈现。
use super::*;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Receipt {
    Applied { formal_revision: u64 },
    NoResult,
    Failed,
    Cancelled,
}
impl Receipt {
    pub(super) fn validate(&self) -> Result<(), Error> {
        if matches!(self, Self::Applied { formal_revision } if *formal_revision == 0 || *formal_revision > 9_007_199_254_740_991)
        {
            return Err(Error::Wire);
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Idle,
    Ready,
    Claimed,
    Acknowledged,
    Expired,
    Stopped,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    UserStopped,
    GrantExpired,
    ClockRegressed,
    DeliveryExpired,
    ViewerDisconnected,
    ApprovalChanged,
    DeliveryLimit,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Delivery {
    pub delivery_id: String,
    pub callback: String,
    pub condition_revision: u64,
    pub observation: crate::LiveObservation,
    pub source_device_id: String,
    pub expires_at_ms: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Reply {
    pub protocol_version: String,
    pub binding: Binding,
    pub grant_id: String,
    pub scope: Scope,
    pub condition: LiveTriggerCondition,
    pub enabled: bool,
    pub status: Status,
    pub reason: Option<StopReason>,
    pub delivery_id: Option<String>,
    pub receipt: Option<Receipt>,
    /// 只有第一次 Claim 可返回；Inspect/重复 Claim/ACK 从不重新交付 payload。
    pub delivery: Option<Delivery>,
}
