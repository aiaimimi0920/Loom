use std::collections::BTreeMap;

use serde_json::Value;

use super::*;

fn node(id: &str) -> SurfaceNode {
    SurfaceNode {
        id: id.to_owned(),
        node_type: "text".to_owned(),
        ..SurfaceNode::default()
    }
}

#[test]
fn snapshot_round_trip_preserves_stable_wire_names() {
    let snapshot = SurfaceSnapshot {
        protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
        instance_id: "instance:stock-01".to_owned(),
        attachment_id: "attachment:hook-01".to_owned(),
        art_id: "NA00000000001".to_owned(),
        art_version: "1.2.3".to_owned(),
        revision: 7,
        runtime: SurfaceRuntimeKind::Declarative,
        entry_resource_id: None,
        view_id: Some("full".to_owned()),
        scene: node("root"),
        authoritative_state: serde_json::json!({"price": 221.18}),
        resources: Vec::new(),
        resource_leases: Vec::new(),
    };

    validate_surface_snapshot(&snapshot).expect("valid snapshot");
    let value = serde_json::to_value(&snapshot).expect("serialize snapshot");
    assert_eq!(value["protocolVersion"], SURFACE_PROTOCOL_VERSION);
    assert_eq!(value["instanceId"], "instance:stock-01");
    assert_eq!(value["viewId"], "full");
    assert_eq!(value["scene"]["type"], "text");
    assert_eq!(
        serde_json::from_value::<SurfaceSnapshot>(value).expect("deserialize snapshot"),
        snapshot
    );
}

#[test]
fn duplicate_scene_ids_are_rejected() {
    let mut root = node("root");
    root.children = vec![node("price"), node("price")];
    assert_eq!(
        validate_surface_node_tree(&root),
        Err(SurfaceValidationError::DuplicateNodeId("price".to_owned()))
    );
}

#[test]
fn deep_scene_validation_uses_heap_traversal() {
    let mut root = node("leaf");
    for index in (0..4_096).rev() {
        let mut parent = node(&format!("node-{index}"));
        parent.children.push(root);
        root = parent;
    }

    validate_surface_node_tree(&root).expect("deep scene remains valid");

    // Avoid recursive drop so this test measures the validator rather than Vec's destructor.
    let mut pending = vec![root];
    while let Some(mut current) = pending.pop() {
        pending.append(&mut current.children);
    }
}

#[test]
fn patch_must_advance_revision() {
    let patch = SurfacePatch {
        protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
        instance_id: "instance:one".to_owned(),
        attachment_id: "attachment:one".to_owned(),
        base_revision: 4,
        revision: 4,
        operations: Vec::new(),
        state_patch: Value::Null,
        resources: Vec::new(),
        resource_leases: Vec::new(),
    };
    assert_eq!(
        validate_surface_patch(&patch),
        Err(SurfaceValidationError::InvalidPatchRevision {
            base_revision: 4,
            revision: 4,
        })
    );
}

#[test]
fn surface_action_contract_round_trips_without_runtime_owned_revisions() {
    let invocation = SurfaceActionInvocation {
        protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
        instance_id: "instance:stock".to_owned(),
        attachment_id: "attachment:hook".to_owned(),
        request_id: "request:refresh".to_owned(),
        event_id: "event:refresh".to_owned(),
        action_id: "refresh_price".to_owned(),
        event_class: SurfaceEventClass::Discrete,
        generation: 4,
        base_revision: 9,
        payload: serde_json::json!({"symbol": "MSFT"}),
        authoritative_state: serde_json::json!({"price": 100}),
    };
    validate_surface_action_invocation(&invocation).expect("valid invocation");
    let encoded = serde_json::to_value(&invocation).expect("serialize invocation");
    assert_eq!(encoded["actionId"], "refresh_price");
    assert_eq!(encoded["eventClass"], "discrete");

    let response: SurfaceActionResponse = serde_json::from_value(serde_json::json!({
        "protocolVersion": SURFACE_PROTOCOL_VERSION,
        "patches": [{
            "operations": [{
                "op": "set",
                "nodeId": "price",
                "path": "/props/text",
                "value": "101"
            }],
            "statePatch": {"price": 101}
        }],
        "result": {
            "outputs": {
                "price": {"kind": "value", "value": 101}
            },
            "statePatch": {"price": 101}
        }
    }))
    .expect("deserialize action response");
    assert_eq!(response.patches.len(), 1);
    assert!(response.result.is_some());
    let wire = serde_json::to_value(response).expect("serialize action response");
    assert!(wire.get("resultRevision").is_none());
    assert!(wire["patches"][0].get("revision").is_none());
}

