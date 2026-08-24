use super::*;

#[test]
fn installs_package_extracts_files_and_rewrites_paths() {
    let root = temp_root();
    let framework = FrameworkRegistry::new(&root);
    install_test_framework(&framework, "process");
    let registry = ToolRegistry::new(root.join("tools"));

    let manifest = r#"{"id":"pingo-art","name":"Pingo","description":"compress","enabled":true,
            "execution":{"type":"framework_art","framework":"process"}}"#;
    let zip = build_zip(manifest, &[("bin/pingo.exe", b"MZ-fake-exe")]);

    let report = install_art_from_zip(&zip, &root, &framework, &registry).expect("install art");
    assert_eq!(report.tool_id, "pingo-art");
    assert_eq!(report.framework, "process");
    // Binary extracted into the art dir.
    assert!(report.art_dir.join("bin/pingo.exe").exists());
    assert!(report
        .installed_files
        .iter()
        .any(|f| f.contains("pingo.exe")));

    // Registered tool keeps the generic process framework identity.
    let saved = registry.get_tool("pingo-art").unwrap().unwrap();
    assert!(matches!(
        saved.execution,
        crate::ToolExecution::FrameworkArt { ref framework } if framework == "process"
    ));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn reinstall_rejects_a_tampered_existing_immutable_version() {
    let root = temp_root();
    let framework = FrameworkRegistry::new(&root);
    install_test_framework(&framework, "process");
    let registry = ToolRegistry::new(root.join("tools"));
    let manifest = r#"{"id":"tampered-art","name":"Tampered","description":"integrity","enabled":true,
            "execution":{"type":"framework_art","framework":"process"}}"#;
    let zip = build_zip(manifest, &[("bin/tool.exe", b"original")]);
    let report = install_art_from_zip(&zip, &root, &framework, &registry).expect("install Art");

    set_tree_readonly(&report.art_dir, false).expect("unlock fixture");
    std::fs::write(report.art_dir.join("bin/tool.exe"), b"tampered").expect("tamper version");
    let error = install_art_from_zip(&zip, &root, &framework, &registry)
        .expect_err("reinstall must not reuse modified immutable content");

    assert!(matches!(
        error,
        ArtInstallError::InvalidPackage(reason)
            if reason.contains("existing immutable Art version content does not match its digest")
    ));
    remove_tree(&root).ok();
}

