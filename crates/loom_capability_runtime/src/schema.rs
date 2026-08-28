use std::collections::HashMap;
use std::fs;

use jsonschema::Validator;
use loom_protocol::{
    CapabilityCommandContribution, CapabilityRuntimeStatus, ExtensionEffect, ExtensionEffectType,
    ExtensionResourceRef,
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
    validators: &CommandSchemaValidators,
    input: &Value,
    resources: &[ExtensionResourceRef],
) -> HostResult<()> {
    validate_resources(resources)?;
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

pub(super) fn validate_command_output(
    package: &CapabilityRuntimePackage,
    command: &CapabilityCommandContribution,
    validators: &CommandSchemaValidators,
    output: &CapabilityInvocationOutput,
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
            validate_effect(package, command, effect, user_gesture)?;
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
    user_gesture: bool,
) -> HostResult<()> {
    let required = match effect.effect_type {
        ExtensionEffectType::AttachmentUpsert | ExtensionEffectType::AttachmentRemove => {
            validate_namespaced_payload(package, &effect.payload, "typeId")?;
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
