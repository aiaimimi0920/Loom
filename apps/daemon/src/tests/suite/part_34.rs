// Capability Plugin catalog defaults and diagnostics coverage.
#[test]
fn capability_catalog_is_disabled_until_an_official_source_is_configured() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let previous_url = std::env::var(CAPABILITY_CATALOG_URL_ENV).ok();
    let previous_loopback = std::env::var(CAPABILITY_CATALOG_LOOPBACK_ENV).ok();
    std::env::remove_var(CAPABILITY_CATALOG_URL_ENV);
    std::env::remove_var(CAPABILITY_CATALOG_LOOPBACK_ENV);
    let root = unique_temp_dir("capability-catalog-disabled");
    fs::create_dir_all(&root).expect("control root");
    let daemon = test_daemon_runtime(&root, None);

    let (status, body) = list_capability_catalog(&root, &daemon.hook_bridge).expect("catalog response");
    assert_eq!(status, 200);
    let body: Value = serde_json::from_str(&body).expect("catalog JSON");
    assert_eq!(body["configured"], false);
    assert!(body["packages"].as_array().unwrap().is_empty());

    let (status, body) = install_capability_catalog_plugin(
        &json!({ "qualifiedId": "neuro.official/text-tools" }).to_string(),
        &root,
        &daemon.hook_bridge,
    )
    .expect("catalog install response");
    assert_eq!(status, 404);
    assert_eq!(serde_json::from_str::<Value>(&body).unwrap()["error"]["code"], "capability_catalog_not_found");

    drop(daemon);
    fs::remove_dir_all(root).expect("cleanup capability catalog root");
    restore_env(CAPABILITY_CATALOG_URL_ENV, previous_url);
    restore_env(CAPABILITY_CATALOG_LOOPBACK_ENV, previous_loopback);
}

#[test]
fn signed_catalog_route_installs_a_digest_pinned_package() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let previous_url = std::env::var(CAPABILITY_CATALOG_URL_ENV).ok();
    let previous_loopback = std::env::var(CAPABILITY_CATALOG_LOOPBACK_ENV).ok();
    let root = unique_temp_dir("capability-catalog-install");
    fs::create_dir_all(&root).expect("control root");
    let package_key = loom_plugin_security::generate_signing_key("package-1");
    let catalog_key = loom_plugin_security::generate_signing_key("catalog-1");
    let mut trust = loom_plugin_security::TrustStore::default();
    for (publisher_id, key) in [
        ("publisher.example", &package_key),
        ("neuro.official", &catalog_key),
    ] {
        trust.trust(PublisherTrustRecord {
            publisher_id: publisher_id.to_owned(),
            key_id: key.key_id.clone(),
            public_key: key.public_key.clone(),
            revoked: false,
        });
    }
    trust.write_atomic(&root.join("plugin-trust.json")).expect("trust store");

    let package = capability_api_fixture(&root, &package_key);
    let sbom = br#"{"bomFormat":"CycloneDX","specVersion":"1.5"}"#.to_vec();
    let provenance = br#"{"predicateType":"https://slsa.dev/provenance/v1"}"#.to_vec();
    let listener = TcpListener::bind("127.0.0.1:0").expect("catalog listener");
    let base = format!("http://{}", listener.local_addr().unwrap());
    let catalog = capability_catalog_fixture(
        &catalog_key,
        &package_key,
        &base,
        &package,
        &sbom,
        &provenance,
    );
    let bodies = BTreeMap::from([
        ("/catalog.json".to_owned(), catalog),
        ("/package.zip".to_owned(), package),
        ("/package.cdx.json".to_owned(), sbom),
        ("/provenance.json".to_owned(), provenance),
    ]);
    let server = thread::spawn(move || serve_capability_catalog_fixture(listener, bodies, 5));
    std::env::set_var(CAPABILITY_CATALOG_URL_ENV, format!("{base}/catalog.json"));
    std::env::set_var(CAPABILITY_CATALOG_LOOPBACK_ENV, "1");
    let daemon = test_daemon_runtime(&root, None);
    daemon
        .hook_bridge
        .lock()
        .unwrap()
        .extension_capable_clients
        .store(1, Ordering::SeqCst);

    let (status, listed) = list_capability_catalog(&root, &daemon.hook_bridge)
        .expect("signed catalog response");
    assert_eq!(status, 200, "{listed}");
    let listed: Value = serde_json::from_str(&listed).unwrap();
    assert_eq!(listed["configured"], true);
    assert_eq!(listed["packages"][0]["compatible"], true);
    assert_eq!(
        listed["packages"][0]["entry"]["qualifiedId"],
        "publisher.example/api-fixture"
    );

    let (status, installed) = install_capability_catalog_plugin(
        &json!({ "qualifiedId": "publisher.example/api-fixture" }).to_string(),
        &root,
        &daemon.hook_bridge,
    )
    .expect("catalog install response");
    assert_eq!(status, 200, "{installed}");
    let installed: Value = serde_json::from_str(&installed).unwrap();
    assert_eq!(installed["package"]["trustStatus"], "trusted");
    assert_eq!(installed["supplyChainVerified"], true);
    let digest = installed["package"]["digest"].as_str().unwrap();
    loom_tool_registry::capability::CapabilityPluginRegistry::new(&root)
        .verify_installed_version("publisher.example/api-fixture", digest)
        .expect("catalog package remains verifiable");

    drop(daemon);
    server.join().expect("catalog server");
    fs::remove_dir_all(root).expect("cleanup catalog install root");
    restore_env(CAPABILITY_CATALOG_URL_ENV, previous_url);
    restore_env(CAPABILITY_CATALOG_LOOPBACK_ENV, previous_loopback);
}