#[test]
fn strict_trust_policy_allows_local_and_bundled_sources_but_rejects_external_unsigned_packages() {
    let root = temp_root();
    let framework = FrameworkRegistry::new(&root);
    let key = loom_plugin_security::generate_signing_key("framework-key");
    // The framework has to be signed: `framework_ready_in` enforces the persisted policy on
    // every readiness probe, so an unsigned framework would fail all three Art installs below
    // on framework readiness instead of on the Art install policy under test.
    install_signed_test_framework(&framework, "process", &key);
    let registry = ToolRegistry::new(root.join("tools"));
    let mut trust = TrustStore::default();
    trust.set_policy(loom_plugin_security::TrustPolicy::RequireSigned);
    trust
        .write_atomic(&root.join("plugin-trust.json"))
        .expect("write strict trust policy");
    let manifest = r#"{"id":"local-draft","name":"Local Draft","description":"draft","enabled":true,
            "execution":{"type":"framework_art","framework":"process"}}"#;
    let zip = build_zip(manifest, &[("runtime/main.txt", b"local")]);

    let external_error = install_art_from_zip(&zip, &root, &framework, &registry)
        .expect_err("external unsigned package must remain rejected");
    assert!(matches!(
        external_error,
        ArtInstallError::InvalidPackage(reason)
            if reason.contains("trust policy rejected package status Unsigned")
    ));

    let report = install_authored_art_from_zip(&zip, &root, &framework, &registry)
        .expect("local authored draft must bypass external install policy");
    assert_eq!(report.trust_status, PackageTrustStatus::Unsigned);
    let saved = registry
        .get_tool("local-draft")
        .expect("read local draft")
        .expect("local draft registered");
    verify_art_package_integrity(&root, &saved, &framework)
        .expect("local draft integrity must remain verifiable");
    let activation = read_art_activation(&root.join("arts/publisher.test/local-draft/active.json"))
        .expect("local draft activation");
    assert!(activation.local_authoring);
    assert!(!activation.bundled_catalog);

    let bundled_manifest = r#"{"id":"bundled-draft","name":"Bundled Draft","description":"catalog","enabled":true,
            "execution":{"type":"framework_art","framework":"process"}}"#;
    let bundled_zip = build_zip(bundled_manifest, &[("runtime/main.txt", b"bundled")]);
    let bundled_report = install_bundled_art_from_zip(&bundled_zip, &root, &framework, &registry)
        .expect("checksum-verified bundled catalog package must bypass user install policy");
    assert_eq!(bundled_report.trust_status, PackageTrustStatus::Unsigned);
    let bundled = registry
        .get_tool("bundled-draft")
        .expect("read bundled draft")
        .expect("bundled draft registered");
    verify_art_package_integrity(&root, &bundled, &framework)
        .expect("bundled catalog package integrity must remain verifiable");
    let bundled_activation =
        read_art_activation(&root.join("arts/publisher.test/bundled-draft/active.json"))
            .expect("bundled draft activation");
    assert!(!bundled_activation.local_authoring);
    assert!(bundled_activation.bundled_catalog);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn publisher_namespace_keeps_same_art_id_in_separate_roots() {
    let root = temp_root();
    let framework = FrameworkRegistry::new(&root);
    install_test_framework(&framework, "process");
    let registry = ToolRegistry::new(root.join("tools"));
    let package = |publisher: &str, marker: &'static [u8]| {
        let manifest = serde_json::json!({
            "id": "shared-art",
            "name": publisher,
            "description": "publisher scoped",
            "enabled": true,
            "execution": { "type": "framework_art", "framework": "process" },
            "metadata": {
                "packageSecurity": {
                    "version": "0.1.0",
                    "publisher": { "id": publisher, "name": publisher }
                }
            }
        });
        build_zip(
            &serde_json::to_string(&manifest).expect("serialize Art manifest"),
            &[("bin/tool.exe", marker)],
        )
    };
    let alpha = install_art_from_zip(
        &package("publisher.alpha", b"alpha"),
        &root,
        &framework,
        &registry,
    )
    .expect("install alpha Art");
    let beta = install_art_from_zip(
        &package("publisher.beta", b"beta"),
        &root,
        &framework,
        &registry,
    )
    .expect("install beta Art");

    assert_ne!(alpha.art_dir, beta.art_dir);
    assert!(alpha.art_dir.starts_with(root.join("arts/publisher.alpha")));
    assert!(beta.art_dir.starts_with(root.join("arts/publisher.beta")));
    assert!(matches!(
        registry.get_tool("shared-art"),
        Err(crate::ToolRegistryError::AmbiguousToolId { .. })
    ));
    uninstall_art_package(&root, "publisher.alpha/shared-art", &registry).expect("uninstall alpha");
    assert!(registry
        .get_tool("publisher.beta/shared-art")
        .expect("get beta")
        .is_some());
    remove_tree(&root).ok();
}

#[test]
fn unqualified_uninstall_recovers_a_unique_package_missing_from_the_registry() {
    let root = temp_root();
    let framework = FrameworkRegistry::new(&root);
    install_test_framework(&framework, "process");
    let registry = ToolRegistry::new(root.join("tools"));
    let manifest = serde_json::json!({
        "id": "orphan-art",
        "name": "Orphan Art",
        "description": "package remains after a registry-only deletion",
        "enabled": true,
        "execution": { "type": "framework_art", "framework": "process" },
        "metadata": {
            "packageSecurity": {
                "version": "0.1.0",
                "publisher": { "id": "publisher.test", "name": "Publisher Test" }
            }
        }
    });
    install_art_from_zip(
        &build_zip(
            &serde_json::to_string(&manifest).expect("serialize orphan Art manifest"),
            &[],
        ),
        &root,
        &framework,
        &registry,
    )
    .expect("install orphan Art");
    let package_root = root.join("arts/publisher.test/orphan-art");
    assert!(package_root.is_dir());
    registry
        .delete_tool("publisher.test/orphan-art")
        .expect("delete orphan registry entry");

    uninstall_art_package(&root, "orphan-art", &registry)
        .expect("resolve and uninstall orphan package");

    assert!(!package_root.exists());
    remove_tree(&root).ok();
}

