//! Stable Loom-to-Otter control protocol.
//!
//! `loom.hook.v1` owns connection negotiation, capability discovery, workflow
//! control, and traditional Art-node execution. Distributed UI messages remain
//! in `loom.surface.v1`; device authentication remains in
//! `loom.device-session.v1`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::{
    SurfaceExecutionError, SurfaceHostCapabilities, SurfaceResourceDescriptor,
    SURFACE_EVENT_METHODS,
};

pub const HOOK_PROTOCOL_VERSION: &str = "loom.hook.v1";

pub const HOOK_METHOD_HANDSHAKE: &str = "loom.hook.handshake";
pub const HOOK_METHOD_CAPABILITIES_LIST: &str = "loom.hook.capabilities.list";
pub const HOOK_METHOD_SUBSCRIBE: &str = "loom.hook.subscribe";
pub const HOOK_METHOD_WORKFLOW_SYNC: &str = "loom.hook.workflow.sync";
pub const HOOK_METHOD_WORKFLOW_NODE_UPDATE: &str = "loom.hook.workflow.node.update";
pub const HOOK_METHOD_WORKFLOW_INSTANTIATE: &str = "loom.hook.workflow.instantiate";
pub const HOOK_METHOD_ART_EXECUTE: &str = "loom.hook.art.execute";
pub const HOOK_METHOD_ART_CANCEL: &str = "loom.hook.art.cancel";
pub const HOOK_METHOD_ART_RESOURCES_RELEASE: &str = "loom.hook.art.resources.release";
pub const HOOK_METHOD_SETTINGS_GET: &str = "loom.hook.settings.get";

pub const HOOK_EVENT_WORKFLOW_INSTANTIATED: &str = "loom.hook.workflow.instantiated";
pub const HOOK_EVENT_WORKFLOW_UPDATED: &str = "loom.hook.workflow.updated";
pub const HOOK_EVENT_CAPABILITIES_UPDATED: &str = "loom.hook.capabilities.updated";
pub const HOOK_EVENT_ART_ACK: &str = "loom.hook.art.ack";
pub const HOOK_EVENT_ART_PROGRESS: &str = "loom.hook.art.progress";
pub const HOOK_EVENT_ART_PREVIEW: &str = "loom.hook.art.preview";
pub const HOOK_EVENT_ART_RESULT: &str = "loom.hook.art.result";
pub const HOOK_EVENT_ART_FAILURE: &str = "loom.hook.art.failure";
pub const HOOK_EVENT_SETTINGS_UPDATED: &str = "loom.hook.settings.updated";
pub const HOOK_EVENT_CACHE_CONTROL: &str = "loom.hook.cache.control";

pub const HOOK_REQUEST_METHODS: &[&str] = &[
    HOOK_METHOD_HANDSHAKE,
    HOOK_METHOD_CAPABILITIES_LIST,
    HOOK_METHOD_SUBSCRIBE,
    HOOK_METHOD_WORKFLOW_SYNC,
    HOOK_METHOD_WORKFLOW_NODE_UPDATE,
    HOOK_METHOD_WORKFLOW_INSTANTIATE,
    HOOK_METHOD_ART_EXECUTE,
    HOOK_METHOD_ART_CANCEL,
    HOOK_METHOD_ART_RESOURCES_RELEASE,
    HOOK_METHOD_SETTINGS_GET,
];

pub const HOOK_EVENT_METHODS: &[&str] = &[
    HOOK_EVENT_WORKFLOW_INSTANTIATED,
    HOOK_EVENT_WORKFLOW_UPDATED,
    HOOK_EVENT_CAPABILITIES_UPDATED,
    HOOK_EVENT_ART_ACK,
    HOOK_EVENT_ART_PROGRESS,
    HOOK_EVENT_ART_PREVIEW,
    HOOK_EVENT_ART_RESULT,
    HOOK_EVENT_ART_FAILURE,
    HOOK_EVENT_SETTINGS_UPDATED,
    HOOK_EVENT_CACHE_CONTROL,
];

pub fn otter_subscription_event_supported(event: &str) -> bool {
    HOOK_EVENT_METHODS.contains(&event) || SURFACE_EVENT_METHODS.contains(&event)
}