#[test]
fn capability_list_reports_bounded_installed_disk_usage() {
    let root = unique_temp_dir("capability-disk-usage");
    fs::create_dir_all(&root).expect("control root");
    let key = loom_plugin_security::generate_signing_key("release-1");
    let mut trust = loom_plugin_security::TrustStore::default();
    trust.trust(PublisherTrustRecord {
        publisher_id: "publisher.example".to_owned(),
        key_id: key.key_id.clone(),
        public_key: key.public_key.clone(),
        revoked: false,
    });
    trust.write_atomic(&root.join("plugin-trust.json")).expect("trust store");
    let archive = capability_api_fixture(&root, &key);
    let (status, _) = install_capability_plugin(
        &json!({ "zipBase64": format!("data:application/zip;base64,{}", BASE64.encode(archive)) }).to_string(),
        &root,
    )
    .expect("install response");
    assert_eq!(status, 200);

    let (status, body) = list_capability_plugins(&root).expect("capability list");
    assert_eq!(status, 200);
    let body: Value = serde_json::from_str(&body).expect("capability list JSON");
    assert!(body["diskBytesByPlugin"]["publisher.example/api-fixture"].as_u64().unwrap() > 0);

    fs::remove_dir_all(root).expect("cleanup capability disk root");
}

#[test]
fn enabled_capability_is_restored_after_daemon_restart() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let root = unique_temp_dir("capability-restart");
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
    let (_, installed) = install_capability_plugin(
        &json!({ "zipBase64": format!("data:application/zip;base64,{}", BASE64.encode(archive)) }).to_string(),
        &root,
    )
    .expect("install response");
    let installed: Value = serde_json::from_str(&installed).unwrap();
    let digest = installed["package"]["digest"].as_str().unwrap();
    approve_capability_plugin(
        "publisher.example/api-fixture",
        &json!({
            "digest": digest,
            "permissions": ["hook.unit.attachments.write", "hook.notice.show"]
        })
        .to_string(),
        &root,
    )
    .expect("approve response");
    let first = test_daemon_runtime(&root, None);
    let (status, _) = enable_capability_plugin(
        "publisher.example/api-fixture",
        &json!({ "digest": digest }).to_string(),
        &root,
        &first.capability_runtime,
    )
    .expect("enable response");
    assert_eq!(status, 200);
    drop(first);

    let restarted = build_capability_runtime(&root).expect("rebuild capability runtime");
    let (_, snapshot) = capability_extension_snapshot(&restarted)
        .expect("restarted extension snapshot");
    let snapshot: Value = serde_json::from_str(&snapshot).unwrap();
    assert_eq!(snapshot["snapshot"]["plugins"][0]["id"], "publisher.example/api-fixture");
    restarted.deactivate_all();
    fs::remove_dir_all(root).expect("cleanup capability restart root");
}

