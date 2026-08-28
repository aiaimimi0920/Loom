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
