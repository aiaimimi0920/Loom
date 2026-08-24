//! Catalog, permission, dependency, and trust-store tests.
use super::*;

#[test]
fn official_framework_names_match_ui_vocabulary() {
    assert_eq!(framework_name("cloud_api"), "云端");
    assert_eq!(framework_name("mcp"), "MCP");
    assert_eq!(framework_name("process"), "脚本");
    assert_eq!(framework_name("workflow"), "流程");
}

#[test]
fn starts_with_no_frameworks_installed() {
    let root = temp_root();
    let registry = FrameworkRegistry::new(&root);
    let installed = registry.installed_ids();
    assert!(installed.is_empty());
    for id in FRAMEWORK_IDS {
        assert!(!registry.is_installed(id));
    }
    for status in registry.statuses() {
        assert!(!status.installed);
        assert!(!status.enabled);
        assert!(!status.ready);
        assert!(status.version.is_none());
        assert!(status.runtime_dir.is_none());
    }
    // All optional frameworks, including the former built-in kinds, are
    // absent from a fresh control plane.
    assert!(!installed.contains("process"));
    assert!(!installed.contains("mcp"));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn install_and_uninstall_roundtrip() {
    let root = temp_root();
    let registry = FrameworkRegistry::new(&root);
    let status = registry
        .install_with_runtime_fetcher("mcp", &|_id| Ok(fake_framework_package_zip("mcp")))
        .expect("install mcp");
    assert!(status.installed);
    assert!(status.enabled);
    assert!(status.ready, "mcp package entry should be ready");
    assert_eq!(status.version.as_deref(), Some("0.1.0"));
    assert!(status.runtime_dir.is_some());
    assert!(registry.is_installed("mcp"));

    registry.uninstall("mcp").expect("uninstall mcp");
    assert!(!registry.is_installed("mcp"));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn third_party_framework_package_is_dynamic_and_lifecycle_managed() {
    let root = temp_root();
    let registry = FrameworkRegistry::new(&root);
    let id = "third-party-echo";
    let installed = registry
        .install_framework_package_from_zip(&fake_framework_package_zip(id))
        .expect("install third-party framework");

    assert_eq!(installed.id, id);
    assert!(installed.installed);
    assert!(installed.ready);
    assert!(registry.statuses().iter().any(|status| status.id == id));

    let disabled = registry.disable(id).expect("disable third-party framework");
    assert!(!disabled.ready);
    let enabled = registry.enable(id).expect("enable third-party framework");
    assert!(enabled.ready);
    let removed = registry
        .uninstall(id)
        .expect("uninstall third-party framework");
    assert!(!removed.installed);
    assert!(!registry.installed_ids().contains(id));

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn statuses_cover_all_four_frameworks() {
    let root = temp_root();
    let registry = FrameworkRegistry::new(&root);
    let statuses = registry.statuses();
    assert_eq!(statuses.len(), 4);
    for id in FRAMEWORK_IDS {
        let status = statuses.iter().find(|status| status.id == id).unwrap();
        assert!(!status.installed, "{id} should not be installed by default");
        assert!(!status.enabled, "{id} should not be enabled by default");
        assert!(!status.ready, "{id} should not be ready by default");
    }
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn unknown_framework_rejected() {
    let root = temp_root();
    let registry = FrameworkRegistry::new(&root);
    assert!(registry.install("nope").is_err());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn packaged_catalog_discovery_reaches_release_root_from_runtime_sidecar() {
    let executable = Path::new("C:/Loom/runtime/loom-daemon.exe");
    let roots = packaged_framework_catalog_roots(executable);

    assert_eq!(
        roots,
        vec![
            PathBuf::from("C:/Loom/runtime/packages/frameworks"),
            PathBuf::from("C:/Loom/packages/frameworks"),
        ]
    );
}

#[test]
fn local_framework_catalog_requires_matching_sha256_sidecar() {
    let root = temp_root();
    let catalog = root.join("catalog");
    std::fs::create_dir_all(&catalog).expect("catalog directory");
    let package_path = catalog.join("process.zip");
    let package = b"independent-framework-package";
    std::fs::write(&package_path, package).expect("framework package");
    let hash = format!("{:x}", Sha256::digest(package));
    std::fs::write(
        package_path.with_extension("zip.sha256"),
        format!("{hash}  process.zip\n"),
    )
    .expect("framework checksum");

    assert_eq!(
        read_framework_package_from_catalog("process", &package_path)
            .expect("verified local framework package"),
        package
    );

    std::fs::write(
        package_path.with_extension("zip.sha256"),
        format!("{}  process.zip\n", "0".repeat(64)),
    )
    .expect("tampered framework checksum");
    let error = read_framework_package_from_catalog("process", &package_path)
        .expect_err("checksum mismatch must fail");
    assert!(error.to_string().contains("checksum mismatch"));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn read_dependencies_defaults_framework_from_execution() {
    let tool = ToolDefinition {
        id: "art-a".to_owned(),
        name: "A".to_owned(),
        description: "d".to_owned(),
        enabled: true,
        execution: ToolExecution::FrameworkArt {
            framework: "process".to_owned(),
        },
        inputs: vec![],
        outputs: vec![],
        params: vec![],
        metadata: None,
    };
    let deps = read_dependencies(&tool);
    assert_eq!(deps.framework.as_deref(), Some("process"));
    assert!(deps.binaries.is_empty());
}

#[test]
fn read_dependencies_parses_metadata_manifest() {
    let tool = ToolDefinition {
        id: "art-b".to_owned(),
        name: "B".to_owned(),
        description: "d".to_owned(),
        enabled: true,
        execution: ToolExecution::FrameworkArt {
            framework: "process".to_owned(),
        },
        inputs: vec![],
        outputs: vec![],
        params: vec![],
        metadata: Some(serde_json::json!({
            "dependencies": {
                "framework": "process",
                "binaries": [{ "name": "pingo.exe", "sha256": "abc" }],
                "arts": ["dep-art-1"]
            }
        })),
    };
    let deps = read_dependencies(&tool);
    assert_eq!(deps.framework.as_deref(), Some("process"));
    assert_eq!(deps.binaries.len(), 1);
    assert_eq!(deps.binaries[0].name, "pingo.exe");
    assert_eq!(deps.arts, vec!["dep-art-1"]);
}
