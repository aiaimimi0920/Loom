use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use super::CapabilityErrorCode;
use crate::EXTENSION_PROTOCOL;

pub const EXTENSION_SNAPSHOT_BYTES: usize = 2 * 1024 * 1024;

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

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ExtensionValidationError {
    #[error("extension message exceeds {EXTENSION_SNAPSHOT_BYTES} bytes")]
    MessageTooLarge,
    #[error("extension message is invalid JSON: {0}")]
    InvalidJson(String),
    #[error("extension protocol or API version is unsupported")]
    UnsupportedProtocol,
    #[error("extension snapshot exceeds a contribution budget")]
    ContributionLimit,
    #[error("extension plugin or contribution id is duplicated: {0}")]
    DuplicateId(String),
    #[error("extension contribution has no matching plugin scope: {0}")]
    InvalidScope(String),
    #[error("extension digest is invalid")]
    InvalidDigest,
    #[error("extension invocation is invalid")]
    InvalidInvocation,
    #[error("failed extension result requires an error")]
    MissingError,
    #[error("extension effect payload is invalid")]
    InvalidEffect,
}

pub fn parse_extension_message(bytes: &[u8]) -> Result<ExtensionMessage, ExtensionValidationError> {
    if bytes.len() > EXTENSION_SNAPSHOT_BYTES {
        return Err(ExtensionValidationError::MessageTooLarge);
    }
    let message = serde_json::from_slice(bytes)
        .map_err(|error| ExtensionValidationError::InvalidJson(error.to_string()))?;
    validate_extension_message(&message)?;
    Ok(message)
}

pub fn validate_extension_message(
    message: &ExtensionMessage,
) -> Result<(), ExtensionValidationError> {
    match message {
        ExtensionMessage::Snapshot(snapshot) => validate_snapshot(snapshot),
        ExtensionMessage::Invocation(invocation) => validate_invocation(invocation),
        ExtensionMessage::Result(result) => validate_result(result),
    }
}

fn validate_snapshot(snapshot: &ContributionSnapshot) -> Result<(), ExtensionValidationError> {
    validate_protocol(&snapshot.protocol, &snapshot.api_version)?;
    if snapshot.plugins.len() > 128 || snapshot.contributions.iter().count() > 2048 {
        return Err(ExtensionValidationError::ContributionLimit);
    }
    let mut plugins = HashMap::new();
    for plugin in &snapshot.plugins {
        if !valid_digest(&plugin.package_digest) || !valid_digest(&plugin.permission_grant_digest) {
            return Err(ExtensionValidationError::InvalidDigest);
        }
        if plugins
            .insert(plugin.id.as_str(), plugin.scope_id.as_str())
            .is_some()
        {
            return Err(ExtensionValidationError::DuplicateId(plugin.id.clone()));
        }
    }
    let mut contributions = HashSet::new();
    for contribution in snapshot.contributions.iter() {
        if !contributions.insert(contribution.id.to_ascii_lowercase()) {
            return Err(ExtensionValidationError::DuplicateId(
                contribution.id.clone(),
            ));
        }
        let valid_scope = plugins
            .get(contribution.plugin_id.as_str())
            .is_some_and(|scope| *scope == contribution.scope_id);
        if !valid_scope {
            return Err(ExtensionValidationError::InvalidScope(
                contribution.id.clone(),
            ));
        }
    }
    Ok(())
}

fn validate_invocation(invocation: &ExtensionInvocation) -> Result<(), ExtensionValidationError> {
    validate_protocol(&invocation.protocol, &invocation.api_version)?;
    let gesture_valid = invocation
        .user_gesture_token
        .as_ref()
        .is_none_or(|token| (16..=512).contains(&token.len()));
    let resources_valid = invocation.resource_refs.len() <= 128
        && invocation
            .resource_refs
            .iter()
            .all(|resource| valid_digest(&resource.digest) && resource.byte_length <= 536_870_912);
    if invocation.request_id.is_empty()
        || invocation.request_id.len() > 384
        || invocation.plugin_id.is_empty()
        || invocation.command_id.is_empty()
        || !gesture_valid
        || !resources_valid
    {
        return Err(ExtensionValidationError::InvalidInvocation);
    }
    Ok(())
}

fn validate_result(result: &ExtensionResult) -> Result<(), ExtensionValidationError> {
    validate_protocol(&result.protocol, &result.api_version)?;
    if result.effects.len() > 256 || result.request_id.is_empty() {
        return Err(ExtensionValidationError::ContributionLimit);
    }
    if matches!(result.status, ExtensionResultStatus::Failed) && result.error.is_none() {
        return Err(ExtensionValidationError::MissingError);
    }
    for effect in &result.effects {
        validate_effect(effect)?;
    }
    Ok(())
}

