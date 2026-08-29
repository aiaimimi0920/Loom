use serde_json::{json, Value};

use super::*;

const MINIMAL_MANIFEST: &str =
    include_str!("../../../../protocol/examples/capability/minimal-capability.manifest.json");
const INITIALIZE_REQUEST: &str =
    include_str!("../../../../protocol/examples/capability/runtime-initialize.request.json");
const EXTENSION_SNAPSHOT: &str =
    include_str!("../../../../protocol/examples/capability/extension-snapshot.json");

fn manifest_value() -> Value {
    serde_json::from_str(MINIMAL_MANIFEST).expect("fixture JSON")
}

#[test]
fn canonical_capability_fixture_is_accepted() {
    let manifest = parse_capability_manifest(MINIMAL_MANIFEST.as_bytes()).expect("manifest");
    assert_eq!(manifest.qualified_id(), "publisher.example/text-tools");
    assert_eq!(manifest.contributes.commands.len(), 1);
}

#[test]
fn package_kind_confusion_and_unknown_fields_are_rejected() {
    let mut value = manifest_value();
    value["kind"] = json!("framework");
    assert_eq!(
        parse_capability_manifest(value.to_string().as_bytes()),
        Err(CapabilityValidationError::InvalidKind)
    );

    let mut value = manifest_value();
    value["unexpected"] = json!(true);
    assert!(matches!(
        parse_capability_manifest(value.to_string().as_bytes()),
        Err(CapabilityValidationError::InvalidJson(_))
    ));
}

#[test]
fn entrypoint_paths_are_package_relative() {
    for unsafe_path in [
        "../escape.exe",
        "C:/Windows/System32/cmd.exe",
        "/tmp/plugin",
    ] {
        let mut value = manifest_value();
        value["entrypoints"]["service"]["targets"]["windows-x64"]["command"] = json!(unsafe_path);
        assert!(matches!(
            parse_capability_manifest(value.to_string().as_bytes()),
            Err(CapabilityValidationError::UnsafePath(_))
        ));
    }
}

#[test]
fn contribution_ids_are_owned_and_unique_case_insensitively() {
    let mut value = manifest_value();
    value["contributes"]["menus"] = json!([{
        "id": "other.publisher/plugin.menu",
        "payload": {}
    }]);
    assert!(matches!(
        parse_capability_manifest(value.to_string().as_bytes()),
        Err(CapabilityValidationError::InvalidNamespace(_))
    ));

    let mut value = manifest_value();
    value["contributes"]["menus"] = json!([{
        "id": "publisher.example/text-tools.transform",
        "payload": {}
    }]);
    assert!(matches!(
        parse_capability_manifest(value.to_string().as_bytes()),
        Err(CapabilityValidationError::DuplicateContribution(_))
    ));
}

#[test]
fn settings_use_bounded_manifest_driven_field_definitions() {
    let mut value = manifest_value();
    value["contributes"]["settings"] = json!([{
        "id": "publisher.example/text-tools.mode",
        "title": "Transform mode",
        "payload": {
            "type": "enum",
            "options": ["safe", "fast"],
            "default": "safe"
        }
    }]);
    let parsed = parse_capability_manifest(value.to_string().as_bytes()).expect("setting");
    assert_eq!(parsed.contributes.settings.len(), 1);

    value["contributes"]["settings"][0]["payload"]["default"] = json!("undeclared");
    assert!(matches!(
        parse_capability_manifest(value.to_string().as_bytes()),
        Err(CapabilityValidationError::InvalidSetting(_))
    ));
}

#[test]
fn api_permissions_and_signature_are_strict() {
    let mut value = manifest_value();
    value["hostCompatibility"]["loomCapabilityApi"]["minimum"] = json!("2.0");
    assert!(matches!(
        parse_capability_manifest(value.to_string().as_bytes()),
        Err(CapabilityValidationError::UnsupportedApi(_))
    ));

    let mut value = manifest_value();
    value["permissions"] = json!(["host.everything"]);
    assert!(matches!(
        parse_capability_manifest(value.to_string().as_bytes()),
        Err(CapabilityValidationError::InvalidPermission(_))
    ));

    let mut value = manifest_value();
    value["signature"]["keyId"] = json!("other-key");
    assert_eq!(
        parse_capability_manifest(value.to_string().as_bytes()),
        Err(CapabilityValidationError::InvalidSignature)
    );
}

