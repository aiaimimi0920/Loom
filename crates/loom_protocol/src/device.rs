use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const DEVICE_SESSION_PROTOCOL_VERSION: &str = "loom.device-session.v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSessionChallengeRequest {
    pub device_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSessionChallengeResponse {
    #[serde(default = "default_device_session_protocol_version")]
    pub protocol_version: String,
    pub challenge_id: String,
    pub device_id: String,
    pub challenge: String,
    pub expires_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSessionIssueRequest {
    pub device_id: String,
    pub challenge_id: String,
    pub client_nonce: String,
    pub signature: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSessionIssueResponse {
    #[serde(default = "default_device_session_protocol_version")]
    pub protocol_version: String,
    pub device_id: String,
    pub token: String,
    pub expires_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum DeviceSessionValidationError {
    #[error("unsupported device session protocol `{0}`")]
    UnsupportedProtocol(String),
    #[error("invalid device session identifier `{0}`")]
    InvalidIdentifier(String),
    #[error("invalid device session nonce")]
    InvalidNonce,
    #[error("device session expiry must be greater than zero")]
    InvalidExpiry,
    #[error("device session signature must not be empty")]
    MissingSignature,
}

pub fn device_session_signature_message(
    device_id: &str,
    challenge_id: &str,
    challenge: &str,
    client_nonce: &str,
) -> String {
    format!(
        "{DEVICE_SESSION_PROTOCOL_VERSION}\n{device_id}\n{challenge_id}\n{challenge}\n{client_nonce}"
    )
}

pub fn validate_device_session_challenge_response(
    response: &DeviceSessionChallengeResponse,
) -> Result<(), DeviceSessionValidationError> {
    validate_protocol(&response.protocol_version)?;
    validate_identifier(&response.challenge_id)?;
    validate_identifier(&response.device_id)?;
    validate_nonce(&response.challenge)?;
    if response.expires_at_ms == 0 {
        return Err(DeviceSessionValidationError::InvalidExpiry);
    }
    Ok(())
}

pub fn validate_device_session_issue_request(
    request: &DeviceSessionIssueRequest,
) -> Result<(), DeviceSessionValidationError> {
    validate_identifier(&request.device_id)?;
    validate_identifier(&request.challenge_id)?;
    validate_nonce(&request.client_nonce)?;
    if request.signature.trim().is_empty() {
        return Err(DeviceSessionValidationError::MissingSignature);
    }
    Ok(())
}

pub fn validate_device_session_issue_response(
    response: &DeviceSessionIssueResponse,
) -> Result<(), DeviceSessionValidationError> {
    validate_protocol(&response.protocol_version)?;
    validate_identifier(&response.device_id)?;
    validate_nonce(&response.token)?;
    if response.expires_at_ms == 0 {
        return Err(DeviceSessionValidationError::InvalidExpiry);
    }
    Ok(())
}

fn validate_protocol(protocol: &str) -> Result<(), DeviceSessionValidationError> {
    if protocol != DEVICE_SESSION_PROTOCOL_VERSION {
        return Err(DeviceSessionValidationError::UnsupportedProtocol(
            protocol.to_owned(),
        ));
    }
    Ok(())
}

fn validate_identifier(value: &str) -> Result<(), DeviceSessionValidationError> {
    if value.is_empty()
        || value.len() > 160
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:/-".contains(&byte))
    {
        return Err(DeviceSessionValidationError::InvalidIdentifier(
            value.to_owned(),
        ));
    }
    Ok(())
}

fn validate_nonce(value: &str) -> Result<(), DeviceSessionValidationError> {
    if value.len() < 16
        || value.len() > 160
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(DeviceSessionValidationError::InvalidNonce);
    }
    Ok(())
}

fn default_device_session_protocol_version() -> String {
    DEVICE_SESSION_PROTOCOL_VERSION.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_session_wire_contract_and_signature_context_are_stable() {
        let request = DeviceSessionIssueRequest {
            device_id: "device:one".to_owned(),
            challenge_id: "challenge:one".to_owned(),
            client_nonce: "client_nonce_00000001".to_owned(),
            signature: "signature".to_owned(),
        };
        validate_device_session_issue_request(&request).expect("valid issue request");
        assert_eq!(
            device_session_signature_message(
                &request.device_id,
                &request.challenge_id,
                "challenge_value_0001",
                &request.client_nonce,
            ),
            "loom.device-session.v1\ndevice:one\nchallenge:one\nchallenge_value_0001\nclient_nonce_00000001"
        );
        let value = serde_json::to_value(request).expect("serialize issue request");
        assert_eq!(value["deviceId"], "device:one");
        assert!(value.get("device_id").is_none());
    }
}
