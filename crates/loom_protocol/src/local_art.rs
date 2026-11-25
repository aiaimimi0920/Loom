//! Otter 单侧执行的独立授权合同；不是设备登录、包签名或旧 Surface resource lease。
use serde::{Deserialize, Serialize};
#[path = "local_art_live_control.rs"]
pub mod live_control;
#[path = "local_art_live_trigger.rs"]
pub mod live_trigger;
#[path = "local_art_live_viewer.rs"]
pub mod live_viewer;
#[path = "local_art_package.rs"]
pub mod package;
#[path = "local_art_resume.rs"]
pub mod resume;
#[path = "local_art_service.rs"]
pub mod service;

pub const PROTOCOL: &str = "loom.art.local.grant.v1";
pub const SDK: &str = "otter.art.v1";
pub const DOMAIN: &[u8] = b"loom.art.local.grant.v1\0";
pub const MAX_PAYLOAD: usize = 16 * 1024;
pub const MAX_ENVELOPE: usize = 24 * 1024;
pub const MAX_VALIDITY_MS: u64 = 15 * 60 * 1000;
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Binding {
    pub origin: String,
    pub request_id: String,
    pub device_id: String,
    pub workspace_id: String,
    pub instance_id: String,
    pub generation: u64,
    pub art_id: String,
    pub art_version: String,
    pub package_digest: String,
    pub sdk_version: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    LocalState,
    Preview,
    LoomService,
    WorkspaceResult,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Source {
    pub resource_id: String,
    pub digest: String,
    pub size: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Grant {
    pub protocol_version: String,
    pub grant_id: String,
    pub binding: Binding,
    pub execution_owner: String,
    pub result_owner: String,
    pub permissions: Vec<Permission>,
    /// 仅允许显式列出的服务 ID；远程调用仍消费其自身授权与敏感确认。
    pub services: Vec<String>,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    /// 0 表示不允许离线；非零值不能超过授权期限。缓存不自动延长期限。
    pub offline_until_ms: u64,
    pub source: Source,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Envelope {
    pub algorithm: String,
    pub key_id: String,
    /// 签名覆盖 domain || base64 解码后的原始 JSON 字节，不对解析后 JSON 重序列化。
    pub payload: String,
    pub signature: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Version,
    Identity,
    Permission,
    Time,
    Source,
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
}
fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl Grant {
    /// 只验证合同，不赋予信任。签发调用者须已核精确安装包、设备、启用及明确授权。
    pub fn validate(&self) -> Result<(), Error> {
        let b = &self.binding;
        if self.protocol_version != PROTOCOL
            || b.sdk_version != SDK
            || self.execution_owner != "otter"
            || self.result_owner != "otter_workspace"
        {
            return Err(Error::Version);
        }
        if !identifier(&self.grant_id)
            || [&b.request_id, &b.device_id, &b.workspace_id, &b.instance_id]
                .iter()
                .any(|v| !identifier(v))
            || !b.art_id.split_once('/').is_some_and(|(publisher, art)| {
                crate::is_safe_publisher_id(publisher) && crate::is_safe_package_id(art)
            })
            || b.generation == 0
            || b.generation > MAX_SAFE_INTEGER
            || b.origin.is_empty()
            || b.origin.len() > 2048
            || !b.origin.is_ascii()
            || b.origin
                .bytes()
                .any(|c| c.is_ascii_control() || c.is_ascii_whitespace())
            || b.art_version.is_empty()
            || b.art_version.len() > 128
            || !b
                .art_version
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b".+-".contains(&c))
            || !digest(&b.package_digest)
        {
            return Err(Error::Identity);
        }
        if self.permissions.is_empty()
            || self.permissions.len() > 4
            || self
                .permissions
                .iter()
                .enumerate()
                .any(|(i, p)| self.permissions[..i].contains(p))
            || self.services.len() > 32
            || self
                .services
                .iter()
                .enumerate()
                .any(|(i, s)| !identifier(s) || self.services[..i].contains(s))
            || (self.services.is_empty() == self.permissions.contains(&Permission::LoomService))
        {
            return Err(Error::Permission);
        }
        if self.issued_at_ms == 0
            || self.expires_at_ms > MAX_SAFE_INTEGER
            || self.expires_at_ms <= self.issued_at_ms
            || self.expires_at_ms - self.issued_at_ms > MAX_VALIDITY_MS
            || (self.offline_until_ms != 0
                && (self.offline_until_ms <= self.issued_at_ms
                    || self.offline_until_ms > self.expires_at_ms))
        {
            return Err(Error::Time);
        }
        if !digest(&self.source.digest)
            || self.source.resource_id != format!("sha256:{}", self.source.digest)
            || self.source.size == 0
            || self.source.size > 1024 * 1024
        {
            return Err(Error::Source);
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "local_art_tests.rs"]
mod tests;
