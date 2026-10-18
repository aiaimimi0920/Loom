#[test]
fn cloud_install_capability_requires_explicit_approval_then_runs_real_process() {
    let root = unique_temp_dir("cloud-capability-install");
    fs::create_dir_all(&root).unwrap();
    let package = if let Ok(config_path) =
        std::env::var("LOOM_ASSETLIBRARY_NATIVE_CAPABILITY_FIXTURE")
    {
        let config: Value = serde_json::from_slice(&fs::read(config_path).unwrap()).unwrap();
        let downloaded = asset_library::prepare_trial_install(&config);
        if config["expected"] == "denied" {
            match downloaded {
                Ok(_) => panic!("撤销或下架后不得取得安装包"),
                Err(error) => assert_eq!(error.status, 404),
            }
            assert!(!Path::new(config["downloadRoot"].as_str().unwrap())
                .join("asset-library-downloads")
                .exists());
            remove_test_dir(&root);
            return;
        }
        assert_eq!(config["expected"], "saved");
        let fixture = PathBuf::from(std::env::var("LOOM_CLOUD_SIGNED_FIXTURE_ROOT").unwrap());
        fs::copy(
            fixture.join("capability-trust.json"),
            root.join("plugin-trust.json"),
        )
        .unwrap();
        downloaded.unwrap()
    } else {
        let key = loom_plugin_security::generate_signing_key("cloud-test-key");
        let mut trust = loom_plugin_security::TrustStore::default();
        trust.trust(PublisherTrustRecord {
            publisher_id: "publisher.example".into(),
            key_id: key.key_id.clone(),
            public_key: key.public_key.clone(),
            revoked: false,
        });
        trust.write_atomic(&root.join("plugin-trust.json")).unwrap();
        let archive = capability_api_fixture(&root, &key);
        let receipt = json!({"publicationId":"isolated-test", "sha256":sha256_bytes(&archive), "size":archive.len()});
        asset_library::InstallPackage {
            archive: None,
            kind: "capability".into(),
            bytes: archive,
            download: receipt,
        }
    };
    let runtime = test_daemon_runtime(&root, None);
    let receipt = package.download.clone();
    let (status, body) = install_asset_library_package(
        package,
        &root,
        &runtime.capability_runtime,
        &runtime.capability_resources,
        &runtime.hook_bridge,
        &runtime.tool_registry,
        &runtime.framework_registry,
        &runtime.workflow_store,
        &runtime.mcp_servers,
    )
    .unwrap();
    assert_eq!(status, 200, "{body}");
    let result: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(result["download"], receipt);
    let digest = result["installation"]["package"]["digest"]
        .as_str()
        .unwrap();
    assert_eq!(
        runtime.capability_runtime.generation(),
        0,
        "安装不得自动激活"
    );
    let (status, _) = enable_capability_plugin(
        "publisher.example/api-fixture",
        &json!({"digest":digest}).to_string(),
        &root,
        &runtime.capability_runtime,
    )
    .unwrap();
    assert_ne!(status, 200, "未批准权限不得启用");
    let (status, body) = approve_capability_plugin(
        "publisher.example/api-fixture",
        &json!({"digest":digest,"permissions":["hook.unit.attachments.write","hook.notice.show"]})
            .to_string(),
        &root,
    )
    .unwrap();
    assert_eq!(status, 200, "{body}");
    let (status, body) = enable_capability_plugin(
        "publisher.example/api-fixture",
        &json!({"digest":digest}).to_string(),
        &root,
        &runtime.capability_runtime,
    )
    .unwrap();
    assert_eq!(status, 200, "{body}");
    let invoked = expect_json_text_route_response(route_request(&runtime, &parsed_request("POST", "/v1/invoke", &[], Some(&json!({
        "requestId":"cloud-installed-command", "caller":"hook", "capability":"publisher.example/api-fixture.run", "input":{"text":"hello"}
    }).to_string()))), 200);
    assert_eq!(invoked["status"], "succeeded", "{invoked}");
    assert_eq!(invoked["output"]["output"]["ok"], true);
    runtime
        .capability_runtime
        .deactivate("publisher.example/api-fixture")
        .unwrap();
    drop(runtime);
    remove_test_dir(&root);
}

#[test]
#[ignore = "explicitly export a signed executable fixture for the cross-repository local trial"]
fn cloud_generate_capability_trial_fixture() {
    let output = PathBuf::from(
        std::env::var("LOOM_CLOUD_SIGNED_FIXTURE_ROOT").expect("fixture output root"),
    );
    assert!(!output.join("capability-trust.json").exists());
    let root = unique_temp_dir("cloud-capability-export");
    fs::create_dir_all(&root).unwrap();
    let key = loom_plugin_security::generate_signing_key("cloud-export-test-key");
    let archive = capability_api_fixture(&root, &key);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output.join("capability.zip"))
        .unwrap();
    file.write_all(&archive).unwrap();
    file.sync_all().unwrap();
    let mut trust = loom_plugin_security::TrustStore::default();
    trust.trust(PublisherTrustRecord {
        publisher_id: "publisher.example".into(),
        key_id: key.key_id,
        public_key: key.public_key,
        revoked: false,
    });
    trust
        .write_atomic(&output.join("capability-trust.json"))
        .unwrap();
    println!(
        "exported signed executable fixture: {} bytes",
        archive.len()
    );
    remove_test_dir(&root);
}

#[test]
fn cloud_install_never_uses_bundled_art_trust_and_requires_admin() {
    let root = unique_temp_dir("cloud-art-trust-boundary");
    let zip = art_package_zip("cloud-art", "1.0.0", b"fixture");
    let mut config = DaemonConfig::localhost(0)
        .with_bundled_art_sha256_allowlist([sha256_bytes(&zip)])
        .unwrap();
    config.auth_token = Some("isolated-admin-token".into());
    let runtime = test_daemon_runtime_from_config(&root, config);
    let key = loom_plugin_security::generate_signing_key("framework-key");
    let mut trust = loom_plugin_security::TrustStore::default();
    trust.set_policy(TrustPolicy::RequireTrusted);
    trust.trust(PublisherTrustRecord {
        publisher_id: "publisher.test".into(),
        key_id: key.key_id.clone(),
        public_key: key.public_key.clone(),
        revoked: false,
    });
    trust.write_atomic(&root.join("plugin-trust.json")).unwrap();
    runtime
        .framework_registry
        .install_framework_package_from_zip(&signed_framework_package_zip("process", "1.0.0", &key))
        .unwrap();
    let (status, body) = install_asset_library_package(
        asset_library::InstallPackage {
            archive: None,
            kind: "art".into(),
            bytes: zip,
            download: json!({}),
        },
        &root,
        &runtime.capability_runtime,
        &runtime.capability_resources,
        &runtime.hook_bridge,
        &runtime.tool_registry,
        &runtime.framework_registry,
        &runtime.workflow_store,
        &runtime.mcp_servers,
    )
    .unwrap();
    assert_eq!(status, 400, "{body}");
    assert!(body.contains("asset_library_host_install_rejected"));
    assert!(runtime
        .tool_registry
        .get_tool("cloud-art")
        .unwrap()
        .is_none());
    let request = parsed_request("POST", "/v1/asset-library/install", &[], Some("{}"));
    let (status, _) = route_with_runtime(&runtime, &request).unwrap();
    assert!(matches!(status, 401 | 403), "未认证请求不得进入云下载");
    drop(runtime);
    remove_test_dir(&root);
}
