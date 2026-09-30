#[test]
fn capability_catalog_cli_signs_and_validates_canonical_payload() {
    use chrono::Duration;
    use loom_protocol::{
        CapabilityApiRequirement, CapabilityHostCompatibility, CapabilityPublisher,
    };
    use loom_tool_registry::capability::{
        CapabilityCatalogArtifact, CapabilityCatalogEntry, CapabilityCatalogPackage,
        CapabilityCatalogPackageSignature, CapabilityCatalogPayload,
        CAPABILITY_CATALOG_SCHEMA_VERSION,
    };

    let root = temp_root("catalog-sign");
    let key_path = root.join("official-key.json");
    let trust_path = root.join("plugin-trust.json");
    let payload_path = root.join("catalog-payload.json");
    let catalog_path = root.join("catalog.json");
    let key = generate_signing_key("official-capability-1");
    write_signing_key_document(&key_path, &key).expect("write key");
    let requirement = || CapabilityApiRequirement {
        minimum: "1.0".to_owned(),
        maximum: None,
        required_features: Vec::new(),
        optional_features: Vec::new(),
    };
    let publisher = CapabilityPublisher {
        id: "neuro.official".to_owned(),
        key_id: key.key_id.clone(),
    };
    let artifact = |name: &str| CapabilityCatalogArtifact {
        url: format!("https://downloads.example/{name}"),
        sha256: "a".repeat(64),
        bytes: 1,
    };
    let now = chrono::Utc::now();
    let payload = CapabilityCatalogPayload {
        schema_version: CAPABILITY_CATALOG_SCHEMA_VERSION,
        publisher: publisher.clone(),
        generated_at: now.to_rfc3339(),
        expires_at: (now + Duration::hours(1)).to_rfc3339(),
        packages: vec![CapabilityCatalogEntry {
            qualified_id: "neuro.official/ocr".to_owned(),
            name: "OCR".to_owned(),
            description: "OCR and code recognition".to_owned(),
            version: "1.1.0".to_owned(),
            publisher: publisher.clone(),
            package: CapabilityCatalogPackage {
                url: "https://downloads.example/ocr.zip".to_owned(),
                sha256: "b".repeat(64),
                bytes: 1,
                signature: CapabilityCatalogPackageSignature {
                    algorithm: "ed25519".to_owned(),
                    key_id: key.key_id.clone(),
                },
            },
            sbom: artifact("ocr.cdx.json"),
            provenance: artifact("ocr.provenance.json"),
            host_compatibility: CapabilityHostCompatibility {
                loom_capability_api: requirement(),
                hook_extension_api: requirement(),
                surface_api: None,
            },
            permissions: vec!["hook.notice.show".to_owned()],
            disk_bytes: 2,
        }],
    };
    write_pretty_json(
        payload_path.clone(),
        &serde_json::to_value(payload).expect("payload JSON"),
    )
    .expect("write payload");
    trust_publisher(&trust_path, "neuro.official", &key_path).expect("trust key");

    let signed = sign_capability_catalog(&payload_path, &key_path, &catalog_path)
        .expect("sign catalog");
    assert!(signed.contains("packages=1"));
    let validated = validate_capability_catalog(&catalog_path, &trust_path)
        .expect("validate catalog");
    assert!(validated.contains("publisher=neuro.official"));

    fs::remove_dir_all(root).ok();
}

#[test]
fn capability_catalog_cli_rejects_non_official_publishers() {
    let root = temp_root("catalog-publisher");
    let key_path = root.join("key.json");
    let payload_path = root.join("payload.json");
    let output_path = root.join("catalog.json");
    let key = generate_signing_key("catalog-key");
    write_signing_key_document(&key_path, &key).expect("write key");
    fs::write(
        &payload_path,
        format!(
            "{{\"schemaVersion\":1,\"publisher\":{{\"id\":\"publisher.example\",\"keyId\":\"{}\"}},\"generatedAt\":\"2020-01-01T00:00:00Z\",\"expiresAt\":\"2099-01-01T00:00:00Z\",\"packages\":[]}}",
            key.key_id
        ),
    )
    .expect("write payload");

    let error = sign_capability_catalog(&payload_path, &key_path, &output_path)
        .expect_err("non-official publisher must fail");
    assert!(error.to_string().contains("neuro.official"));
    assert!(!output_path.exists());
    fs::remove_dir_all(root).ok();
}
