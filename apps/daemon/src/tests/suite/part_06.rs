// Loom daemon tests fragment 6; included into the shared crate test module.
#[test]
fn bundled_catalog_art_install_preserves_strict_external_trust_policy() {
    let root = unique_temp_dir("bundled-catalog-art-install");
    let zip = art_package_zip("bundled-catalog-art", "1.0.0", b"bundled");
    let package_sha256 = sha256_bytes(&zip);
    let config = DaemonConfig::localhost(0)
        .with_bundled_art_sha256_allowlist([package_sha256])
        .expect("configure bundled Art allowlist");
    let runtime = test_daemon_runtime_from_config(&root, config);
    let framework_key = loom_plugin_security::generate_signing_key("framework-key");
    let mut trust = loom_plugin_security::TrustStore::default();
    trust.set_policy(TrustPolicy::RequireTrusted);
    // The framework's publisher has to be trusted, not merely signed: the strict policy is
    // enforced on every framework readiness probe, so an untrusted framework would fail the
    // Art installs below on readiness instead of on the external-package policy under test.
    trust.trust(loom_protocol::PublisherTrustRecord {
        publisher_id: "publisher.test".to_owned(),
        key_id: framework_key.key_id.clone(),
        public_key: framework_key.public_key.clone(),
        revoked: false,
    });
    trust
        .write_atomic(&root.join("plugin-trust.json"))
        .expect("write strict trust policy");
    runtime
        .framework_registry
        .install_framework_package_from_zip(&signed_framework_package_zip(
            "process",
            "1.0.0",
            &framework_key,
        ))
        .expect("install process framework");
    let encoded = format!("data:application/zip;base64,{}", BASE64.encode(&zip));

    let external_body = serde_json::to_string(&json!({ "zipBase64": encoded.clone() }))
        .expect("external install body");
    let (status, body) = install_art(
        &external_body,
        &runtime.tool_registry,
        &runtime.framework_registry,
        &runtime.workflow_store,
        &root,
        &runtime.hook_bridge,
        &runtime.bundled_art_sha256_allowlist,
    )
    .expect("external install response");
    assert_eq!(status, 400);
    assert!(body.contains("trust policy rejected package status Unsigned"));

    let forged_zip = art_package_zip("forged-bundled-art", "1.0.0", b"forged");
    let forged_body = serde_json::to_string(&json!({
        "zipBase64": format!("data:application/zip;base64,{}", BASE64.encode(forged_zip)),
        "bundledCatalog": true,
    }))
    .expect("forged bundled install body");
    let (status, body) = install_art(
        &forged_body,
        &runtime.tool_registry,
        &runtime.framework_registry,
        &runtime.workflow_store,
        &root,
        &runtime.hook_bridge,
        &runtime.bundled_art_sha256_allowlist,
    )
    .expect("forged bundled install response");
    assert_eq!(status, 403);
    assert!(body.contains("bundled_art_not_allowlisted"));

    let bundled_body = serde_json::to_string(&json!({
        "zipBase64": encoded,
        "bundledCatalog": true,
    }))
    .expect("bundled install body");
    let (status, body) = install_art(
        &bundled_body,
        &runtime.tool_registry,
        &runtime.framework_registry,
        &runtime.workflow_store,
        &root,
        &runtime.hook_bridge,
        &runtime.bundled_art_sha256_allowlist,
    )
    .expect("bundled install response");
    assert_eq!(status, 200, "body={body}");
    assert!(runtime
        .tool_registry
        .get_tool("bundled-catalog-art")
        .unwrap()
        .is_some());
    drop(runtime);
    fs::remove_dir_all(&root).ok();
}

