use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::CAPABILITY_RUNTIME_PROTOCOL;

pub const CAPABILITY_RUNTIME_FRAME_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum CapabilityRuntimeMessage {
    Request {
        protocol: String,
        #[serde(rename = "apiVersion")]
        api_version: String,
        #[serde(rename = "requestId")]
        request_id: String,
        method: CapabilityRuntimeMethod,
        payload: Value,
    },
    Response {
        protocol: String,
        #[serde(rename = "apiVersion")]
        api_version: String,
        #[serde(rename = "requestId")]
        request_id: String,
        status: CapabilityRuntimeStatus,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        payload: Option<Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<CapabilityProtocolError>,
    },
    Event {
        protocol: String,
        #[serde(rename = "apiVersion")]
        api_version: String,
        event: CapabilityRuntimeEvent,
        payload: Value,
    },
}

impl CapabilityRuntimeMessage {
    #[must_use]
    pub fn protocol(&self) -> &str {
        match self {
            Self::Request { protocol, .. }
            | Self::Response { protocol, .. }
            | Self::Event { protocol, .. } => protocol,
        }
    }

    #[must_use]
    pub fn api_version(&self) -> &str {
        match self {
            Self::Request { api_version, .. }
            | Self::Response { api_version, .. }
            | Self::Event { api_version, .. } => api_version,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityRuntimeMethod {
    Initialize,
    Activate,
    Deactivate,
    Command,
    Cancel,
    Health,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityRuntimeStatus {
    Accepted,
    Progress,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityRuntimeEvent {
    Contributions,
    Diagnostic,
    HealthChanged,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityErrorCode {
    UnsupportedApi,
    PluginNotActive,
    ContributionConflict,
    PermissionDenied,
    StaleGeneration,
    StaleTarget,
    InvalidInput,
    ResourceNotFound,
    Busy,
    Cancelled,
    RuntimeFault,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityProtocolError {
    pub code: CapabilityErrorCode,
    pub message: String,
    #[serde(default)]
    pub retryable: bool,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CapabilityRuntimeValidationError {
    #[error("runtime frame exceeds {CAPABILITY_RUNTIME_FRAME_BYTES} bytes")]
    FrameTooLarge,
    #[error("runtime frame is invalid JSON: {0}")]
    InvalidJson(String),
    #[error("runtime protocol must be `{CAPABILITY_RUNTIME_PROTOCOL}`")]
    UnsupportedProtocol,
    #[error("runtime API version is unsupported: {0}")]
    UnsupportedApi(String),
    #[error("runtime request id is invalid")]
    InvalidRequestId,
    #[error("runtime error message exceeds 4096 bytes")]
    ErrorMessageTooLarge,
    #[error("failed runtime response requires an error")]
    MissingError,
}

pub fn parse_capability_runtime_frame(
    bytes: &[u8],
) -> Result<CapabilityRuntimeMessage, CapabilityRuntimeValidationError> {
    if bytes.len() > CAPABILITY_RUNTIME_FRAME_BYTES {
        return Err(CapabilityRuntimeValidationError::FrameTooLarge);
    }
    let message = serde_json::from_slice(bytes)
        .map_err(|error| CapabilityRuntimeValidationError::InvalidJson(error.to_string()))?;
    validate_capability_runtime_message(&message)?;
    Ok(message)
}

pub fn validate_capability_runtime_message(
    message: &CapabilityRuntimeMessage,
) -> Result<(), CapabilityRuntimeValidationError> {
    if message.protocol() != CAPABILITY_RUNTIME_PROTOCOL {
        return Err(CapabilityRuntimeValidationError::UnsupportedProtocol);
    }
    validate_api_version(message.api_version())?;
    match message {
        CapabilityRuntimeMessage::Request { request_id, .. }
        | CapabilityRuntimeMessage::Response { request_id, .. } => {
            if !valid_request_id(request_id) {
                return Err(CapabilityRuntimeValidationError::InvalidRequestId);
            }
        }
        CapabilityRuntimeMessage::Event { .. } => {}
    }
    if let CapabilityRuntimeMessage::Response { status, error, .. } = message {
        if matches!(status, CapabilityRuntimeStatus::Failed) && error.is_none() {
            return Err(CapabilityRuntimeValidationError::MissingError);
        }
        if error
            .as_ref()
            .is_some_and(|value| value.message.len() > 4096)
        {
            return Err(CapabilityRuntimeValidationError::ErrorMessageTooLarge);
        }
    }
    Ok(())
}

fn validate_api_version(version: &str) -> Result<(), CapabilityRuntimeValidationError> {
    let valid = version
        .split_once('.')
        .is_some_and(|(major, minor)| major == "1" && minor.parse::<u32>().is_ok());
    if valid {
        Ok(())
    } else {
        Err(CapabilityRuntimeValidationError::UnsupportedApi(
            version.to_owned(),
        ))
    }
}

fn valid_request_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
}
