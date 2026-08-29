use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use super::{ContributionSnapshot, ExtensionInvocation};
use crate::EXTENSION_PROTOCOL;

pub const EXTENSION_METHOD_HANDSHAKE: &str = "loom.extension.handshake";
pub const EXTENSION_METHOD_SNAPSHOT_GET: &str = "loom.extension.snapshot.get";
pub const EXTENSION_METHOD_COMMAND_INVOKE: &str = "loom.extension.command.invoke";
pub const EXTENSION_EVENT_SNAPSHOT_UPDATED: &str = "loom.extension.snapshot.updated";
pub const EXTENSION_REQUEST_METHODS: &[&str] = &[
    EXTENSION_METHOD_HANDSHAKE,
    EXTENSION_METHOD_SNAPSHOT_GET,
    EXTENSION_METHOD_COMMAND_INVOKE,
];
pub const EXTENSION_EVENT_METHODS: &[&str] = &[EXTENSION_EVENT_SNAPSHOT_UPDATED];

pub const EXTENSION_FEATURE_SNAPSHOT: &str = "contribution.snapshot";
pub const EXTENSION_FEATURE_COMMANDS: &str = "command.invoke";
pub const EXTENSION_FEATURE_SHORTCUTS: &str = "shortcut.registry";
pub const EXTENSION_FEATURE_MENUS: &str = "menu.registry";
pub const EXTENSION_FEATURE_NOTICES: &str = "notice.effects";
pub const EXTENSION_FEATURES: &[&str] = &[
    EXTENSION_FEATURE_SNAPSHOT,
    EXTENSION_FEATURE_COMMANDS,
    EXTENSION_FEATURE_SHORTCUTS,
    EXTENSION_FEATURE_MENUS,
    EXTENSION_FEATURE_NOTICES,
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum ExtensionBridgeRequest {
    #[serde(rename = "loom.extension.handshake")]
    Handshake(ExtensionHandshakeRequest),
    #[serde(rename = "loom.extension.snapshot.get")]
    SnapshotGet(ExtensionSessionRequest),
    #[serde(rename = "loom.extension.command.invoke")]
    CommandInvoke(ExtensionCommandInvokeRequest),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionHandshakeRequest {
    pub request_id: String,
    pub hook_session_id: String,
    pub protocol: String,
    pub api_version: String,
    #[serde(default)]
    pub required_features: Vec<String>,
    #[serde(default)]
    pub optional_features: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionSessionRequest {
    pub request_id: String,
    pub session_id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionCommandInvokeRequest {
    pub session_id: String,
    pub invocation: ExtensionInvocation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionHandshakeData {
    pub session_id: String,
    pub features: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionBridgeStatus {
    Succeeded,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionBridgeResponse {
    pub protocol: String,
    pub api_version: String,
    pub request_id: String,
    pub status: ExtensionBridgeStatus,
    #[serde(default)]
    pub data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ExtensionBridgeError>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionBridgeError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionSnapshotEvent {
    pub protocol: String,
    pub api_version: String,
    pub method: String,
    pub params: ExtensionSnapshotEventParams,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionSnapshotEventParams {
    pub snapshot: ContributionSnapshot,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ExtensionHandshakeError {
    #[error("extension protocol or API version is unsupported")]
    UnsupportedProtocol,
    #[error("extension handshake request is invalid")]
    InvalidRequest,
    #[error("required extension feature is unsupported: {0}")]
    UnsupportedFeature(String),
}

pub fn negotiate_extension_features(
    request: &ExtensionHandshakeRequest,
) -> Result<Vec<String>, ExtensionHandshakeError> {
    if request.protocol != EXTENSION_PROTOCOL
        || !request
            .api_version
            .split_once('.')
            .is_some_and(|(major, minor)| major == "1" && minor.parse::<u32>().is_ok())
    {
        return Err(ExtensionHandshakeError::UnsupportedProtocol);
    }
    if request.request_id.is_empty()
        || request.request_id.len() > 384
        || request.hook_session_id.is_empty()
        || request.required_features.len() > 32
        || request.optional_features.len() > 32
    {
        return Err(ExtensionHandshakeError::InvalidRequest);
    }
    if let Some(feature) = request
        .required_features
        .iter()
        .find(|feature| !EXTENSION_FEATURES.contains(&feature.as_str()))
    {
        return Err(ExtensionHandshakeError::UnsupportedFeature(feature.clone()));
    }
    let mut negotiated = request.required_features.clone();
    for feature in &request.optional_features {
        if EXTENSION_FEATURES.contains(&feature.as_str()) && !negotiated.contains(feature) {
            negotiated.push(feature.clone());
        }
    }
    Ok(negotiated)
}