fn default_otter_protocol_version() -> String {
    HOOK_PROTOCOL_VERSION.to_owned()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OtterTransportMode {
    Websocket,
    SharedMemory,
    CloudflareRelay,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OtterHandshakeRequest {
    #[serde(default = "default_otter_protocol_version")]
    pub protocol_version: String,
    #[serde(default)]
    pub supported_protocol_versions: Vec<String>,
    pub client_id: String,
    pub client_version: String,
    pub platform: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    #[serde(default)]
    pub transports: Vec<OtterTransportMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface: Option<SurfaceHostCapabilities>,
}

impl OtterHandshakeRequest {
    #[must_use]
    pub fn advertised_protocol_versions(&self) -> Vec<&str> {
        let mut versions = vec![self.protocol_version.as_str()];
        for version in &self.supported_protocol_versions {
            if !versions.contains(&version.as_str()) {
                versions.push(version.as_str());
            }
        }
        versions
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OtterCapabilities {
    #[serde(default)]
    pub art_definitions: Vec<OtterArtCapability>,
    pub surface: SurfaceHostCapabilities,
    #[serde(default)]
    pub operations: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OtterArtCapability {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_capability_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub auto_process: bool,
    #[serde(default)]
    pub supported_transports: Vec<OtterTransportMode>,
    #[serde(default)]
    pub parameters: Vec<Value>,
    #[serde(default)]
    pub inputs: Vec<Value>,
    #[serde(default)]
    pub outputs: Vec<Value>,
    #[serde(default)]
    pub execution: Value,
    #[serde(default)]
    pub defaults: Value,
    #[serde(default)]
    pub metadata: Value,
    #[serde(default)]
    pub default_visibility: BTreeMap<String, bool>,
}

const fn default_capability_enabled() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OtterHandshakeResponse {
    #[serde(default = "default_otter_protocol_version")]
    pub protocol_version: String,
    pub server_name: String,
    pub server_version: String,
    pub session_id: String,
    pub transport: OtterTransportMode,
    pub capabilities: OtterCapabilities,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum OtterRequest {
    #[serde(rename = "loom.hook.handshake")]
    Handshake(OtterHandshakeRequest),
    #[serde(rename = "loom.hook.capabilities.list")]
    CapabilitiesList(OtterCapabilitiesListRequest),
    #[serde(rename = "loom.hook.subscribe")]
    Subscribe(OtterSubscribeRequest),
    #[serde(rename = "loom.hook.workflow.sync")]
    WorkflowSync(OtterWorkflowSyncRequest),
    #[serde(rename = "loom.hook.workflow.node.update")]
    WorkflowNodeUpdate(OtterWorkflowNodeUpdateRequest),
    #[serde(rename = "loom.hook.workflow.instantiate")]
    WorkflowInstantiate(OtterWorkflowInstantiateRequest),
    #[serde(rename = "loom.hook.art.execute")]
    ArtExecute(OtterArtExecuteRequest),
    #[serde(rename = "loom.hook.art.cancel")]
    ArtCancel(OtterArtCancelRequest),
    #[serde(rename = "loom.hook.art.resources.release")]
    ArtResourcesRelease(OtterArtResourcesReleaseRequest),
    #[serde(rename = "loom.hook.settings.get")]
    SettingsGet(OtterSettingsGetRequest),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OtterCapabilitiesListRequest {
    pub request_id: String,
    #[serde(default)]
    pub enabled_only: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OtterSubscribeRequest {
    pub request_id: String,
    #[serde(default)]
    pub events: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OtterWorkflowSyncRequest {
    pub request_id: String,
    pub workflow_id: String,
    pub snapshot: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OtterWorkflowNodeUpdateRequest {
    pub request_id: String,
    pub workflow_id: String,
    pub node_id: String,
    pub parameter_id: String,
    pub value: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OtterWorkflowInstantiateRequest {
    pub request_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<String>,
    pub mode: String,
    #[serde(default)]
    pub nodes: Vec<Value>,
    #[serde(default)]
    pub edges: Vec<Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OtterArtExecuteRequest {
    pub protocol_version: String,
    pub request_id: String,
    pub node_id: String,
    pub art_id: String,
    pub generation: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    pub output_transports: Vec<OtterTransportMode>,
    #[serde(default)]
    pub inputs: BTreeMap<String, OtterArtPortValue>,
    #[serde(default)]
    pub parameters: BTreeMap<String, Value>,
    #[serde(default)]
    pub disabled_parameters: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline_at_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OtterArtPortValue {
    Value {
        value: Value,
    },
    InlineResource {
        mime: String,
        #[serde(rename = "dataBase64")]
        data_base64: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        width: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        height: Option<u32>,
    },
    SharedMemory {
        handle: String,
        size: u64,
        width: u32,
        height: u32,
        format: String,
    },
    Resource {
        resource: SurfaceResourceDescriptor,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OtterArtCancelRequest {
    pub protocol_version: String,
    pub request_id: String,
    pub node_id: String,
    pub generation: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OtterArtResourcesReleaseRequest {
    pub protocol_version: String,
    pub request_id: String,
    pub execution_request_id: String,
    pub node_id: String,
    pub generation: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    pub handles: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OtterSettingsGetRequest {
    pub request_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OtterRequestStatus {
    Accepted,
    Running,
    CancelRequested,
    Cancelled,
    Succeeded,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OtterResponse {
    #[serde(default = "default_otter_protocol_version")]
    pub protocol_version: String,
    pub request_id: String,
    pub status: OtterRequestStatus,
    #[serde(default)]
    pub data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<SurfaceExecutionError>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OtterArtAck {
    #[serde(default = "default_otter_protocol_version")]
    pub protocol_version: String,
    pub request_id: String,
    pub node_id: String,
    pub generation: u64,
    pub accepted: bool,
    pub status: OtterRequestStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<SurfaceExecutionError>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OtterArtProgress {
    #[serde(default = "default_otter_protocol_version")]
    pub protocol_version: String,
    pub request_id: String,
    pub node_id: String,
    pub generation: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_key: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OtterArtPreviewCommit {
    #[serde(default = "default_otter_protocol_version")]
    pub protocol_version: String,
    pub request_id: String,
    pub node_id: String,
    pub generation: u64,
    pub preview_revision: u64,
    pub port_id: String,
    pub value: OtterArtPortValue,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OtterArtResultCommit {
    #[serde(default = "default_otter_protocol_version")]
    pub protocol_version: String,
    pub request_id: String,
    pub node_id: String,
    pub generation: u64,
    pub result_revision: u64,
    #[serde(default)]
    pub outputs: BTreeMap<String, OtterArtPortValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidates: Option<Value>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OtterArtFailure {
    #[serde(default = "default_otter_protocol_version")]
    pub protocol_version: String,
    pub request_id: String,
    pub node_id: String,
    pub generation: u64,
    pub error: SurfaceExecutionError,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_successful_result_revision: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookEvent {
    #[serde(default = "default_otter_protocol_version")]
    pub protocol_version: String,
    pub method: String,
    pub params: Value,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum OtterProtocolError {
    #[error("unsupported Otter protocol; client advertises {advertised:?}")]
    UnsupportedProtocol { advertised: Vec<String> },
    #[error("invalid Otter protocol identifier: {0}")]
    InvalidIdentifier(String),
    #[error("unsupported Otter transport")]
    UnsupportedTransport,
}

pub fn negotiate_otter_protocol(
    request: &OtterHandshakeRequest,
) -> Result<&'static str, OtterProtocolError> {
    let advertised = request.advertised_protocol_versions();
    if advertised.contains(&HOOK_PROTOCOL_VERSION) {
        return Ok(HOOK_PROTOCOL_VERSION);
    }
    Err(OtterProtocolError::UnsupportedProtocol {
        advertised: advertised.into_iter().map(str::to_owned).collect(),
    })
}

pub fn validate_otter_handshake(request: &OtterHandshakeRequest) -> Result<(), OtterProtocolError> {
    negotiate_otter_protocol(request)?;
    for value in [
        request.client_id.as_str(),
        request.client_version.as_str(),
        request.platform.as_str(),
    ] {
        if !is_safe_otter_identifier(value) {
            return Err(OtterProtocolError::InvalidIdentifier(value.to_owned()));
        }
    }
    if request.transports.is_empty() {
        return Err(OtterProtocolError::UnsupportedTransport);
    }
    Ok(())
}

#[must_use]
pub fn is_safe_otter_identifier(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty()
        && value.len() <= 160
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/')
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SurfaceInputCapabilities;

    fn surface_capabilities() -> SurfaceHostCapabilities {
        SurfaceHostCapabilities {
            api_version: crate::SURFACE_API_VERSION.to_owned(),
            runtimes: Vec::new(),
            nodes: Vec::new(),
            transports: Vec::new(),
            capabilities: Vec::new(),
            input: SurfaceInputCapabilities {
                pointer: true,
                hover: true,
                touch: false,
                keyboard: true,
            },
        }
    }

    #[test]
    fn otter_handshake_negotiates_from_supported_versions() {
        let request = OtterHandshakeRequest {
            protocol_version: "loom.hook.v2".to_owned(),
            supported_protocol_versions: vec![HOOK_PROTOCOL_VERSION.to_owned()],
            client_id: "hook.desktop".to_owned(),
            client_version: "0.1.7".to_owned(),
            platform: "windows-x64".to_owned(),
            device_id: Some("device:local".to_owned()),
            transports: vec![OtterTransportMode::SharedMemory],
            surface: Some(surface_capabilities()),
        };

        assert_eq!(
            negotiate_otter_protocol(&request),
            Ok(HOOK_PROTOCOL_VERSION)
        );
        validate_otter_handshake(&request).expect("valid handshake");
    }

    #[test]
    fn otter_handshake_rejects_unsupported_protocols() {
        let request = OtterHandshakeRequest {
            protocol_version: "loom.hook.v2".to_owned(),
            supported_protocol_versions: Vec::new(),
            client_id: "hook.desktop".to_owned(),
            client_version: "0.1.7".to_owned(),
            platform: "windows-x64".to_owned(),
            device_id: None,
            transports: vec![OtterTransportMode::Websocket],
            surface: None,
        };

        assert!(matches!(
            negotiate_otter_protocol(&request),
            Err(OtterProtocolError::UnsupportedProtocol { .. })
        ));
    }

    #[test]
    fn otter_request_uses_only_namespaced_methods() {
        let request = OtterRequest::ArtCancel(OtterArtCancelRequest {
            protocol_version: HOOK_PROTOCOL_VERSION.to_owned(),
            request_id: "request:1".to_owned(),
            node_id: "node:1".to_owned(),
            generation: 3,
            device_id: Some("device:local".to_owned()),
        });
        let encoded = serde_json::to_value(request).expect("serialize request");
        assert_eq!(encoded["method"], HOOK_METHOD_ART_CANCEL);
        assert!(HOOK_REQUEST_METHODS
            .iter()
            .all(|method| method.starts_with("loom.hook.")));
        assert!(HOOK_EVENT_METHODS
            .iter()
            .all(|method| method.starts_with("loom.hook.")));
    }

    #[test]
    fn otter_subscriptions_accept_only_versioned_otter_and_surface_events() {
        assert!(otter_subscription_event_supported(
            HOOK_EVENT_WORKFLOW_INSTANTIATED
        ));
        assert!(otter_subscription_event_supported(
            crate::SURFACE_EVENT_SNAPSHOT
        ));
        assert!(crate::SURFACE_EVENT_METHODS
            .iter()
            .all(|method| method.starts_with("loom.surface.")));
        assert!(!otter_subscription_event_supported("surface"));
        assert!(!otter_subscription_event_supported("surface/snapshot"));
    }

    #[test]
    fn otter_art_result_uses_typed_formal_outputs() {
        let commit = OtterArtResultCommit {
            protocol_version: HOOK_PROTOCOL_VERSION.to_owned(),
            request_id: "request:1".to_owned(),
            node_id: "node:1".to_owned(),
            generation: 5,
            result_revision: 2,
            outputs: BTreeMap::from([(
                "result".to_owned(),
                OtterArtPortValue::Value {
                    value: serde_json::json!({ "ok": true }),
                },
            )]),
            candidates: None,
        };

        let encoded = serde_json::to_value(commit).expect("serialize result");
        assert_eq!(encoded["protocolVersion"], HOOK_PROTOCOL_VERSION);
        assert_eq!(encoded["outputs"]["result"]["kind"], "value");
    }

    #[test]
    fn otter_art_execute_requires_explicit_output_transports() {
        let missing_protocol = serde_json::from_value::<OtterRequest>(serde_json::json!({
            "method": HOOK_METHOD_ART_EXECUTE,
            "params": {
                "requestId": "request:1",
                "nodeId": "node:1",
                "artId": "publisher/art",
                "generation": 1,
                "outputTransports": ["websocket"],
                "inputs": {},
                "parameters": {},
                "disabledParameters": []
            }
        }));
        assert!(missing_protocol.is_err());

        let missing = serde_json::from_value::<OtterRequest>(serde_json::json!({
            "method": HOOK_METHOD_ART_EXECUTE,
            "params": {
                "protocolVersion": HOOK_PROTOCOL_VERSION,
                "requestId": "request:1",
                "nodeId": "node:1",
                "artId": "publisher/art",
                "generation": 1,
                "inputs": {},
                "parameters": {},
                "disabledParameters": []
            }
        }));
        assert!(missing.is_err());

        let request = serde_json::from_value::<OtterRequest>(serde_json::json!({
            "method": HOOK_METHOD_ART_EXECUTE,
            "params": {
                "protocolVersion": HOOK_PROTOCOL_VERSION,
                "requestId": "request:1",
                "nodeId": "node:1",
                "artId": "publisher/art",
                "generation": 1,
                "outputTransports": ["websocket"],
                "inputs": {},
                "parameters": {},
                "disabledParameters": []
            }
        }))
        .expect("canonical Art execute request");
        let OtterRequest::ArtExecute(request) = request else {
            panic!("expected Art execute request")
        };
        assert_eq!(
            request.output_transports,
            vec![OtterTransportMode::Websocket]
        );
    }

    #[test]
    fn otter_art_resource_release_uses_canonical_identity() {
        let missing_protocol = serde_json::from_value::<OtterRequest>(serde_json::json!({
            "method": HOOK_METHOD_ART_RESOURCES_RELEASE,
            "params": {
                "requestId": "release:request:1",
                "executionRequestId": "request:1",
                "nodeId": "node:1",
                "generation": 2,
                "deviceId": "device:local",
                "handles": ["Loom_Buffer_1"]
            }
        }));
        assert!(missing_protocol.is_err());

        let missing_execution = serde_json::from_value::<OtterRequest>(serde_json::json!({
            "method": HOOK_METHOD_ART_RESOURCES_RELEASE,
            "params": {
                "protocolVersion": HOOK_PROTOCOL_VERSION,
                "requestId": "release:request:1",
                "nodeId": "node:1",
                "generation": 2,
                "deviceId": "device:local",
                "handles": ["Loom_Buffer_1"]
            }
        }));
        assert!(missing_execution.is_err());

        let request = OtterRequest::ArtResourcesRelease(OtterArtResourcesReleaseRequest {
            protocol_version: HOOK_PROTOCOL_VERSION.to_owned(),
            request_id: "release:request:1".to_owned(),
            execution_request_id: "request:1".to_owned(),
            node_id: "node:1".to_owned(),
            generation: 2,
            device_id: Some("device:local".to_owned()),
            handles: vec!["Loom_Buffer_1".to_owned()],
        });
        let encoded = serde_json::to_value(request).expect("serialize resource release");
        assert_eq!(encoded["method"], HOOK_METHOD_ART_RESOURCES_RELEASE);
        assert_eq!(encoded["params"]["executionRequestId"], "request:1");
        assert_eq!(encoded["params"]["handles"][0], "Loom_Buffer_1");
    }

    #[test]
    fn otter_art_cancel_requires_explicit_protocol_version() {
        let missing_protocol = serde_json::from_value::<OtterRequest>(serde_json::json!({
            "method": HOOK_METHOD_ART_CANCEL,
            "params": {
                "requestId": "request:1",
                "nodeId": "node:1",
                "generation": 2,
                "deviceId": "device:local"
            }
        }));
        assert!(missing_protocol.is_err());
    }
}
