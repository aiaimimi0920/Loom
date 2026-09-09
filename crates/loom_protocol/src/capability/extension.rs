use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::CapabilityErrorCode;

mod validation;

pub use validation::*;

pub const EXTENSION_SNAPSHOT_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_EXTENSION_RESOURCE_REFS: usize = 128;
pub const MAX_EXTENSION_INVOCATION_RESOURCE_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ExtensionMessage {
    Snapshot(ContributionSnapshot),
    Invocation(ExtensionInvocation),
    Result(ExtensionResult),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContributionSnapshot {
    pub protocol: String,
    pub api_version: String,
    pub generation: u64,
    pub plugins: Vec<ExtensionPluginBinding>,
    pub contributions: ExtensionContributions,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionPluginBinding {
    pub id: String,
    pub version: String,
    pub package_digest: String,
    pub trust_status: ExtensionTrustStatus,
    pub permission_grant_digest: String,
    pub scope_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionTrustStatus {
    Trusted,
    UnsignedDeveloper,
    Revoked,
    Untrusted,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionContribution {
    pub id: String,
    pub plugin_id: String,
    pub scope_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<i32>,
    #[serde(default)]
    pub payload: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionContributions {
    pub commands: Vec<ExtensionContribution>,
    pub shortcuts: Vec<ExtensionContribution>,
    pub menus: Vec<ExtensionContribution>,
    pub settings: Vec<ExtensionContribution>,
    pub data_types: Vec<ExtensionContribution>,
    pub renderers: Vec<ExtensionContribution>,
    pub unit_overlays: Vec<ExtensionContribution>,
    pub background_tasks: Vec<ExtensionContribution>,
    pub resource_providers: Vec<ExtensionContribution>,
    pub diagnostics: Vec<ExtensionContribution>,
    pub event_subscriptions: Vec<ExtensionContribution>,
}

impl ExtensionContributions {
    pub fn iter(&self) -> impl Iterator<Item = &ExtensionContribution> {
        self.commands
            .iter()
            .chain(&self.shortcuts)
            .chain(&self.menus)
            .chain(&self.settings)
            .chain(&self.data_types)
            .chain(&self.renderers)
            .chain(&self.unit_overlays)
            .chain(&self.background_tasks)
            .chain(&self.resource_providers)
            .chain(&self.diagnostics)
            .chain(&self.event_subscriptions)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionInvocation {
    pub protocol: String,
    pub api_version: String,
    pub request_id: String,
    pub plugin_id: String,
    pub command_id: String,
    pub snapshot_generation: u64,
    pub target: ExtensionTarget,
    pub input: Value,
    pub resource_refs: Vec<ExtensionResourceRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unit_attachments: Vec<ExtensionUnitAttachment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_gesture_token: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionTarget {
    pub unit_id: String,
    pub revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionResourceRef {
    pub resource_id: String,
    pub kind: ExtensionResourceKind,
    pub digest: String,
    pub byte_length: u64,
    pub lease_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionResourceKind {
    File,
    SharedImage,
    SharedMemory,
    Inline,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionUnitAttachment {
    pub attachment_id: String,
    pub type_id: String,
    pub schema_version: String,
    pub revision: u64,
    pub plugin_id: String,
    pub plugin_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub renderer_id: Option<String>,
    #[serde(default)]
    pub payload: Value,
    #[serde(default)]
    pub resource_refs: Vec<ExtensionResourceRef>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionResult {
    pub protocol: String,
    pub api_version: String,
    pub request_id: String,
    pub status: ExtensionResultStatus,
    #[serde(default)]
    pub output: Value,
    pub effects: Vec<ExtensionEffect>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ExtensionError>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionResultStatus {
    Accepted,
    Progress,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionEffect {
    #[serde(rename = "type")]
    pub effect_type: ExtensionEffectType,
    pub payload: Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExtensionEffectType {
    #[serde(rename = "attachment.upsert")]
    AttachmentUpsert,
    #[serde(rename = "attachment.remove")]
    AttachmentRemove,
    #[serde(rename = "notice.show")]
    NoticeShow,
    #[serde(rename = "clipboard.writeText")]
    ClipboardWriteText,
    #[serde(rename = "external.openUrl")]
    ExternalOpenUrl,
    #[serde(rename = "overlay.invalidate")]
    OverlayInvalidate,
    #[serde(rename = "resource.publish")]
    ResourcePublish,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionError {
    pub code: CapabilityErrorCode,
    pub message: String,
}