fn validate_effect(effect: &ExtensionEffect) -> Result<(), ExtensionValidationError> {
    let Some(payload) = effect.payload.as_object() else {
        return Err(ExtensionValidationError::InvalidEffect);
    };
    match effect.effect_type {
        ExtensionEffectType::AttachmentUpsert => {
            let allowed = [
                "attachmentId",
                "typeId",
                "schemaVersion",
                "priorRevision",
                "revision",
                "rendererId",
                "payload",
                "resourceRefs",
            ];
            if payload.keys().any(|key| !allowed.contains(&key.as_str()))
                || !bounded_effect_string(payload.get("attachmentId"), 384)
                || !bounded_effect_string(payload.get("typeId"), 384)
                || !bounded_effect_string(payload.get("schemaVersion"), 64)
                || payload
                    .get("rendererId")
                    .is_some_and(|value| !bounded_effect_string(Some(value), 384))
            {
                return Err(ExtensionValidationError::InvalidEffect);
            }
            let Some(prior) = payload.get("priorRevision").and_then(Value::as_u64) else {
                return Err(ExtensionValidationError::InvalidEffect);
            };
            if payload.get("revision").and_then(Value::as_u64) != prior.checked_add(1) {
                return Err(ExtensionValidationError::InvalidEffect);
            }
            let payload_present = payload.get("payload").is_some();
            let resource_refs = payload.get("resourceRefs").and_then(Value::as_array);
            if !payload_present && resource_refs.is_none_or(Vec::is_empty) {
                return Err(ExtensionValidationError::InvalidEffect);
            }
            if payload.get("payload").is_some_and(|value| {
                serde_json::to_vec(value).map_or(true, |bytes| bytes.len() > 256 * 1024)
            }) || resource_refs.is_some_and(|values| {
                values.len() > 16
                    || values.iter().any(|value| {
                        serde_json::from_value::<ExtensionResourceRef>(value.clone())
                            .map_or(true, |resource| !valid_effect_resource(&resource))
                    })
            }) {
                return Err(ExtensionValidationError::InvalidEffect);
            }
        }
        ExtensionEffectType::AttachmentRemove => {
            let allowed = ["attachmentId", "priorRevision"];
            if payload.keys().any(|key| !allowed.contains(&key.as_str()))
                || !bounded_effect_string(payload.get("attachmentId"), 384)
                || payload
                    .get("priorRevision")
                    .and_then(Value::as_u64)
                    .is_none()
            {
                return Err(ExtensionValidationError::InvalidEffect);
            }
        }
        ExtensionEffectType::NoticeShow => {
            let allowed = ["title", "message"];
            if payload.keys().any(|key| !allowed.contains(&key.as_str()))
                || payload
                    .get("title")
                    .is_some_and(|value| !bounded_effect_string(Some(value), 128))
                || payload
                    .get("message")
                    .is_some_and(|value| !bounded_effect_string(Some(value), 2048))
            {
                return Err(ExtensionValidationError::InvalidEffect);
            }
        }
        ExtensionEffectType::ClipboardWriteText => {
            if payload.keys().any(|key| key != "text")
                || !bounded_effect_string(payload.get("text"), 1_048_576)
            {
                return Err(ExtensionValidationError::InvalidEffect);
            }
        }
        ExtensionEffectType::OverlayInvalidate | ExtensionEffectType::ResourcePublish => {
            if serde_json::to_vec(payload).map_or(true, |bytes| bytes.len() > 256 * 1024) {
                return Err(ExtensionValidationError::InvalidEffect);
            }
        }
    }
    Ok(())
}

fn bounded_effect_string(value: Option<&Value>, maximum: usize) -> bool {
    value
        .and_then(Value::as_str)
        .is_some_and(|value| !value.is_empty() && value.len() <= maximum)
}

fn valid_effect_resource(resource: &ExtensionResourceRef) -> bool {
    resource.kind != ExtensionResourceKind::Inline
        && valid_digest(&resource.digest)
        && resource.resource_id == format!("sha256:{}", resource.digest)
        && resource.byte_length <= 64 * 1024 * 1024
        && !resource.lease_id.is_empty()
        && resource.lease_id.len() <= 384
}

fn validate_protocol(protocol: &str, api_version: &str) -> Result<(), ExtensionValidationError> {
    let supported_api = api_version
        .split_once('.')
        .is_some_and(|(major, minor)| major == "1" && minor.parse::<u32>().is_ok());
    if protocol == EXTENSION_PROTOCOL && supported_api {
        Ok(())
    } else {
        Err(ExtensionValidationError::UnsupportedProtocol)
    }
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}
