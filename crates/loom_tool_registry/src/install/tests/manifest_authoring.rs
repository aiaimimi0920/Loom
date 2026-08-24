use super::*;

#[test]
fn reads_manifest_from_zip() {
    let manifest = r#"{"id":"art-x","name":"X","description":"d","enabled":true,
            "execution":{"type":"framework_art","framework":"process"}}"#;
    let zip = build_zip(manifest, &[]);
    let tool = read_manifest_from_zip(&zip).expect("read manifest");
    assert_eq!(tool.id, "art-x");
}

#[test]
fn resolves_mcp_dependency_from_the_installer_immutable_directory() {
    let root = temp_root();
    let config = install_test_mcp_package(&root);
    let package = config.package.as_ref().expect("MCP package state");
    assert_eq!(
        package
            .package_dir
            .file_name()
            .and_then(|name| name.to_str()),
        Some(
            format!(
                "{}-{}",
                package.version,
                &package.digest[..loom_mcp::package::PACKAGE_DIRECTORY_DIGEST_CHARS]
            )
            .as_str()
        )
    );

    let resolved = resolve_mcp_dependency_locks(
        &root,
        &[ArtMcpServerDependency {
            id: package.qualified_id.clone(),
            version: "^1.2".to_owned(),
        }],
    )
    .expect("resolve installed MCP package");
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].kind, "mcp");
    assert_eq!(resolved[0].id, package.qualified_id);
    assert_eq!(resolved[0].version, package.version);
    assert!(is_sha256_hex(&resolved[0].sha256));
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn install_rejects_art_package_without_publisher() {
    let root = temp_root();
    let framework = FrameworkRegistry::new(&root);
    install_test_framework(&framework, "process");
    let registry = ToolRegistry::new(root.join("tools"));
    let manifest = r#"{"id":"unpublished-art","name":"Unpublished","description":"test","enabled":true,
            "execution":{"type":"framework_art","framework":"process"}}"#;
    let mut bytes = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut bytes));
        let options = SimpleFileOptions::default();
        writer.start_file(MANIFEST_NAME, options).unwrap();
        writer.write_all(manifest.as_bytes()).unwrap();
        writer.finish().unwrap();
    }

    let error = install_art_from_zip(&bytes, &root, &framework, &registry)
        .expect_err("publisherless Art package must fail closed");
    assert!(matches!(
        error,
        ArtInstallError::InvalidPackage(reason) if reason.contains("publisher is required")
    ));
    assert!(!root.join("arts").join("unpublished-art").exists());
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn install_rejects_missing_or_invalid_art_package_version() {
    let root = temp_root();
    let framework = FrameworkRegistry::new(&root);
    install_test_framework(&framework, "process");
    let registry = ToolRegistry::new(root.join("tools"));

    for version in [None, Some("latest")] {
        let mut manifest = serde_json::json!({
            "id": "version-required-art",
            "name": "Version required",
            "description": "test",
            "enabled": true,
            "execution": { "type": "framework_art", "framework": "process" },
            "metadata": {
                "packageSecurity": {
                    "publisher": { "id": "publisher.test" }
                },
                "art": { "qualifiedId": "publisher.test/version-required-art" }
            }
        });
        if let Some(version) = version {
            manifest["metadata"]["packageSecurity"]["version"] = serde_json::json!(version);
        }
        let mut bytes = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(Cursor::new(&mut bytes));
            let options = SimpleFileOptions::default();
            writer.start_file(MANIFEST_NAME, options).unwrap();
            writer
                .write_all(serde_json::to_string(&manifest).unwrap().as_bytes())
                .unwrap();
            writer.finish().unwrap();
        }

        let error = install_art_from_zip(&bytes, &root, &framework, &registry)
            .expect_err("Art version must fail closed");
        assert!(
            matches!(error, ArtInstallError::InvalidPackage(ref reason)
                    if reason.contains("metadata.packageSecurity.version")),
            "{error:?}"
        );
    }
    assert!(registry
        .get_tool("publisher.test/version-required-art")
        .unwrap()
        .is_none());
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn authored_package_includes_runtime_and_package_local_files() {
    let mut tool = ToolDefinition::new(
        "authored-process-art",
        "Authored Process Art",
        "authored package fixture",
        ToolExecution::FrameworkArt {
            framework: "publisher.test/process".to_owned(),
        },
    );
    tool.metadata = Some(serde_json::json!({
        "packageSecurity": { "version": "0.1.0", "publisher": { "id": "publisher.test" } }
    }));
    let runtime: ArtRuntimeManifest = serde_json::from_value(serde_json::json!({
        "protocolVersion": "loom.art.runtime.v1",
        "entry": {
            "command": "python.exe",
            "args": ["runtime/main.py"]
        }
    }))
    .expect("runtime manifest");
    let zip = build_authored_art_package_zip(
        &tool,
        Some(&runtime),
        &[
            ("runtime/main.py".to_owned(), b"print('ok')\n".to_vec()),
            ("runtime/data/config.json".to_owned(), b"{}\n".to_vec()),
        ],
    )
    .expect("build authored package");

    let mut archive = zip::ZipArchive::new(Cursor::new(zip)).expect("open authored package");
    for expected in [
        "manifest.json",
        "art.runtime.json",
        "runtime/main.py",
        "runtime/data/config.json",
    ] {
        assert!(archive.by_name(expected).is_ok(), "missing {expected}");
    }
    let mut source = String::new();
    archive
        .by_name("runtime/main.py")
        .expect("runtime source")
        .read_to_string(&mut source)
        .expect("read runtime source");
    assert_eq!(source, "print('ok')\n");
}

#[test]
fn authored_package_rejects_reserved_unsafe_and_duplicate_paths() {
    let mut tool = ToolDefinition::new(
        "invalid-authored-process-art",
        "Invalid Authored Process Art",
        "invalid authored package fixture",
        ToolExecution::FrameworkArt {
            framework: "publisher.test/process".to_owned(),
        },
    );
    tool.metadata = Some(serde_json::json!({
        "packageSecurity": { "version": "0.1.0", "publisher": { "id": "publisher.test" } }
    }));

    for path in [
        "manifest.json",
        "art.runtime.json",
        "../escape.py",
        "C:/escape.py",
    ] {
        let error = build_authored_art_package_zip(&tool, None, &[(path.to_owned(), Vec::new())])
            .expect_err("unsafe authored path must fail");
        assert!(error.to_string().contains("invalid authored Art file path"));
    }

    let error = build_authored_art_package_zip(
        &tool,
        None,
        &[
            ("runtime/main.py".to_owned(), Vec::new()),
            ("runtime/main.py".to_owned(), Vec::new()),
        ],
    )
    .expect_err("duplicate authored path must fail");
    assert!(error.to_string().contains("invalid authored Art file path"));
}
