use std::collections::HashMap;
use std::fs;

use jsonschema::Validator;
use loom_protocol::{
    CapabilityCommandContribution, CapabilityRuntimeStatus, ExtensionEffect, ExtensionEffectType,
    ExtensionResourceKind, ExtensionResourceRef, ExtensionUnitAttachment,
};
use serde_json::Value;

use crate::error::{CapabilityHostError, HostResult};
use crate::host::{CapabilityInvocationOutput, CapabilityRuntimePackage};

const MAX_COMMAND_SCHEMA_BYTES: u64 = 1024 * 1024;

pub(super) struct CommandSchemaValidators {
    input: Option<Validator>,
    output: Option<Validator>,
}

pub(super) fn compile_command_schemas(
    package: &CapabilityRuntimePackage,
) -> HostResult<HashMap<String, CommandSchemaValidators>> {
    package
        .manifest
        .contributes
        .commands
        .iter()
        .map(|command| {
            Ok((
                command.id.clone(),
                CommandSchemaValidators {
                    input: compile_schema(package, command.input_schema.as_deref())?,
                    output: compile_schema(package, command.output_schema.as_deref())?,
                },
            ))
        })
        .collect()
}

pub(super) fn validate_command_input(
    package: &CapabilityRuntimePackage,
    command: &CapabilityCommandContribution,
    validators: &CommandSchemaValidators,
    input: &Value,
    resources: &[ExtensionResourceRef],
    unit_attachments: &[ExtensionUnitAttachment],
) -> HostResult<()> {
    validate_resources(resources)?;
    validate_input_permissions(package, command, resources, unit_attachments)?;
    if validators
        .input
        .as_ref()
        .is_some_and(|validator| !validator.is_valid(input))
    {
        return Err(CapabilityHostError::Protocol(
            "command input does not match its signed schema".to_owned(),
        ));
    }
    Ok(())
}

fn validate_input_permissions(
    package: &CapabilityRuntimePackage,
    command: &CapabilityCommandContribution,
    resources: &[ExtensionResourceRef],
    unit_attachments: &[ExtensionUnitAttachment],
) -> HostResult<()> {
    let requires_image_read = resources.iter().any(|resource| {
        matches!(
            resource.kind,
            ExtensionResourceKind::SharedImage | ExtensionResourceKind::SharedMemory
        )
    });
    if requires_image_read
        && (!package
            .manifest
            .permissions
            .iter()
            .any(|permission| permission == "hook.unit.image.read")
            || !command
                .permissions
                .iter()
                .any(|permission| permission == "hook.unit.image.read"))
    {
        return Err(CapabilityHostError::Protocol(
            "image resources require declared permission `hook.unit.image.read`".to_owned(),
        ));
    }
    if !unit_attachments.is_empty()
        && (!package
            .manifest
            .permissions
            .iter()
            .any(|permission| permission == "hook.unit.attachments.read")
            || !command
                .permissions
                .iter()
                .any(|permission| permission == "hook.unit.attachments.read"))
    {
        return Err(CapabilityHostError::Protocol(
            "unit attachments require declared permission `hook.unit.attachments.read`".to_owned(),
        ));
    }
    if unit_attachments
        .iter()
        .any(|attachment| attachment.plugin_id != package.manifest.qualified_id())
    {
        return Err(CapabilityHostError::Protocol(
            "unit attachments are outside the invoking plugin namespace".to_owned(),
        ));
    }
    Ok(())
}

pub(super) fn validate_command_output(
    package: &CapabilityRuntimePackage,
    command: &CapabilityCommandContribution,
    validators: &CommandSchemaValidators,
    output: &CapabilityInvocationOutput,
    invocation_resources: &[ExtensionResourceRef],
    user_gesture: bool,
) -> HostResult<()> {
    if matches!(output.status, CapabilityRuntimeStatus::Succeeded) && output.error.is_some() {
        return Err(CapabilityHostError::Protocol(
            "successful command returned an error".to_owned(),
        ));
    }
    let payload = output.payload.as_ref().unwrap_or(&Value::Null);
    let schema_value = payload.get("output").unwrap_or(payload);
    if validators
        .output
        .as_ref()
        .is_some_and(|validator| !validator.is_valid(schema_value))
    {
        return Err(CapabilityHostError::Protocol(
            "command output does not match its signed schema".to_owned(),
        ));
    }
    if let Some(effects) = payload.get("effects") {
        let effects = serde_json::from_value::<Vec<ExtensionEffect>>(effects.clone())
            .map_err(|_| CapabilityHostError::Protocol("command effects are invalid".to_owned()))?;
        if effects.len() > 256 {
            return Err(CapabilityHostError::Protocol(
                "command effects exceed the result budget".to_owned(),
            ));
        }
        for effect in &effects {
            validate_effect(package, command, effect, invocation_resources, user_gesture)?;
        }
    }
    Ok(())
}

