use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use super::{
    ContributionSnapshot, ExtensionEffect, ExtensionEffectType, ExtensionInvocation,
    ExtensionMessage, ExtensionResourceKind, ExtensionResourceRef, ExtensionResult,
    ExtensionResultStatus, EXTENSION_SNAPSHOT_BYTES, MAX_EXTENSION_INVOCATION_RESOURCE_BYTES,
    MAX_EXTENSION_RESOURCE_REFS,
};
use crate::EXTENSION_PROTOCOL;

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
        ExtensionMessage::Invocation(invocation) => validate_extension_invocation(invocation),
        ExtensionMessage::Result(result) => validate_result(result),
    }
}

/// Validates one invocation without cloning its potentially large payload.
pub fn validate_extension_invocation(
    invocation: &ExtensionInvocation,
) -> Result<(), ExtensionValidationError> {
    validate_invocation(invocation)
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
    let resources_valid = valid_invocation_resources(&invocation.resource_refs);
    let attachments_valid = invocation.unit_attachments.len() <= 32
        && serialized_within(&invocation.unit_attachments, EXTENSION_SNAPSHOT_BYTES)
        && invocation.unit_attachments.iter().all(|attachment| {
            !attachment.attachment_id.is_empty()
                && attachment.attachment_id.len() <= 384
                && !attachment.type_id.is_empty()
                && attachment.type_id.len() <= 384
                && !attachment.schema_version.is_empty()
                && attachment.schema_version.len() <= 64
                && !attachment.plugin_id.is_empty()
                && attachment.plugin_id.len() <= 384
                && !attachment.plugin_version.is_empty()
                && attachment.plugin_version.len() <= 128
                && attachment
                    .renderer_id
                    .as_ref()
                    .is_none_or(|renderer| !renderer.is_empty() && renderer.len() <= 384)
                && serialized_within(&attachment.payload, 256 * 1024)
                && attachment.resource_refs.len() <= 16
                && attachment.resource_refs.iter().all(valid_effect_resource)
        });
    if invocation.request_id.is_empty()
        || invocation.request_id.len() > 384
        || invocation.plugin_id.is_empty()
        || invocation.command_id.is_empty()
        || !gesture_valid
        || !resources_valid
        || !attachments_valid
    {
        return Err(ExtensionValidationError::InvalidInvocation);
    }
    Ok(())
}

fn valid_invocation_resources(resources: &[ExtensionResourceRef]) -> bool {
    if resources.len() > MAX_EXTENSION_RESOURCE_REFS {
        return false;
    }
    let mut resource_ids = HashSet::with_capacity(resources.len());
    let mut lease_ids = HashSet::with_capacity(resources.len());
    let mut total_bytes = 0u64;
    for resource in resources {
        if !valid_effect_resource(resource)
            || !resource_ids.insert(resource.resource_id.as_str())
            || !lease_ids.insert(resource.lease_id.as_str())
        {
            return false;
        }
        let Some(next_total) = total_bytes.checked_add(resource.byte_length) else {
            return false;
        };
        if next_total > MAX_EXTENSION_INVOCATION_RESOURCE_BYTES {
            return false;
        }
        total_bytes = next_total;
    }
    true
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
        ExtensionEffectType::AttachmentUpsert => validate_attachment_upsert(payload)?,
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
        ExtensionEffectType::ExternalOpenUrl => {
            if payload.keys().any(|key| key != "url")
                || !payload
                    .get("url")
                    .and_then(Value::as_str)
                    .is_some_and(crate::capability::is_safe_external_https_url)
            {
                return Err(ExtensionValidationError::InvalidEffect);
            }
        }
        ExtensionEffectType::OverlayInvalidate | ExtensionEffectType::ResourcePublish => {
            if !serialized_within(payload, 256 * 1024) {
                return Err(ExtensionValidationError::InvalidEffect);
            }
        }
    }
    Ok(())
}

fn validate_attachment_upsert(
    payload: &serde_json::Map<String, Value>,
) -> Result<(), ExtensionValidationError> {
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
    if payload
        .get("payload")
        .is_some_and(|value| !serialized_within(value, 256 * 1024))
        || resource_refs.is_some_and(|values| {
            values.len() > 16
                || values.iter().any(|value| {
                    ExtensionResourceRef::deserialize(value)
                        .map_or(true, |resource| !valid_effect_resource(&resource))
                })
        })
    {
        return Err(ExtensionValidationError::InvalidEffect);
    }
    Ok(())
}

/// Counts encoded bytes without allocating the encoded document.
fn serialized_within(value: &impl Serialize, maximum: usize) -> bool {
    struct BudgetWriter {
        remaining: usize,
    }

    impl std::io::Write for BudgetWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.remaining = self.remaining.checked_sub(buf.len()).ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::WriteZero,
                    "serialized value exceeds its budget",
                )
            })?;
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    serde_json::to_writer(BudgetWriter { remaining: maximum }, value).is_ok()
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
        && resource.byte_length <= MAX_EXTENSION_INVOCATION_RESOURCE_BYTES
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
