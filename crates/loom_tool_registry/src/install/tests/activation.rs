use super::*;

#[test]
fn art_upgrade_rollback_and_integrity_verification_roundtrip() {
    let root = temp_root();
    let framework = FrameworkRegistry::new(&root);
    install_test_framework(&framework, "process");
    let registry = ToolRegistry::new(root.join("tools"));
    let manifest = r#"{"id":"rollback-art","name":"Rollback","description":"d","enabled":true,
            "execution":{"type":"framework_art","framework":"process"},
            "metadata":{"packageSecurity":{"version":"1.0.0"}}}"#;
    install_art_from_zip(
        &build_zip(manifest, &[("bin/tool.exe", b"version-one")]),
        &root,
        &framework,
        &registry,
    )
    .expect("install first Art");
    let settings = ArtUserSettings {
        credential_bindings: BTreeMap::from([("api_key".to_owned(), "rollback-secret".to_owned())]),
        ..ArtUserSettings::default()
    };
    ArtSettingsStore::new(&root)
        .save("publisher.test/rollback-art", settings.clone())
        .expect("save Art settings before upgrade");
    install_art_from_zip(
        &build_zip(manifest, &[("bin/tool.exe", b"version-two")]),
        &root,
        &framework,
        &registry,
    )
    .expect("install second Art");
    let current = registry.get_tool("rollback-art").unwrap().unwrap();
    assert_eq!(
        current
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.pointer("/artUserSettings/credentialBindings/api_key"))
            .and_then(serde_json::Value::as_str),
        Some("rollback-secret")
    );
    verify_art_package_integrity(&root, &current, &framework).expect("verify current Art");
    let installed = list_installed_art_versions(&root, "rollback-art", &registry)
        .expect("list immutable versions");
    assert_eq!(installed.len(), 2);
    let payloads = installed
        .iter()
        .map(|version| {
            let pinned = resolve_installed_art_package(
                &root,
                "rollback-art",
                &version.version,
                &version.digest,
                &registry,
                &framework,
            )
            .expect("resolve pinned package");
            let directory = pinned
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("artPackage"))
                .and_then(|package| package.get("dir"))
                .and_then(serde_json::Value::as_str)
                .expect("pinned Art directory");
            std::fs::read(Path::new(directory).join("bin/tool.exe")).expect("read pinned payload")
        })
        .collect::<Vec<_>>();
    assert!(payloads.iter().any(|payload| payload == b"version-one"));
    assert!(payloads.iter().any(|payload| payload == b"version-two"));

    let rolled_back =
        rollback_art_package(&root, "rollback-art", &registry, &framework).expect("rollback Art");
    assert_eq!(
        rolled_back
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.pointer("/artUserSettings/credentialBindings/api_key"))
            .and_then(serde_json::Value::as_str),
        Some("rollback-secret")
    );
    assert_eq!(
        ArtSettingsStore::new(&root)
            .get("publisher.test/rollback-art")
            .expect("load Art settings after rollback"),
        settings
    );
    verify_art_package_integrity(&root, &rolled_back, &framework).expect("verify rolled-back Art");
    let active =
        resolve_active_art_package(&root, "publisher.test/rollback-art").expect("active Art");
    assert_eq!(
        std::fs::read(active.join("bin/tool.exe")).unwrap(),
        b"version-one"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn art_integrity_and_rollback_reject_revoked_publisher_versions() {
    let root = temp_root();
    let framework = FrameworkRegistry::new(&root);
    install_test_framework(&framework, "process");
    let registry = ToolRegistry::new(root.join("tools"));
    let key = loom_plugin_security::generate_signing_key("art-release-key");
    framework
        .trust_publisher(loom_protocol::PublisherTrustRecord {
            publisher_id: "publisher.art".to_owned(),
            key_id: key.key_id.clone(),
            public_key: key.public_key.clone(),
            revoked: false,
        })
        .expect("trust publisher");
    for (version, payload) in [("1.0.0", b"one".as_slice()), ("2.0.0", b"two".as_slice())] {
        install_art_from_zip(
            &signed_art_zip("signed-art", version, "publisher.art", payload, &key),
            &root,
            &framework,
            &registry,
        )
        .unwrap_or_else(|error| panic!("install {version}: {error}"));
    }
    framework
        .revoke_publisher("publisher.art", &key.key_id)
        .expect("revoke publisher");
    let tool = registry
        .get_tool("publisher.art/signed-art")
        .expect("read tool")
        .expect("installed tool");
    assert!(verify_art_package_integrity(&root, &tool, &framework).is_err());
    assert!(
        rollback_art_package(&root, "publisher.art/signed-art", &registry, &framework,).is_err()
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn art_integrity_verification_rejects_package_and_lockfile_tampering() {
    let root = temp_root();
    let framework = FrameworkRegistry::new(&root);
    install_test_framework(&framework, "process");
    let registry = ToolRegistry::new(root.join("tools"));
    let manifest = r#"{"id":"tamper-art","name":"Tamper","description":"d","enabled":true,
            "execution":{"type":"framework_art","framework":"process"}}"#;
    let report = install_art_from_zip(
        &build_zip(manifest, &[("bin/tool.exe", b"original")]),
        &root,
        &framework,
        &registry,
    )
    .expect("install Art");
    let tool = registry.get_tool("tamper-art").unwrap().unwrap();
    set_tree_readonly(&report.art_dir, false).expect("unlock test package");
    std::fs::write(report.art_dir.join("bin/tool.exe"), b"tampered").unwrap();
    assert!(verify_art_package_integrity(&root, &tool, &framework).is_err());

    std::fs::write(report.art_dir.join("bin/tool.exe"), b"original").unwrap();
    let art_root = root.join("arts/publisher.test/tamper-art");
    let activation = read_art_activation(&art_root.join("active.json")).unwrap();
    let mut lock: PluginLockfile = serde_json::from_slice(
        &std::fs::read(&activation.active.lockfile).expect("read Art lockfile"),
    )
    .unwrap();
    let original_version = lock.package_version.clone();
    lock.package_version = "9.9.9".to_owned();
    std::fs::write(
        &activation.active.lockfile,
        serde_json::to_vec_pretty(&lock).unwrap(),
    )
    .unwrap();
    assert!(verify_art_package_integrity(&root, &tool, &framework).is_err());

    lock.package_version = original_version;
    lock.schema_version = u32::MAX;
    std::fs::write(
        &activation.active.lockfile,
        serde_json::to_vec_pretty(&lock).unwrap(),
    )
    .unwrap();
    assert!(verify_art_package_integrity(&root, &tool, &framework).is_err());
    std::fs::remove_dir_all(&root).ok();
}
