//! Art 专属异步服务合同；执行许可、设备会话和服务自身敏感授权必须分别核验。
use super::{identifier, Binding, Envelope, Grant, Permission};
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[path = "local_art_service_confirmation.rs"]
pub mod confirmation;
#[path = "local_art_service_image_output.rs"]
pub mod image_output;
#[path = "local_art_service_images.rs"]
pub mod images;

pub const PROTOCOL: &str = "loom.art.local.service.v1";
pub const MAX_REQUEST: usize = 96 * 1024;
pub const MAX_INPUT: usize = 64 * 1024;
pub const MAX_OUTPUT: usize = 64 * 1024;
pub const MAX_PROGRESS: usize = 4 * 1024;
pub const MAX_TIMEOUT_MS: u64 = 60_000;

/// callId 由宿主生成，不能用 prepare requestId 操作其他任务。完整 binding 不由响应补造。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Owner {
    pub binding: Binding,
    pub grant_id: String,
    pub call_id: String,
    pub service: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Submit {
    pub protocol_version: String,
    pub owner: Owner,
    pub grant: Envelope,
    pub input: Value,
    pub timeout_ms: u64,
}

/// poll/cancel 使用同一所有权与签名许可；单独知道 callId 不授予读结果或取消权限。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Control {
    pub protocol_version: String,
    pub owner: Owner,
    pub grant: Envelope,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Accepted,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Reply {
    pub protocol_version: String,
    pub owner: Owner,
    /// 单任务单调序号，重复 poll 可返回相同序号，不能复活已完成任务。
    pub sequence: u64,
    pub status: Status,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_output"
    )]
    pub output: Option<Value>,
    pub error: Option<String>,
    /// 只携带 runtime 实际上报的进度；没有事件时为 null，不能捏造百分比。
    pub progress: Option<Value>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Wire,
    Identity,
    Permission,
    Time,
    Limit,
    State,
}

fn bounded(value: &Value, maximum: usize) -> Result<(), Error> {
    let bytes = serde_json::to_vec(value).map_err(|_| Error::Wire)?;
    if bytes.len() > maximum {
        return Err(Error::Limit);
    }
    Ok(())
}
// Missing means no output; an explicit JSON null is a legitimate successful value.
fn present_output<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Value>, D::Error> {
    Value::deserialize(deserializer).map(Some)
}

impl Owner {
    /// grant 必须已由独立 issuer 验签；本方法不做验签，不替代当前审批/设备 epoch/服务授权。
    pub fn validate(&self, grant: &Grant, actor: &str, now: u64) -> Result<(), Error> {
        grant.validate().map_err(|_| Error::Wire)?;
        if self.binding != grant.binding
            || self.binding.device_id != actor
            || self.grant_id != grant.grant_id
            || !identifier(&self.call_id)
            || !identifier(&self.service)
        {
            return Err(Error::Identity);
        }
        if now < grant.issued_at_ms || now >= grant.expires_at_ms {
            return Err(Error::Time);
        }
        if !grant.permissions.contains(&Permission::LoomService)
            || !grant.services.contains(&self.service)
        {
            return Err(Error::Permission);
        }
        Ok(())
    }
}

impl Submit {
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > MAX_REQUEST {
            return Err(Error::Limit);
        }
        serde_json::from_slice(bytes).map_err(|_| Error::Wire)
    }
    pub fn validate(&self, grant: &Grant, actor: &str, now: u64) -> Result<(), Error> {
        if self.protocol_version != PROTOCOL {
            return Err(Error::Wire);
        }
        self.owner.validate(grant, actor, now)?;
        if self.timeout_ms == 0 || self.timeout_ms > MAX_TIMEOUT_MS {
            return Err(Error::Time);
        }
        bounded(&self.input, MAX_INPUT)
    }
}
impl Control {
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > MAX_REQUEST {
            return Err(Error::Limit);
        }
        serde_json::from_slice(bytes).map_err(|_| Error::Wire)
    }
    pub fn validate(&self, grant: &Grant, actor: &str, now: u64) -> Result<(), Error> {
        if self.protocol_version != PROTOCOL {
            return Err(Error::Wire);
        }
        self.owner.validate(grant, actor, now)
    }
}
impl Reply {
    /// 消费方同时保留原请求 owner；这里不允许服务结果变成正式工作区提交。
    pub fn validate(&self, expected: &Owner) -> Result<(), Error> {
        if self.protocol_version != PROTOCOL {
            return Err(Error::Wire);
        }
        if &self.owner != expected {
            return Err(Error::Identity);
        }
        if self.sequence == 0 || self.sequence > 9_007_199_254_740_991 {
            return Err(Error::State);
        }
        match self.status {
            Status::Succeeded if self.output.is_some() && self.error.is_none() => {}
            Status::Failed
                if self.output.is_none() && self.error.as_deref().is_some_and(identifier) => {}
            Status::Accepted | Status::Running | Status::Cancelled
                if self.output.is_none() && self.error.is_none() => {}
            _ => return Err(Error::State),
        }
        if let Some(value) = &self.output {
            bounded(value, MAX_OUTPUT)?;
        }
        if let Some(value) = &self.progress {
            bounded(value, MAX_PROGRESS)?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "local_art_service_tests.rs"]
mod tests;
