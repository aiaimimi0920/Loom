use serde_json::Value;

use super::*;

fn manifest() -> FrameworkPackageManifest {
    FrameworkPackageManifest {
        id: "example-framework".to_owned(),
        name: "Example".to_owned(),
        description: "Example framework".to_owned(),
        version: "1.2.3".to_owned(),
        protocol_version: FRAMEWORK_PROTOCOL_VERSION.to_owned(),
        supported_protocol_versions: Vec::new(),
        platforms: vec!["windows-x64".to_owned()],
        entry: FrameworkRuntimeEntry {
            kind: "process".to_owned(),
            command: "runtime/framework.exe".to_owned(),
            args: Vec::new(),
            process_model: "per_execution".to_owned(),
        },
        permissions: Vec::new(),
        permission_policy: PermissionPolicy::default(),
        resources: ResourceLimits::default(),
        publisher: PublisherIdentity {
            id: "example.vendor".to_owned(),
            ..PublisherIdentity::default()
        },
        signature: None,
        host_compatibility: HostCompatibility {
            minimum: Some(">=0.1.0".to_owned()),
            maximum: None,
        },
        health_check: None,
        authoring_schema: None,
        dependencies: Vec::new(),
        art_execution: FrameworkArtExecutionContract::default(),
    }
}

#[test]
fn v1_manifest_contract_is_accepted() {
    assert_eq!(validate_framework_manifest_contract(&manifest()), Ok(()));
}

#[test]
fn framework_response_requires_canonical_success_status() {
    assert!(response_status_is_success("success"));
    assert!(!response_status_is_success("ok"));
    assert!(!response_status_is_success("completed"));
}

#[test]
fn supported_protocol_versions_can_negotiate_v1() {
    let mut manifest = manifest();
    manifest.protocol_version = "loom.framework.v2".to_owned();
    manifest.supported_protocol_versions = vec![FRAMEWORK_PROTOCOL_VERSION.to_owned()];
    assert_eq!(
        negotiate_framework_protocol(&manifest),
        Ok(FRAMEWORK_PROTOCOL_VERSION)
    );
}

#[test]
fn advertised_protocol_versions_are_deduplicated_in_first_seen_order() {
    let mut manifest = manifest();
    manifest.protocol_version = "loom.framework.v2".to_owned();
    manifest.supported_protocol_versions = vec![
        FRAMEWORK_PROTOCOL_VERSION.to_owned(),
        "loom.framework.v2".to_owned(),
        "loom.framework.v3".to_owned(),
        FRAMEWORK_PROTOCOL_VERSION.to_owned(),
    ];

    assert_eq!(
        manifest.advertised_protocol_versions(),
        vec![
            "loom.framework.v2",
            FRAMEWORK_PROTOCOL_VERSION,
            "loom.framework.v3"
        ]
    );
}

#[test]
fn package_ids_reject_windows_aliases_and_traversal() {
    assert!(is_safe_package_id("custom-image-search"));
    assert!(is_safe_package_id("core.image.pixelate"));
    assert!(!is_safe_package_id(" art"));
    assert!(!is_safe_package_id("art."));
    assert!(!is_safe_package_id(".art"));
    assert!(!is_safe_package_id("a..b"));
    assert!(!is_safe_package_id("CON"));
    assert!(!is_safe_package_id("com1"));
    assert!(!is_safe_package_id("../escape"));
    assert!(!is_safe_publisher_id("Pub."));
}

#[test]
fn invalid_publisher_and_semver_are_rejected() {
    let mut invalid = manifest();
    invalid.publisher.id = "../vendor".to_owned();
    assert!(matches!(
        validate_framework_manifest_contract(&invalid),
        Err(ProtocolValidationError::UnsafePublisherId(_))
    ));

    let mut invalid = manifest();
    invalid.version = "latest".to_owned();
    assert!(matches!(
        validate_framework_manifest_contract(&invalid),
        Err(ProtocolValidationError::InvalidVersion { .. })
    ));
}

#[test]
fn resource_limits_use_manifest_mib_spelling() {
    let limits: ResourceLimits = serde_json::from_value(serde_json::json!({
        "timeoutSeconds": 120,
        "memoryMiB": 512,
        "maxProcesses": 4,
        "stdoutMiB": 64,
        "stderrMiB": 8
    }))
    .expect("resource limits");
    assert_eq!(limits.stdout_mib, Some(64));
    assert_eq!(limits.stderr_mib, Some(8));
    assert_eq!(limits.memory_mib, Some(512));
    let serialized = serde_json::to_value(limits).expect("serialize resource limits");
    assert_eq!(serialized["stdoutMiB"], 64);
    assert_eq!(serialized["stderrMiB"], 8);
    assert_eq!(serialized["memoryMiB"], 512);
}

/// The surface stream envelope is the one Hook parses without going through a Rust type, so the
/// schema and the constant have to agree. Hook also rejects an envelope with no
/// `protocolVersion` instead of treating it as an older daemon, which is why the field stays
/// required here rather than optional.
#[test]
fn surface_stream_schema_pins_the_protocol_version_constant() {
    let schema: Value = serde_json::from_str(schemas::SURFACE_STREAM_V1).expect("schema JSON");
    assert_eq!(
        schema["properties"]["protocolVersion"]["const"],
        Value::String(surface::SURFACE_STREAM_PROTOCOL_VERSION.to_owned())
    );
    let required = schema["required"]
        .as_array()
        .expect("required list")
        .iter()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    for field in ["protocolVersion", "next", "reset", "messages"] {
        assert!(required.contains(&field), "`{field}` must stay required");
    }
}