#[test]
fn permission_expansion_requires_new_digest_approval_after_restart() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let root = unique_temp_dir("capability-expanded-permissions");
    fs::create_dir_all(&root).expect("control root");
    let key = loom_plugin_security::generate_signing_key("release-1");
    let mut trust = loom_plugin_security::TrustStore::default();
    trust.trust(PublisherTrustRecord {
        publisher_id: "publisher.example".to_owned(),
        key_id: key.key_id.clone(),
        public_key: key.public_key.clone(),
        revoked: false,
    });
    trust.write_atomic(&root.join("plugin-trust.json")).expect("trust store");

    let v1 = capability_api_fixture(&root, &key);
    let (_, installed_v1) = install_capability_plugin(
        &json!({ "zipBase64": format!("data:application/zip;base64,{}", BASE64.encode(v1)) }).to_string(),
        &root,
    )
    .expect("install v1");
    let installed_v1: Value = serde_json::from_str(&installed_v1).unwrap();
    let digest_v1 = installed_v1["package"]["digest"].as_str().unwrap();
    approve_capability_plugin(
        "publisher.example/api-fixture",
        &json!({
            "digest": digest_v1,
            "permissions": ["hook.unit.attachments.write", "hook.notice.show"]
        })
        .to_string(),
        &root,
    )
    .expect("approve v1");
    let first = test_daemon_runtime(&root, None);
    enable_capability_plugin(
        "publisher.example/api-fixture",
        &json!({ "digest": digest_v1 }).to_string(),
        &root,
        &first.capability_runtime,
    )
    .expect("enable v1");
    drop(first);

    let expanded_permissions = [
        "hook.unit.attachments.write",
        "hook.unit.attachments.read",
        "hook.notice.show",
    ];
    let v2 = capability_api_fixture_version(&root, &key, "2.0.0", &expanded_permissions);
    let (_, installed_v2) = install_capability_plugin(
        &json!({ "zipBase64": format!("data:application/zip;base64,{}", BASE64.encode(v2)) }).to_string(),
        &root,
    )
    .expect("install v2");
    let installed_v2: Value = serde_json::from_str(&installed_v2).unwrap();
    let digest_v2 = installed_v2["package"]["digest"].as_str().unwrap();

    let restarted = build_capability_runtime(&root).expect("restart runtime");
    let (status, rejected) = upgrade_capability_plugin(
        "publisher.example/api-fixture",
        &json!({ "digest": digest_v2 }).to_string(),
        &root,
        &restarted,
    )
    .expect("permission rejection response");
    assert_eq!(status, 409);
    assert_eq!(
        serde_json::from_str::<Value>(&rejected).unwrap()["error"]["code"],
        "capability_permission_required"
    );
    let still_v1 = loom_tool_registry::capability::CapabilityPluginRegistry::new(&root)
        .get("publisher.example/api-fixture")
        .unwrap()
        .unwrap();
    assert_eq!(still_v1.active_digest.as_deref(), Some(digest_v1));

    approve_capability_plugin(
        "publisher.example/api-fixture",
        &json!({ "digest": digest_v2, "permissions": expanded_permissions }).to_string(),
        &root,
    )
    .expect("approve v2");
    let (status, upgraded) = upgrade_capability_plugin(
        "publisher.example/api-fixture",
        &json!({ "digest": digest_v2 }).to_string(),
        &root,
        &restarted,
    )
    .expect("upgrade v2");
    assert_eq!(status, 200, "{upgraded}");
    let upgraded: Value = serde_json::from_str(&upgraded).unwrap();
    assert_eq!(upgraded["plugin"]["activeDigest"], digest_v2);
    assert_eq!(upgraded["plugin"]["previousDigest"], digest_v1);

    restarted.deactivate_all();
    fs::remove_dir_all(root).expect("cleanup expanded permissions root");
}

