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
fn package_ids_cannot_nest_so_one_package_cannot_claim_another_namespace() {
    // Everything a package owns is matched by the prefix `"{publisher}/{package}."`. A dotted
    // package id makes those prefixes nest — `publisher.example/text-tools.` is a prefix of every
    // id belonging to a package called `text-tools.extra` — which would let one package mint
    // contribution, attachment, data type and resource ids inside another's namespace.
    let mut value = manifest_value();
    value["id"] = json!("text-tools.extra");
    value["contributes"]["commands"][0]["id"] =
        json!("publisher.example/text-tools.extra.transform");
    assert!(matches!(
        parse_capability_manifest(value.to_string().as_bytes()),
        Err(CapabilityValidationError::UnsafeId {
            field: "package id",
            ..
        })
    ));

    // A dependency names a package the same way, so it carries the same restriction.
    let mut value = manifest_value();
    value["dependencies"] =
        json!([{ "id": "publisher.example/text-tools.extra", "version": "1.0.0" }]);
    assert!(matches!(
        parse_capability_manifest(value.to_string().as_bytes()),
        Err(CapabilityValidationError::InvalidDependency(id)) if id == "publisher.example/text-tools.extra"
    ));

    // Publishers keep reverse-DNS dots: `/` belongs to no id alphabet, so they cannot nest.
    let mut value = manifest_value();
    value["publisher"]["id"] = json!("publisher.example.team");
    value["activationEvents"] = json!(["onCommand:publisher.example.team/text-tools.transform"]);
    value["contributes"]["commands"][0]["id"] =
        json!("publisher.example.team/text-tools.transform");
    assert!(parse_capability_manifest(value.to_string().as_bytes()).is_ok());
}

