// Capability Plugin lifecycle API coverage.
#[test]
fn capability_plugin_api_installs_configures_enables_and_uninstalls() {
    let root = unique_temp_dir("capability-api");
    fs::create_dir_all(&root).expect("control root");
    let key = loom_plugin_security::generate_signing_key("release-1");
    let mut trust = loom_plugin_security::TrustStore::default();
    trust.trust(PublisherTrustRecord {
        publisher_id: "publisher.example".to_owned(),
        key_id: key.key_id.clone(),
        public_key: key.public_key.clone(),
        revoked: false,
    });
    trust
        .write_atomic(&root.join("plugin-trust.json"))
        .expect("trust store");
    let archive = capability_api_fixture(&root, &key);
    let runtime = Arc::new(CapabilityRuntimeHost::new(RuntimeHostLimits::default()));
    let body = json!({
        "zipBase64": format!(
            "data:application/zip;base64,{}",
            BASE64.encode(&archive)
        )
    })
    .to_string();

    let (status, installed) =
        install_capability_plugin(&body, &root).expect("install response");
    assert_eq!(status, 200, "{installed}");
    let installed: Value = serde_json::from_str(&installed).expect("install JSON");
    let digest = installed["package"]["digest"]
        .as_str()
        .expect("package digest");
    assert_eq!(
        installed["package"]["qualifiedId"],
        "publisher.example/api-fixture"
    );
    loom_tool_registry::capability::CapabilityPluginRegistry::new(&root)
        .verify_installed_version("publisher.example/api-fixture", digest)
        .expect("installed package remains verifiable");

    let (status, enabled) = enable_capability_plugin(
        "publisher.example/api-fixture",
        &json!({ "digest": digest }).to_string(),
        &root,
        &runtime,
    )
    .expect("enable response");
    assert_eq!(status, 200, "{enabled}");
    assert_eq!(
        serde_json::from_str::<Value>(&enabled).unwrap()["plugin"]["status"],
        "active"
    );
    let (status, snapshot) = capability_extension_snapshot(&runtime).expect("extension snapshot");
    assert_eq!(status, 200);
    let snapshot: Value = serde_json::from_str(&snapshot).unwrap();
    assert_eq!(snapshot["snapshot"]["generation"], 1);
    assert_eq!(
        snapshot["snapshot"]["plugins"][0]["id"],
        "publisher.example/api-fixture"
    );
    assert_eq!(
        snapshot["snapshot"]["contributions"]["commands"][0]["id"],
        "publisher.example/api-fixture.run"
    );

    let (status, config) = update_capability_config(
        "publisher.example/api-fixture",
        &json!({
            "expectedRevision": 0,
            "values": { "language": "zh-CN" }
        })
        .to_string(),
        &root,
    )
    .expect("config response");
    assert_eq!(status, 200);
    assert_eq!(
        serde_json::from_str::<Value>(&config).unwrap()["config"]["revision"],
        1
    );

    let (status, listed) = list_capability_plugins(&root).expect("list response");
    assert_eq!(status, 200);
    assert_eq!(
        serde_json::from_str::<Value>(&listed).unwrap()["plugins"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    disable_capability_plugin("publisher.example/api-fixture", &root, &runtime).expect("disable");
    let (status, _) = uninstall_capability_plugin(
        "publisher.example/api-fixture",
        &root,
        &runtime,
    )
    .expect("uninstall");
    assert_eq!(status, 200);
    assert!(
        loom_tool_registry::capability::CapabilityPluginRegistry::new(&root)
            .list()
            .expect("registry")
            .is_empty()
    );
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn capability_plugin_api_rejects_unknown_fields_and_routes_only_its_namespace() {
    let root = unique_temp_dir("capability-api-invalid");
    let runtime = Arc::new(CapabilityRuntimeHost::new(RuntimeHostLimits::default()));
    let (status, body) = install_capability_plugin(
        r#"{"zipBase64":"bad","unexpected":true}"#,
        &root,
    )
    .expect("error response");
    assert_eq!(status, 400);
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["error"]["code"],
        "invalid_capability_install"
    );
    let request = ParsedHttpRequest {
        method: "GET".to_owned(),
        path: "/v1/unrelated".to_owned(),
        headers: Vec::new(),
        body: String::new(),
    };
    assert!(route_capability_plugins(&request, "/v1/unrelated", &root, &runtime).is_none());
}

fn capability_api_fixture(
    root: &Path,
    key: &loom_plugin_security::SigningKeyDocument,
) -> Vec<u8> {
    let package = root.join("api-fixture-package");
    fs::create_dir_all(&package).expect("package dirs");
    fs::write(package.join("ui.surface.json"), b"{}\n").expect("Surface manifest");
    let manifest = json!({
        "schemaVersion": 1,
        "kind": "capability",
        "id": "api-fixture",
        "name": "API Fixture",
        "description": "Lifecycle API fixture",
        "version": "1.0.0",
        "publisher": { "id": "publisher.example", "keyId": key.key_id },
        "hostCompatibility": {
            "loomCapabilityApi": { "minimum": "1.0" },
            "hookExtensionApi": { "minimum": "1.0" }
        },
        "entrypoints": {
            "hookUi": { "kind": "surface", "manifest": "ui.surface.json" }
        },
        "contributes": {
            "commands": [{
                "id": "publisher.example/api-fixture.run",
                "title": "Run fixture"
            }]
        },
        "permissions": [],
        "resources": { "memoryMiB": 64, "maxProcesses": 1, "timeoutSeconds": 10 },
        "dependencies": [],
        "signature": {
            "algorithm": "ed25519",
            "keyId": key.key_id,
            "file": "signature.json"
        }
    });
    fs::write(
        package.join("capability.manifest.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .expect("manifest");
    loom_plugin_security::sign_package(&package, "signature.json", key).expect("sign fixture");
    let mut bytes = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut bytes));
        let options = zip::write::SimpleFileOptions::default();
        for relative in [
            "capability.manifest.json",
            "ui.surface.json",
            "signature.json",
        ] {
            writer.start_file(relative, options).expect("zip entry");
            writer
                .write_all(&fs::read(package.join(relative)).expect("package file"))
                .expect("zip content");
        }
        writer.finish().expect("finish zip");
    }
    bytes
}
