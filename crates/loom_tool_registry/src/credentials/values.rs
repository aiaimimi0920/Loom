use chrono::DateTime;
use loom_protocol::{is_safe_package_id, is_safe_publisher_id};

use super::error::CredentialError;
use super::types::{
    CredentialInput, CredentialValueType, MAX_CREDENTIAL_JSON_DEPTH, MAX_CREDENTIAL_VALUE_BYTES,
};

pub(super) fn validate_input(input: &CredentialInput) -> Result<(), CredentialError> {
    if !is_safe_package_id(&input.name) {
        return Err(CredentialError::UnsafeName(input.name.clone()));
    }
    for scope in [
        input.scope.framework_id.as_deref(),
        input.scope.art_id.as_deref(),
        input.scope.mcp_server_id.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if !is_safe_scope_reference(scope) {
            return Err(CredentialError::UnsafeScope(scope.to_owned()));
        }
    }
    if input.value.is_empty() {
        return Err(CredentialError::EmptyValue);
    }
    if input.value.len() > MAX_CREDENTIAL_VALUE_BYTES {
        return Err(CredentialError::InvalidValue {
            value_type: input.value_type,
            reason: format!("value exceeds {MAX_CREDENTIAL_VALUE_BYTES} bytes"),
        });
    }
    canonicalize_value(input.value_type, &input.value)?;
    if let Some(expires_at) = &input.expires_at {
        DateTime::parse_from_rfc3339(expires_at)
            .map_err(|error| CredentialError::InvalidExpiration(error.to_string()))?;
    }
    Ok(())
}

pub(super) fn canonicalize_value(
    value_type: CredentialValueType,
    raw: &str,
) -> Result<String, CredentialError> {
    if raw.is_empty() {
        return Err(CredentialError::EmptyValue);
    }
    match value_type {
        CredentialValueType::String => Ok(raw.to_owned()),
        CredentialValueType::Number => match serde_json::from_str::<serde_json::Value>(raw) {
            Ok(serde_json::Value::Number(value)) => Ok(value.to_string()),
            Ok(_) => Err(invalid_value(value_type, "expected a JSON number")),
            Err(error) => Err(invalid_value(value_type, error.to_string())),
        },
        CredentialValueType::Integer => match serde_json::from_str::<serde_json::Value>(raw) {
            Ok(serde_json::Value::Number(value)) if value.is_i64() || value.is_u64() => {
                Ok(value.to_string())
            }
            Ok(_) => Err(invalid_value(value_type, "expected a JSON integer")),
            Err(error) => Err(invalid_value(value_type, error.to_string())),
        },
        CredentialValueType::Boolean => match serde_json::from_str::<serde_json::Value>(raw) {
            Ok(serde_json::Value::Bool(value)) => Ok(value.to_string()),
            Ok(_) => Err(invalid_value(value_type, "expected true or false")),
            Err(error) => Err(invalid_value(value_type, error.to_string())),
        },
        CredentialValueType::Json => loom_security::json::parse_within_limits(
            raw,
            "credential JSON value",
            MAX_CREDENTIAL_VALUE_BYTES,
            MAX_CREDENTIAL_JSON_DEPTH,
        )
        .and_then(|value| serde_json::to_string(&value).map_err(|error| error.to_string()))
        .map_err(|error| invalid_value(value_type, error)),
    }
}

pub(super) fn decode_canonical_value(
    value_type: CredentialValueType,
    raw: &str,
) -> Result<serde_json::Value, CredentialError> {
    match value_type {
        CredentialValueType::String => Ok(serde_json::Value::String(raw.to_owned())),
        CredentialValueType::Json => loom_security::json::parse_within_limits(
            raw,
            "stored credential JSON value",
            MAX_CREDENTIAL_VALUE_BYTES,
            MAX_CREDENTIAL_JSON_DEPTH,
        )
        .map_err(|error| invalid_value(value_type, error)),
        _ => {
            serde_json::from_str(raw).map_err(|error| invalid_value(value_type, error.to_string()))
        }
    }
}

fn invalid_value(value_type: CredentialValueType, reason: impl Into<String>) -> CredentialError {
    CredentialError::InvalidValue {
        value_type,
        reason: reason.into(),
    }
}

pub(super) fn is_safe_scope_reference(value: &str) -> bool {
    value
        .split_once('/')
        .map(|(publisher, id)| {
            !id.contains('/') && is_safe_publisher_id(publisher) && is_safe_package_id(id)
        })
        .unwrap_or_else(|| is_safe_package_id(value))
}