#[test]
fn authored_art_handlers_cover_create_package_rollback_and_uninstall() {
    let root = unique_temp_dir("authored-art-handlers");
    let source_root = root.join("author-source");
    fs::create_dir_all(source_root.join("nested")).expect("create authored source");
    fs::write(source_root.join("main.py"), "print('authored source')\n")
        .expect("write authored source");
    fs::write(source_root.join("nested/helper.txt"), "helper\n").expect("write authored helper");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
    let framework_key = loom_plugin_security::generate_signing_key("framework-key");
    let mut trust = loom_plugin_security::TrustStore::default();
    trust.set_policy(TrustPolicy::RequireSigned);
    trust
        .write_atomic(&root.join("plugin-trust.json"))
        .expect("write strict authored Art trust policy");
    // Signed, because the strict policy is enforced on every framework readiness probe and the
    // authored-Art routes below all need a ready framework.
    runtime
        .framework_registry
        .install_framework_package_from_zip(&signed_framework_package_zip(
            "process",
            "1.0.0",
            &framework_key,
        ))
        .expect("install process framework");
    let request = |version: &str| {
        serde_json::to_string(&json!({
            "tool": {
                "id": "authored-art",
                "name": "Authored Art",
                "description": "daemon authoring route fixture",
                "enabled": true,
                "execution": { "type": "framework_art", "framework": "process" },
                "metadata": {
                    "dependencies": { "framework": "process" },
                    "packageSecurity": { "version": version }
                }
            },
            "runtime": {
                "protocolVersion": "loom.art.runtime.v1",
                "entry": { "command": "runtime.cmd", "args": [] }
            },
            "files": [{
                "path": "runtime/adapter.txt",
                "content": "adapter"
            }],
            "sourceDirectory": source_root,
            "sourceDirectoryTarget": "runtime/plugin"
        }))
        .unwrap()
    };
    let (status, first) = create_authored_art(
        &request("1.0.0"),
        &runtime.tool_registry,
        &runtime.framework_registry,
        &root,
        &runtime.hook_bridge,
    )
    .expect("create v1");
    assert_eq!(status, 200, "body={first}");
    let first_response: Value = serde_json::from_str(&first).expect("parse create response");
    assert_eq!(first_response["report"]["trustStatus"], "unsigned");
    let installed = runtime
        .tool_registry
        .get_tool("authored-art")
        .expect("read authored Art")
        .expect("authored Art registered");
    loom_tool_registry::install::verify_art_package_integrity(
        &root,
        &installed,
        &runtime.framework_registry,
    )
    .expect("verify locally authored Art under strict trust policy");
    let (status, second) = create_authored_art(
        &request("2.0.0"),
        &runtime.tool_registry,
        &runtime.framework_registry,
        &root,
        &runtime.hook_bridge,
    )
    .expect("create v2");
    assert_eq!(status, 200, "body={second}");

    let (status, package) =
        package_art("authored-art", &runtime.tool_registry, &root).expect("package authored Art");
    assert_eq!(status, 200);
    let package: Value = serde_json::from_str(&package).unwrap();
    let zip_base64 = package["zipBase64"]
        .as_str()
        .expect("authored package data URL");
    assert!(zip_base64.starts_with("data:application/zip;base64,"));
    let zip_bytes = loom_image_io::decode_data_url_bytes(zip_base64).expect("decode package");
    let mut archive = zip::ZipArchive::new(Cursor::new(zip_bytes)).expect("open authored zip");
    for expected in [
        "manifest.json",
        "art.runtime.json",
        "runtime/adapter.txt",
        "runtime/plugin/main.py",
        "runtime/plugin/nested/helper.txt",
    ] {
        assert!(archive.by_name(expected).is_ok(), "missing {expected}");
    }

    let invalid_files = request("3.0.0").replace("runtime/adapter.txt", "../escape.txt");
    let (status, body) = create_authored_art(
        &invalid_files,
        &runtime.tool_registry,
        &runtime.framework_registry,
        &root,
        &runtime.hook_bridge,
    )
    .expect("reject unsafe authored file");
    assert_eq!(status, 400, "body={body}");
    assert!(
        body.contains("invalid authored Art file path"),
        "body={body}"
    );

    let (status, rollback) = rollback_art(
        "authored-art",
        &runtime.tool_registry,
        &runtime.framework_registry,
        &runtime.workflow_store,
        &root,
        &runtime.hook_bridge,
    )
    .expect("rollback authored Art");
    assert_eq!(status, 200, "body={rollback}");
    let rollback: Value = serde_json::from_str(&rollback).unwrap();
    assert_eq!(
        rollback["tool"]["metadata"]["packageSecurity"]["version"],
        "1.0.0"
    );

    let (status, updated) = update_art_version(
        "authored-art",
        r#"{"version":"2.0.0"}"#,
        &runtime.tool_registry,
        &runtime.framework_registry,
        &runtime.workflow_store,
        &root,
        &runtime.hook_bridge,
    )
    .expect("activate exact authored Art version");
    assert_eq!(status, 200, "body={updated}");
    let updated: Value = serde_json::from_str(&updated).unwrap();
    assert_eq!(updated["currentVersion"], "2.0.0");

    let (status, body) = uninstall_art(
        "authored-art",
        "{}",
        &runtime.tool_registry,
        &root,
        &runtime.hook_bridge,
        &runtime.mcp_servers,
        &mcp_server_store_path(&root),
    )
    .expect("uninstall authored Art");
    assert_eq!(status, 200, "body={body}");
    assert!(runtime
        .tool_registry
        .get_tool("authored-art")
        .unwrap()
        .is_none());
    assert!(!root.join("arts/authored-art").exists());
    drop(runtime);
    fs::remove_dir_all(&root).ok();
}