#[test]
fn command_permissions_must_be_declared_by_the_package() {
    let mut value = manifest_value();
    value["contributes"]["commands"][0]["permissions"] = json!(["hook.notice.show"]);
    assert!(matches!(
        parse_capability_manifest(value.to_string().as_bytes()),
        Err(CapabilityValidationError::UndeclaredCommandPermission(permission))
            if permission == "hook.notice.show"
    ));
}

#[test]
fn manifest_depth_and_bytes_are_bounded_before_typed_use() {
    let mut nested = json!(null);
    for _ in 0..=MAX_CAPABILITY_JSON_DEPTH {
        nested = json!({ "next": nested });
    }
    let mut value = manifest_value();
    value["contributes"]["menus"] = json!([{
        "id": "publisher.example/text-tools.deep",
        "payload": nested
    }]);
    assert_eq!(
        parse_capability_manifest(value.to_string().as_bytes()),
        Err(CapabilityValidationError::JsonTooDeep)
    );

    let oversized = vec![b' '; MAX_CAPABILITY_MANIFEST_BYTES + 1];
    assert_eq!(
        parse_capability_manifest(&oversized),
        Err(CapabilityValidationError::ManifestTooLarge)
    );
}

#[test]
fn canonical_runtime_fixture_is_strict_and_bounded() {
    let message = parse_capability_runtime_frame(INITIALIZE_REQUEST.as_bytes()).expect("runtime");
    assert!(matches!(
        message,
        CapabilityRuntimeMessage::Request {
            method: CapabilityRuntimeMethod::Initialize,
            ..
        }
    ));

    let mut value: Value = serde_json::from_str(INITIALIZE_REQUEST).expect("fixture");
    value["pluginId"] = json!("spoofed/plugin");
    assert!(matches!(
        parse_capability_runtime_frame(value.to_string().as_bytes()),
        Err(CapabilityRuntimeValidationError::InvalidJson(_))
    ));

    let oversized = vec![b' '; CAPABILITY_RUNTIME_FRAME_BYTES + 1];
    assert_eq!(
        parse_capability_runtime_frame(&oversized),
        Err(CapabilityRuntimeValidationError::FrameTooLarge)
    );
}

#[test]
fn canonical_extension_snapshot_is_atomic_and_scope_bound() {
    let message = parse_extension_message(EXTENSION_SNAPSHOT.as_bytes()).expect("snapshot");
    let ExtensionMessage::Snapshot(snapshot) = message else {
        panic!("expected snapshot");
    };
    assert_eq!(snapshot.generation, 1);
    assert_eq!(snapshot.contributions.commands.len(), 1);

    let mut value: Value = serde_json::from_str(EXTENSION_SNAPSHOT).expect("fixture");
    value["contributions"]["commands"][0]["scopeId"] = json!("other-scope");
    assert!(matches!(
        parse_extension_message(value.to_string().as_bytes()),
        Err(ExtensionValidationError::InvalidScope(_))
    ));

    let mut value: Value = serde_json::from_str(EXTENSION_SNAPSHOT).expect("fixture");
    value["plugins"][0]["packageDigest"] = json!("not-a-digest");
    assert_eq!(
        parse_extension_message(value.to_string().as_bytes()),
        Err(ExtensionValidationError::InvalidDigest)
    );
}