#[test]
fn capability_id_rules_are_exported_so_the_catalog_cannot_advertise_uninstallable_packages() {
    // The signed catalog decides whether a package is installable before anything is downloaded,
    // so it has to apply the manifest's rule rather than the package-wide one. `is_safe_package_id`
    // allows dots because framework and art ids are dotted by convention; capability package ids
    // cannot be, or their namespaces nest.
    assert!(crate::is_safe_package_id("core.image.pixelate"));
    assert!(!is_safe_capability_package_id("core.image.pixelate"));
    assert!(is_safe_capability_package_id("text-tools"));
    assert!(is_safe_capability_publisher_id("publisher.example"));

    // Both halves become directory components under the packages root.
    for reserved in ["con", "nul", "aux", "com1", "lpt9", "con.example"] {
        assert!(
            !is_safe_capability_publisher_id(reserved),
            "{reserved} names a Windows device"
        );
    }
    let mut value = manifest_value();
    value["publisher"]["id"] = json!("nul");
    assert!(matches!(
        parse_capability_manifest(value.to_string().as_bytes()),
        Err(CapabilityValidationError::UnsafeId {
            field: "publisher id",
            ..
        })
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
fn sandbox_budgets_are_bounded_so_a_package_cannot_opt_out_of_containment() {
    // Each of these is applied verbatim by the runtime: an unbounded `memoryMiB` overflows the
    // byte conversion and removes the memory cap, and an unbounded timeout pins an invocation
    // slot indefinitely.
    for (pointer, value) in [
        ("memoryMiB", json!(MAX_CAPABILITY_MEMORY_MIB + 1)),
        ("memoryMiB", json!(MIN_CAPABILITY_MEMORY_MIB - 1)),
        ("maxProcesses", json!(MAX_CAPABILITY_PROCESSES + 1)),
        ("maxProcesses", json!(0)),
        ("timeoutSeconds", json!(MAX_CAPABILITY_TIMEOUT_SECONDS + 1)),
        ("timeoutSeconds", json!(0)),
        ("diskMiB", json!(MAX_CAPABILITY_DISK_MIB + 1)),
        (
            "stderrKiBPerMinute",
            json!(MAX_CAPABILITY_STDERR_KIB_PER_MINUTE + 1),
        ),
    ] {
        let mut manifest = manifest_value();
        manifest["resources"][pointer] = value.clone();
        assert!(
            matches!(
                parse_capability_manifest(manifest.to_string().as_bytes()),
                Err(CapabilityValidationError::InvalidResourceLimit { field }) if field == pointer
            ),
            "resources.{pointer} = {value} must be rejected"
        );
    }

    // A per-command timeout replaces the package-wide one, so it carries the same ceiling.
    let mut manifest = manifest_value();
    manifest["contributes"]["commands"][0]["timeoutMs"] =
        json!(MAX_CAPABILITY_TIMEOUT_SECONDS * 1_000 + 1);
    assert!(matches!(
        parse_capability_manifest(manifest.to_string().as_bytes()),
        Err(CapabilityValidationError::InvalidResourceLimit {
            field: "command timeoutMs"
        })
    ));

    let mut manifest = manifest_value();
    manifest["resources"]["memoryMiB"] = json!(MAX_CAPABILITY_MEMORY_MIB);
    manifest["resources"]["timeoutSeconds"] = json!(MAX_CAPABILITY_TIMEOUT_SECONDS);
    // The published schema spells these two with the `MiB`/`KiB` casing; `deny_unknown_fields`
    // turns any disagreement between it and the Rust type into a load failure.
    manifest["resources"]["diskMiB"] = json!(MAX_CAPABILITY_DISK_MIB);
    manifest["resources"]["stderrKiBPerMinute"] = json!(MAX_CAPABILITY_STDERR_KIB_PER_MINUTE);
    manifest["contributes"]["commands"][0]["timeoutMs"] =
        json!(MAX_CAPABILITY_TIMEOUT_SECONDS * 1_000);
    let parsed =
        parse_capability_manifest(manifest.to_string().as_bytes()).expect("budgets at the ceiling");
    assert_eq!(parsed.resources.disk_mib, Some(MAX_CAPABILITY_DISK_MIB));
    assert_eq!(
        parsed.resources.stderr_kib_per_minute,
        Some(MAX_CAPABILITY_STDERR_KIB_PER_MINUTE)
    );
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
fn legacy_resource_limit_casing_loads_but_serializes_canonically() {
    let mut manifest = manifest_value();
    manifest["resources"]["diskMib"] = json!(512);
    manifest["resources"]["stderrKibPerMinute"] = json!(256);

    let parsed = parse_capability_manifest(manifest.to_string().as_bytes())
        .expect("legacy released manifest must remain loadable");
    assert_eq!(parsed.resources.disk_mib, Some(512));
    assert_eq!(parsed.resources.stderr_kib_per_minute, Some(256));

    let serialized = serde_json::to_value(parsed.resources).expect("serialize resources");
    assert_eq!(serialized["diskMiB"], json!(512));
    assert_eq!(serialized["stderrKiBPerMinute"], json!(256));
    assert!(serialized.get("diskMib").is_none());
    assert!(serialized.get("stderrKibPerMinute").is_none());
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
fn extension_effect_payload_budget_is_measured_exactly_at_the_boundary() {
    // The budget is enforced by a counting sink rather than a materialised encoding, so the
    // boundary is pinned here: `{"blob":"<value>"}` serializes to `value.len() + 11` bytes.
    let effect = |blob_len: usize| {
        json!({
            "protocol": crate::EXTENSION_PROTOCOL,
            "apiVersion": "1.0",
            "requestId": "request-1",
            "status": "succeeded",
            "effects": [{
                "type": "attachment.upsert",
                "payload": {
                    "attachmentId": "result",
                    "typeId": "publisher.example/demo.result.v1",
                    "schemaVersion": "1.0",
                    "priorRevision": 0,
                    "revision": 1,
                    "payload": { "blob": "a".repeat(blob_len) }
                }
            }]
        })
        .to_string()
    };

    const MAXIMUM: usize = 256 * 1024;
    assert!(parse_extension_message(effect(MAXIMUM - 11).as_bytes()).is_ok());
    assert_eq!(
        parse_extension_message(effect(MAXIMUM - 10).as_bytes()),
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

#[test]
fn extension_invocation_resources_are_unique_content_addressed_and_aggregate_bounded() {
    let resource = |marker: char, byte_length: u64, lease_id: &str| {
        let digest = marker.to_string().repeat(64);
        json!({
            "resourceId": format!("sha256:{digest}"),
            "kind": "file",
            "digest": digest,
            "byteLength": byte_length,
            "leaseId": lease_id
        })
    };
    let invocation = |resources: Value| {
        json!({
            "protocol": crate::EXTENSION_PROTOCOL,
            "apiVersion": "1.0",
            "requestId": "request-1",
            "pluginId": "publisher.example/plugin",
            "commandId": "publisher.example/plugin.run",
            "snapshotGeneration": 1,
            "target": { "unitId": "unit-1", "revision": 2 },
            "input": {},
            "resourceRefs": resources
        })
        .to_string()
    };

    let maximum = MAX_EXTENSION_INVOCATION_RESOURCE_BYTES;
    let valid = invocation(json!([
        resource('a', maximum / 2, "lease-1"),
        resource('b', maximum / 2, "lease-2")
    ]));
    assert!(parse_extension_message(valid.as_bytes()).is_ok());

    for invalid in [
        json!([resource('a', 1, "lease-1"), resource('a', 1, "lease-2")]),
        json!([resource('a', 1, "lease-1"), resource('b', 1, "lease-1")]),
        json!([
            resource('a', maximum / 2, "lease-1"),
            resource('b', maximum / 2 + 1, "lease-2")
        ]),
    ] {
        assert_eq!(
            parse_extension_message(invocation(invalid).as_bytes()),
            Err(ExtensionValidationError::InvalidInvocation)
        );
    }

    let mut inline = resource('a', 1, "lease-1");
    inline["kind"] = json!("inline");
    assert_eq!(
        parse_extension_message(invocation(json!([inline])).as_bytes()),
        Err(ExtensionValidationError::InvalidInvocation)
    );
}