#[test]
fn confirmation_contract_binds_host_device_and_attachment_identity() {
    let request = SurfaceConfirmationRequest {
        protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
        confirmation_id: "confirmation:one".to_owned(),
        instance_id: "instance:one".to_owned(),
        attachment_id: "attachment:one".to_owned(),
        device_id: "device-000-local".to_owned(),
        hook_node_id: "hook-node:one".to_owned(),
        event_id: "event:one".to_owned(),
        request_id: "request:one".to_owned(),
        action_id: "submit_order".to_owned(),
        risk: SurfaceActionRisk::High,
        expires_at_ms: 42,
        payload: serde_json::json!({"quantity": 1}),
    };
    validate_surface_confirmation_request(&request).expect("valid confirmation request");
    let wire = serde_json::to_value(&request).expect("serialize confirmation request");
    assert_eq!(wire["risk"], "high");
    assert_eq!(wire["deviceId"], "device-000-local");

    let decision = SurfaceConfirmationDecision {
        protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
        confirmation_id: request.confirmation_id,
        instance_id: request.instance_id,
        attachment_id: request.attachment_id,
        device_id: request.device_id,
        approved: true,
    };
    validate_surface_confirmation_decision(&decision).expect("valid confirmation decision");
    assert_eq!(
        serde_json::to_value(decision).expect("serialize confirmation decision")["approved"],
        true
    );
}

#[test]
fn cancellation_contract_binds_request_to_its_device_and_instance() {
    let request = SurfaceActionCancelRequest {
        protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
        instance_id: "instance:one".to_owned(),
        request_id: "request:one".to_owned(),
        device_id: "device-000-local".to_owned(),
    };
    validate_surface_action_cancel_request(&request).expect("valid cancel request");
    let wire = serde_json::to_value(&request).expect("serialize cancel request");
    assert_eq!(wire["instanceId"], "instance:one");
    assert_eq!(wire["requestId"], "request:one");
    assert_eq!(wire["deviceId"], "device-000-local");
    assert!(wire.get("eventId").is_none());

    let mut invalid = request;
    invalid.device_id = "device with spaces".to_owned();
    assert!(validate_surface_action_cancel_request(&invalid).is_err());
}

#[test]
fn patch_operation_uses_camel_case_wire_fields() {
    let operation = SurfacePatchOperation::Set {
        node_id: "price".to_owned(),
        path: "/props/text".to_owned(),
        value: serde_json::json!("101"),
    };
    let value = serde_json::to_value(&operation).expect("serialize operation");
    assert_eq!(value["op"], "set");
    assert_eq!(value["nodeId"], "price");
    assert!(value.get("node_id").is_none());
    assert_eq!(
        serde_json::from_value::<SurfacePatchOperation>(value).expect("deserialize operation"),
        operation
    );
}

#[test]
fn result_commit_is_atomic_and_preview_is_separate() {
    let resource = SurfaceResourceDescriptor {
        resource_id: "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .to_owned(),
        kind: SurfaceResourceKind::Image,
        mime: "image/webp".to_owned(),
        size: 128,
        width: Some(8),
        height: Some(8),
    };
    let preview = SurfacePreviewCommit {
        protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
        instance_id: "instance:one".to_owned(),
        request_id: "request:one".to_owned(),
        generation: 2,
        preview_revision: 3,
        port_id: "preview".to_owned(),
        value: SurfacePortValue::Resource {
            resource: resource.clone(),
        },
    };
    let result = SurfaceResultCommit {
        protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
        instance_id: "instance:one".to_owned(),
        request_id: "request:one".to_owned(),
        generation: 2,
        result_revision: 4,
        outputs: BTreeMap::from([
            (
                "output_image".to_owned(),
                SurfacePortValue::Resource { resource },
            ),
            (
                "output_size".to_owned(),
                SurfacePortValue::Value {
                    value: serde_json::json!(128),
                },
            ),
        ]),
        state_patch: serde_json::json!({"status": "completed"}),
    };

    let preview_json = serde_json::to_value(preview).expect("preview JSON");
    let result_json = serde_json::to_value(result).expect("result JSON");
    assert!(preview_json.get("outputs").is_none());
    assert_eq!(result_json["outputs"]["output_size"]["kind"], "value");
    assert_eq!(result_json["resultRevision"], 4);
}

#[test]
fn content_addressed_resource_requires_sha256() {
    let resource = SurfaceResourceDescriptor {
        resource_id: "file:C:/private/image.png".to_owned(),
        kind: SurfaceResourceKind::Image,
        mime: "image/png".to_owned(),
        size: 1,
        width: None,
        height: None,
    };
    assert!(matches!(
        validate_surface_resource(&resource),
        Err(SurfaceValidationError::InvalidResourceId(_))
    ));
}

#[test]
fn surface_event_requires_safe_wire_identities() {
    let event = SurfaceEvent {
        protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
        instance_id: "instance:one".to_owned(),
        attachment_id: "attachment:one".to_owned(),
        event_id: "event:one".to_owned(),
        node_id: "button".to_owned(),
        event: "click".to_owned(),
        action: Some("refresh".to_owned()),
        class: SurfaceEventClass::Discrete,
        generation: 1,
        base_revision: 2,
        payload: Value::Null,
    };
    validate_surface_event(&event).expect("valid event");
    assert!(matches!(
        validate_surface_event(&SurfaceEvent {
            event_id: "event with spaces".to_owned(),
            ..event
        }),
        Err(SurfaceValidationError::UnsafeIdentifier(_))
    ));
}