#[test]
fn extension_attachment_effects_are_cas_bound_and_content_addressed() {
    let valid = json!({
        "protocol": crate::EXTENSION_PROTOCOL,
        "apiVersion": "1.0",
        "requestId": "request-1",
        "status": "succeeded",
        "output": { "ok": true },
        "effects": [{
            "type": "attachment.upsert",
            "payload": {
                "attachmentId": "result",
                "typeId": "publisher.example/demo.result.v1",
                "schemaVersion": "1.0",
                "priorRevision": 0,
                "revision": 1,
                "resourceRefs": [{
                    "resourceId": format!("sha256:{}", "a".repeat(64)),
                    "kind": "file",
                    "digest": "a".repeat(64),
                    "byteLength": 4,
                    "leaseId": "lease-1"
                }]
            }
        }]
    });
    assert!(parse_extension_message(valid.to_string().as_bytes()).is_ok());

    let mut stale = valid.clone();
    stale["effects"][0]["payload"]["revision"] = json!(2);
    assert_eq!(
        parse_extension_message(stale.to_string().as_bytes()),
        Err(ExtensionValidationError::InvalidEffect)
    );

    let mut raw_path = valid;
    raw_path["effects"][0]["payload"]["resourceRefs"][0]["resourceId"] =
        json!("file:C:/private.bin");
    assert_eq!(
        parse_extension_message(raw_path.to_string().as_bytes()),
        Err(ExtensionValidationError::InvalidEffect)
    );
}

#[test]
fn extension_handshake_negotiates_optional_features_and_rejects_required_unknowns() {
    let mut request = ExtensionHandshakeRequest {
        request_id: "handshake-1".to_owned(),
        hook_session_id: "hook:session-1".to_owned(),
        protocol: crate::EXTENSION_PROTOCOL.to_owned(),
        api_version: "1.0".to_owned(),
        required_features: vec![EXTENSION_FEATURE_SNAPSHOT.to_owned()],
        optional_features: vec![
            EXTENSION_FEATURE_MENUS.to_owned(),
            "future.optional".to_owned(),
        ],
    };
    assert_eq!(
        negotiate_extension_features(&request).expect("negotiate extension features"),
        vec![
            EXTENSION_FEATURE_SNAPSHOT.to_owned(),
            EXTENSION_FEATURE_MENUS.to_owned(),
        ]
    );

    request.required_features.push("future.required".to_owned());
    assert_eq!(
        negotiate_extension_features(&request),
        Err(ExtensionHandshakeError::UnsupportedFeature(
            "future.required".to_owned()
        ))
    );
}

#[test]
fn extension_resource_uploads_are_explicit_and_backward_compatible() {
    let invocation = json!({
        "protocol": crate::EXTENSION_PROTOCOL,
        "apiVersion": "1.0",
        "requestId": "request-1",
        "pluginId": "publisher.example/plugin",
        "commandId": "publisher.example/plugin.run",
        "snapshotGeneration": 1,
        "target": { "unitId": "unit-1", "revision": 2 },
        "input": {},
        "resourceRefs": [],
        "unitAttachments": []
    });
    let legacy = json!({
        "method": EXTENSION_METHOD_COMMAND_INVOKE,
        "params": { "sessionId": "session-1", "invocation": invocation.clone() }
    });
    let parsed: ExtensionBridgeRequest = serde_json::from_value(legacy).expect("legacy request");
    let ExtensionBridgeRequest::CommandInvoke(request) = parsed else {
        panic!("expected command invocation");
    };
    assert!(request.resource_uploads.is_empty());

    let with_upload = json!({
        "method": EXTENSION_METHOD_COMMAND_INVOKE,
        "params": {
            "sessionId": "session-1",
            "invocation": invocation,
            "resourceUploads": [{
                "kind": "image",
                "mime": "image/png",
                "dataBase64": "AA=="
            }]
        }
    });
    let parsed: ExtensionBridgeRequest =
        serde_json::from_value(with_upload.clone()).expect("upload");
    let ExtensionBridgeRequest::CommandInvoke(request) = parsed else {
        panic!("expected command invocation");
    };
    assert_eq!(request.resource_uploads.len(), 1);

    let mut unknown = with_upload;
    unknown["params"]["resourceUploads"][0]["path"] = json!("C:/private.png");
    assert!(serde_json::from_value::<ExtensionBridgeRequest>(unknown).is_err());
}