#[test]
fn install_preserves_process_framework_identity() {
    let root = temp_root();
    let framework = FrameworkRegistry::new(&root);
    install_test_framework(&framework, "process");
    let registry = ToolRegistry::new(root.join("tools"));

    let manifest = r#"{"id":"shell-copy-art","name":"Shell Copy","description":"copy","enabled":true,
            "execution":{"type":"framework_art","framework":"process"}}"#;
    let zip = build_zip(manifest, &[]);

    install_art_from_zip(&zip, &root, &framework, &registry).expect("install shell art");
    let saved = registry.get_tool("shell-copy-art").unwrap().unwrap();
    assert!(matches!(
        saved.execution,
        crate::ToolExecution::FrameworkArt { ref framework } if framework == "process"
    ));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn verifies_bundled_binary_hash_and_reports_it() {
    let root = temp_root();
    let framework = FrameworkRegistry::new(&root);
    install_test_framework(&framework, "process");
    let registry = ToolRegistry::new(root.join("tools"));

    let exe = b"MZ-fake-exe";
    let digest = sha256_hex(exe);
    let manifest = format!(
        r#"{{"id":"pingo-hashed","name":"Pingo","description":"c","enabled":true,
            "execution":{{"type":"framework_art","framework":"process"}},
            "metadata":{{"dependencies":{{"binaries":[{{"name":"bin/pingo.exe","sha256":"{digest}"}}]}}}}}}"#
    );
    let zip = build_zip(&manifest, &[("bin/pingo.exe", exe)]);
    let report =
        install_art_from_zip(&zip, &root, &framework, &registry).expect("install hashed art");
    assert_eq!(report.binaries, vec!["bin/pingo.exe"]);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn rejects_bundled_binary_hash_mismatch() {
    let root = temp_root();
    let framework = FrameworkRegistry::new(&root);
    install_test_framework(&framework, "process");
    let registry = ToolRegistry::new(root.join("tools"));

    let manifest = r#"{"id":"pingo-badhash","name":"Pingo","description":"c","enabled":true,
            "execution":{"type":"framework_art","framework":"process"},
            "metadata":{"dependencies":{"binaries":[{"name":"bin/pingo.exe","sha256":"deadbeef"}]}}}"#;
    let zip = build_zip(manifest, &[("bin/pingo.exe", b"MZ-fake-exe")]);
    let err = install_art_from_zip(&zip, &root, &framework, &registry).unwrap_err();
    assert!(matches!(err, ArtInstallError::BinaryHashMismatch { .. }));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn rejects_binary_neither_bundled_nor_downloadable() {
    let root = temp_root();
    let framework = FrameworkRegistry::new(&root);
    install_test_framework(&framework, "process");
    let registry = ToolRegistry::new(root.join("tools"));

    let manifest = r#"{"id":"pingo-nobs","name":"Pingo","description":"c","enabled":true,
            "execution":{"type":"framework_art","framework":"process"},
            "metadata":{"dependencies":{"binaries":[{"name":"bin/missing.exe"}]}}}"#;
    let zip = build_zip(manifest, &[]);
    let err = install_art_from_zip(&zip, &root, &framework, &registry).unwrap_err();
    assert!(matches!(err, ArtInstallError::BinaryMissing { .. }));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn rejects_remote_binary_without_sha256_before_downloading() {
    let root = temp_root();
    let framework = FrameworkRegistry::new(&root);
    install_test_framework(&framework, "process");
    let registry = ToolRegistry::new(root.join("tools"));

    let manifest = r#"{"id":"pingo-unpinned","name":"Pingo","description":"c","enabled":true,
            "execution":{"type":"framework_art","framework":"process"},
            "metadata":{"dependencies":{"binaries":[{"name":"bin/pingo.exe","url":"http://127.0.0.1:1/pingo.exe"}]}}}"#;
    let zip = build_zip(manifest, &[]);
    let err = install_art_from_zip(&zip, &root, &framework, &registry).unwrap_err();
    assert!(matches!(
        err,
        ArtInstallError::RemoteBinaryHashRequired { .. }
    ));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn rejects_binary_path_that_escapes_the_art_package() {
    let root = temp_root();
    let framework = FrameworkRegistry::new(&root);
    install_test_framework(&framework, "process");
    let registry = ToolRegistry::new(root.join("tools"));

    let manifest = r#"{"id":"pingo-escape","name":"Pingo","description":"c","enabled":true,
            "execution":{"type":"framework_art","framework":"process"},
            "metadata":{"dependencies":{"binaries":[{"name":"../escape.exe","url":"http://127.0.0.1:1/escape.exe"}]}}}"#;
    let zip = build_zip(manifest, &[]);
    let err = install_art_from_zip(&zip, &root, &framework, &registry).unwrap_err();
    assert!(matches!(
        err,
        ArtInstallError::InvalidPackage(reason)
            if reason.contains("must stay inside the package")
    ));
    assert!(!root.join("escape.exe").exists());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn rejects_install_when_framework_not_installed() {
    let root = temp_root();
    let framework = FrameworkRegistry::new(&root);
    let registry = ToolRegistry::new(root.join("tools"));
    // process is NOT installed by default.
    let manifest = r#"{"id":"py-art","name":"Py","description":"d","enabled":true,
            "execution":{"type":"framework_art","framework":"process"}}"#;
    let zip = build_zip(manifest, &[]);
    let err = install_art_from_zip(&zip, &root, &framework, &registry).unwrap_err();
    assert!(matches!(err, ArtInstallError::FrameworkNotReady { .. }));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn rejects_unsafe_art_id() {
    let root = temp_root();
    let framework = FrameworkRegistry::new(&root);
    let registry = ToolRegistry::new(root.join("tools"));
    let manifest = r#"{"id":"../evil","name":"E","description":"d","enabled":true,
            "execution":{"type":"framework_art","framework":"process"}}"#;
    let zip = build_zip(manifest, &[]);
    let err = install_art_from_zip(&zip, &root, &framework, &registry).unwrap_err();
    assert!(matches!(err, ArtInstallError::InvalidArtId(_)));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn rejects_windows_reserved_and_trailing_dot_art_ids() {
    let root = temp_root();
    let framework = FrameworkRegistry::new(&root);
    let registry = ToolRegistry::new(root.join("tools"));
    for id in ["CON", "art.", "a..b"] {
        let manifest = format!(
            r#"{{"id":"{id}","name":"E","description":"d","enabled":true,
            "execution":{{"type":"framework_art","framework":"process"}}}}"#
        );
        let zip = build_zip(&manifest, &[]);
        let err = install_art_from_zip(&zip, &root, &framework, &registry).unwrap_err();
        assert!(
            matches!(err, ArtInstallError::InvalidArtId(_)),
            "{id} should be rejected, got {err:?}"
        );
    }
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn rejects_missing_or_mismatched_art_qualified_id() {
    let root = temp_root();
    let framework = FrameworkRegistry::new(&root);
    install_test_framework(&framework, "process");
    let registry = ToolRegistry::new(root.join("tools"));

    let missing = r#"{"id":"sample-art","name":"S","description":"d","enabled":true,
            "execution":{"type":"framework_art","framework":"process"},
            "metadata":{"packageSecurity":{"publisher":{"id":"publisher.test"}},"art":{}}}"#;
    let missing_zip = build_zip(missing, &[]);
    let missing_err = install_art_from_zip(&missing_zip, &root, &framework, &registry).unwrap_err();
    assert!(
        matches!(
            missing_err,
            ArtInstallError::InvalidPackage(ref reason) if reason.contains("qualifiedId is required")
        ),
        "{missing_err:?}"
    );

    let mismatched = r#"{"id":"sample-art","name":"S","description":"d","enabled":true,
            "execution":{"type":"framework_art","framework":"process"},
            "metadata":{"packageSecurity":{"publisher":{"id":"publisher.test"}},"art":{"qualifiedId":"evil.pub/other-art"}}}"#;
    let mismatched_zip = build_zip(mismatched, &[]);
    let mismatched_err =
        install_art_from_zip(&mismatched_zip, &root, &framework, &registry).unwrap_err();
    assert!(
        matches!(
            mismatched_err,
            ArtInstallError::InvalidPackage(ref reason) if reason.contains("does not match")
        ),
        "{mismatched_err:?}"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn direct_install_rejects_unlocked_dependent_arts() {
    let root = temp_root();
    let framework = FrameworkRegistry::new(&root);
    install_test_framework(&framework, "workflow");
    let registry = ToolRegistry::new(root.join("tools"));
    let manifest = r#"{"id":"wf-art","name":"WF","description":"d","enabled":true,
            "execution":{"type":"workflow","workflowId":"wf"},
            "metadata":{"dependencies":{"framework":"workflow","arts":["dep-1","dep-2"]}}}"#;
    let zip = build_zip(manifest, &[]);
    let error = install_art_from_zip(&zip, &root, &framework, &registry)
        .expect_err("direct install must not activate an Art with missing dependencies");
    assert!(matches!(
        error,
        ArtInstallError::InvalidPackage(reason)
            if reason.contains("Art dependency `dep-1` is not installed")
    ));
    assert!(registry.get_tool("wf-art").unwrap().is_none());
    std::fs::remove_dir_all(&root).ok();
}
