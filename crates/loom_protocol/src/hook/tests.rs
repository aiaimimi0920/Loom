//! Contract coverage for the stable Loom-to-Otter control protocol.

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