fn capability_catalog_fixture(
    catalog_key: &loom_plugin_security::SigningKeyDocument,
    package_key: &loom_plugin_security::SigningKeyDocument,
    base: &str,
    package: &[u8],
    sbom: &[u8],
    provenance: &[u8],
) -> Vec<u8> {
    use loom_tool_registry::capability::{
        CapabilityCatalogArtifact, CapabilityCatalogDocument, CapabilityCatalogEntry,
        CapabilityCatalogPackage, CapabilityCatalogPackageSignature, CapabilityCatalogPayload,
        CapabilityCatalogSignature, CAPABILITY_CATALOG_SCHEMA_VERSION,
    };
    let requirement = || loom_protocol::CapabilityApiRequirement {
        minimum: "1.0".to_owned(),
        maximum: None,
        required_features: Vec::new(),
        optional_features: Vec::new(),
    };
    let artifact = |name: &str, bytes: &[u8]| CapabilityCatalogArtifact {
        url: format!("{base}/{name}"),
        sha256: format!("{:x}", Sha256::digest(bytes)),
        bytes: bytes.len() as u64,
    };
    let package_publisher = loom_protocol::CapabilityPublisher {
        id: "publisher.example".to_owned(),
        key_id: package_key.key_id.clone(),
    };
    let catalog_publisher = loom_protocol::CapabilityPublisher {
        id: "neuro.official".to_owned(),
        key_id: catalog_key.key_id.clone(),
    };
    let payload = CapabilityCatalogPayload {
        schema_version: CAPABILITY_CATALOG_SCHEMA_VERSION,
        publisher: catalog_publisher,
        generated_at: "2020-01-01T00:00:00Z".to_owned(),
        expires_at: "2099-01-01T00:00:00Z".to_owned(),
        packages: vec![CapabilityCatalogEntry {
            qualified_id: "publisher.example/api-fixture".to_owned(),
            name: "API Fixture".to_owned(),
            description: "Signed daemon catalog fixture".to_owned(),
            version: "1.0.0".to_owned(),
            publisher: package_publisher,
            package: CapabilityCatalogPackage {
                url: format!("{base}/package.zip"),
                sha256: format!("{:x}", Sha256::digest(package)),
                bytes: package.len() as u64,
                signature: CapabilityCatalogPackageSignature {
                    algorithm: "ed25519".to_owned(),
                    key_id: package_key.key_id.clone(),
                },
            },
            sbom: artifact("package.cdx.json", sbom),
            provenance: artifact("provenance.json", provenance),
            host_compatibility: loom_protocol::CapabilityHostCompatibility {
                loom_capability_api: requirement(),
                hook_extension_api: requirement(),
                surface_api: None,
            },
            permissions: vec![
                "hook.unit.attachments.write".to_owned(),
                "hook.notice.show".to_owned(),
            ],
            disk_bytes: package.len() as u64 * 2,
        }],
    };
    let signature = loom_plugin_security::sign_message(
        catalog_key,
        &serde_json::to_vec(&payload).unwrap(),
    )
    .expect("catalog signature");
    serde_json::to_vec(&CapabilityCatalogDocument {
        signed: payload,
        signature: CapabilityCatalogSignature {
            algorithm: "ed25519".to_owned(),
            key_id: catalog_key.key_id.clone(),
            value: signature,
        },
    })
    .unwrap()
}

fn serve_capability_catalog_fixture(
    listener: TcpListener,
    bodies: BTreeMap<String, Vec<u8>>,
    request_count: usize,
) {
    for _ in 0..request_count {
        let (mut stream, _) = listener.accept().expect("catalog connection");
        let mut request = [0_u8; 4096];
        let size = stream.read(&mut request).expect("catalog request");
        let request = String::from_utf8_lossy(&request[..size]);
        let path = request.split_whitespace().nth(1).expect("request path");
        let body = bodies.get(path).expect("fixture body");
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .expect("catalog headers");
        stream.write_all(body).expect("catalog body");
    }
}