fn compile_schema(
    package: &CapabilityRuntimePackage,
    relative: Option<&str>,
) -> HostResult<Option<Validator>> {
    let Some(relative) = relative else {
        return Ok(None);
    };
    let root = fs::canonicalize(&package.package_dir)?;
    let path = fs::canonicalize(root.join(relative))?;
    if !path.starts_with(&root) {
        return Err(CapabilityHostError::InvalidPackage(
            "command schema escapes its package root".to_owned(),
        ));
    }
    let metadata = fs::symlink_metadata(&path)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > MAX_COMMAND_SCHEMA_BYTES
    {
        return Err(CapabilityHostError::InvalidPackage(
            "command schema is linked, missing, or oversized".to_owned(),
        ));
    }
    let schema: Value = serde_json::from_slice(&fs::read(path)?)?;
    jsonschema::validator_for(&schema)
        .map(Some)
        .map_err(|_| CapabilityHostError::InvalidPackage("command schema is invalid".to_owned()))
}

fn validate_resources(resources: &[ExtensionResourceRef]) -> HostResult<()> {
    if resources.len() > 128
        || resources.iter().any(|resource| {
            !safe_id(&resource.resource_id)
                || !safe_id(&resource.lease_id)
                || !valid_digest(&resource.digest)
                || resource.byte_length > 536_870_912
        })
    {
        return Err(CapabilityHostError::Protocol(
            "command resource references are invalid".to_owned(),
        ));
    }
    Ok(())
}

fn validate_effect(
    package: &CapabilityRuntimePackage,
    command: &CapabilityCommandContribution,
    effect: &ExtensionEffect,
    invocation_resources: &[ExtensionResourceRef],
    user_gesture: bool,
) -> HostResult<()> {
    let required = match effect.effect_type {
        ExtensionEffectType::AttachmentUpsert => {
            validate_namespaced_payload(package, &effect.payload, "typeId")?;
            validate_namespaced_payload(package, &effect.payload, "attachmentId")?;
            validate_attachment_resources(effect, invocation_resources)?;
            "hook.unit.attachments.write"
        }
        ExtensionEffectType::AttachmentRemove => {
            validate_namespaced_payload(package, &effect.payload, "attachmentId")?;
            "hook.unit.attachments.write"
        }
        ExtensionEffectType::NoticeShow => "hook.notice.show",
        ExtensionEffectType::ClipboardWriteText => {
            if !user_gesture {
                return Err(CapabilityHostError::Protocol(
                    "clipboard effect requires a consumed user gesture".to_owned(),
                ));
            }
            "hook.clipboard.write"
        }
        ExtensionEffectType::ExternalOpenUrl => {
            let url = effect.payload.get("url").and_then(Value::as_str);
            if !user_gesture || !url.is_some_and(loom_protocol::is_safe_external_https_url) {
                return Err(CapabilityHostError::Protocol(
                    "external URL effect requires a consumed user gesture and a safe HTTPS URL"
                        .to_owned(),
                ));
            }
            "hook.external.open"
        }
        ExtensionEffectType::OverlayInvalidate => "hook.overlay.render",
        ExtensionEffectType::ResourcePublish => {
            validate_namespaced_payload(package, &effect.payload, "resourceId")?;
            "hook.unit.attachments.write"
        }
    };
    if !package
        .manifest
        .permissions
        .iter()
        .any(|value| value == required)
        || !command.permissions.iter().any(|value| value == required)
    {
        return Err(CapabilityHostError::Protocol(format!(
            "command effect requires undeclared permission `{required}`"
        )));
    }
    Ok(())
}

fn validate_attachment_resources(
    effect: &ExtensionEffect,
    invocation_resources: &[ExtensionResourceRef],
) -> HostResult<()> {
    let Some(resources) = effect.payload.get("resourceRefs") else {
        return Ok(());
    };
    let resources = serde_json::from_value::<Vec<ExtensionResourceRef>>(resources.clone())
        .map_err(|_| {
            CapabilityHostError::Protocol("attachment resource references are invalid".to_owned())
        })?;
    if resources
        .iter()
        .any(|resource| !invocation_resources.contains(resource))
    {
        return Err(CapabilityHostError::Protocol(
            "attachment effect references a resource outside its invocation".to_owned(),
        ));
    }
    Ok(())
}

fn validate_namespaced_payload(
    package: &CapabilityRuntimePackage,
    payload: &Value,
    field: &str,
) -> HostResult<()> {
    let namespace = format!("{}.", package.manifest.qualified_id());
    if !payload
        .get(field)
        .and_then(Value::as_str)
        .is_some_and(|value| value.starts_with(&namespace) && safe_id(value))
    {
        return Err(CapabilityHostError::Protocol(format!(
            "effect {field} is outside the plugin namespace"
        )));
    }
    Ok(())
}

fn safe_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 384
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'/' | b'-')
        })
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}
