use super::*;

#[test]
fn recursive_install_pulls_dependent_arts() {
    let root = temp_root();
    let framework = FrameworkRegistry::new(&root);
    install_test_framework(&framework, "workflow");
    install_test_framework(&framework, "cloud_api");
    let registry = ToolRegistry::new(root.join("tools"));

    // Root workflow art depends on dep-1; dep-1 depends on dep-2.
    let root_manifest = r#"{"id":"root-wf","name":"Root","description":"d","enabled":true,
            "execution":{"type":"workflow","workflowId":"wf"},
            "metadata":{"dependencies":{"framework":"workflow","arts":["dep-1"]}}}"#;
    let root_zip = build_zip(root_manifest, &[]);

    let dep1 = r#"{"id":"dep-1","name":"D1","description":"d","enabled":true,
            "execution":{"type":"cloud_api","endpoint":"https://x","method":"POST"},
            "metadata":{"dependencies":{"framework":"cloud_api","arts":["dep-2"]}}}"#;
    let dep2 = r#"{"id":"dep-2","name":"D2","description":"d","enabled":true,
            "execution":{"type":"cloud_api","endpoint":"https://y","method":"POST"}}"#;
    let dep1_zip = build_zip(dep1, &[]);
    let dep2_zip = build_zip(dep2, &[]);

    let fetch = |id: &str| -> Result<Vec<u8>, ArtInstallError> {
        match id {
            "dep-1" => Ok(dep1_zip.clone()),
            "dep-2" => Ok(dep2_zip.clone()),
            other => Err(ArtInstallError::InvalidPackage(format!("no art {other}"))),
        }
    };

    let reports =
        install_art_recursive(&root_zip, &root, &framework, &registry, &fetch).expect("recursive");
    let ids: Vec<&str> = reports.iter().map(|r| r.tool_id.as_str()).collect();
    assert_eq!(ids, vec!["root-wf", "dep-1", "dep-2"]);
    assert_eq!(reports[0].dependent_arts, vec!["dep-1"]);
    assert!(registry.get_tool("dep-2").unwrap().is_some());

    let root_activation =
        read_art_activation(&root.join("arts/publisher.test/root-wf/active.json"))
            .expect("root activation");
    let root_lock: PluginLockfile = serde_json::from_slice(
        &std::fs::read(&root_activation.active.lockfile).expect("root lockfile"),
    )
    .expect("parse root lockfile");
    let locked_dep = root_lock
        .resolved
        .iter()
        .find(|dependency| dependency.kind == "art")
        .expect("root child lock");
    let dep_activation = read_art_activation(&root.join("arts/publisher.test/dep-1/active.json"))
        .expect("dep activation");
    assert_eq!(locked_dep.id, "publisher.test/dep-1");
    assert_eq!(locked_dep.version, dep_activation.active.version);
    assert_eq!(locked_dep.sha256, dep_activation.active.digest);

    let dep_tool = registry.get_tool("dep-1").unwrap().unwrap();
    verify_art_package_integrity(&root, &dep_tool, &framework).expect("verify child graph");
    let root_tool = registry.get_tool("root-wf").unwrap().unwrap();
    verify_art_package_integrity(&root, &root_tool, &framework).expect("verify root graph");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn recursive_install_rolls_back_new_children_when_parent_fails() {
    let root = temp_root();
    let framework = FrameworkRegistry::new(&root);
    install_test_framework(&framework, "cloud_api");
    let registry = ToolRegistry::new(root.join("tools"));
    let parent = serde_json::json!({
        "id": "failing-parent",
        "name": "Failing Parent",
        "description": "missing workflow framework",
        "enabled": true,
        "execution": { "type": "workflow", "workflowId": "wf" },
        "metadata": {
            "dependencies": { "framework": "workflow", "arts": ["new-child"] }
        }
    });
    let child = serde_json::json!({
        "id": "new-child",
        "name": "New Child",
        "description": "child",
        "enabled": true,
        "execution": {
            "type": "cloud_api",
            "endpoint": "https://example.invalid/process",
            "method": "POST"
        }
    });
    let child_zip = build_zip(&child.to_string(), &[]);
    let error = install_art_recursive(
        &build_zip(&parent.to_string(), &[]),
        &root,
        &framework,
        &registry,
        &|id| {
            if id == "new-child" {
                Ok(child_zip.clone())
            } else {
                Err(ArtInstallError::InvalidPackage(format!("no Art `{id}`")))
            }
        },
    )
    .expect_err("parent framework failure must abort the graph install");
    assert!(matches!(error, ArtInstallError::FrameworkNotReady { .. }));
    assert!(registry.get_tool("new-child").unwrap().is_none());
    assert!(registry.get_tool("failing-parent").unwrap().is_none());
    assert!(!root.join("arts/publisher.test/new-child").exists());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn parent_art_lock_resolves_immutable_child_across_active_upgrade_and_rollback() {
    let root = temp_root();
    let framework = FrameworkRegistry::new(&root);
    install_test_framework(&framework, "workflow");
    install_test_framework(&framework, "cloud_api");
    let registry = ToolRegistry::new(root.join("tools"));

    let child_zip = |version: &str, payload: &'static [u8]| {
        let manifest = serde_json::json!({
            "id": "locked-child",
            "name": "Locked Child",
            "description": "child",
            "enabled": true,
            "execution": {
                "type": "cloud_api",
                "endpoint": "https://example.invalid/process",
                "method": "POST"
            },
            "metadata": {
                "packageSecurity": { "version": version }
            }
        });
        build_zip(&manifest.to_string(), &[("payload.bin", payload)])
    };
    let parent_zip = |version: &str| {
        let manifest = serde_json::json!({
            "id": "locked-parent",
            "name": "Locked Parent",
            "description": "parent",
            "enabled": true,
            "execution": { "type": "workflow", "workflowId": "locked-workflow" },
            "metadata": {
                "packageSecurity": { "version": version },
                "dependencies": {
                    "framework": "workflow",
                    "arts": ["locked-child"]
                }
            }
        });
        build_zip(&manifest.to_string(), &[])
    };

    install_art_from_zip(
        &child_zip("1.0.0", b"child-one"),
        &root,
        &framework,
        &registry,
    )
    .expect("install child v1");
    install_art_from_zip(&parent_zip("1.0.0"), &root, &framework, &registry)
        .expect("install parent locked to child v1");
    let parent_v1 = registry.get_tool("locked-parent").unwrap().unwrap();
    let parent_v1_version = parent_v1
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.pointer("/artPackage/version"))
        .and_then(serde_json::Value::as_str)
        .expect("parent v1 version")
        .to_owned();
    let parent_v1_digest = parent_v1
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.pointer("/artPackage/digest"))
        .and_then(serde_json::Value::as_str)
        .expect("parent v1 digest")
        .to_owned();
    verify_art_package_integrity(&root, &parent_v1, &framework).expect("verify parent v1");

    install_art_from_zip(
        &child_zip("2.0.0", b"child-two"),
        &root,
        &framework,
        &registry,
    )
    .expect("upgrade child");
    verify_art_package_integrity(&root, &parent_v1, &framework)
        .expect("parent v1 remains bound to installed child v1");
    let resolved_parent_v1 = resolve_installed_art_package(
        &root,
        "locked-parent",
        &parent_v1_version,
        &parent_v1_digest,
        &registry,
        &framework,
    )
    .expect("resolve immutable parent v1 after child upgrade");
    assert_eq!(
            resolved_parent_v1
                .metadata
                .as_ref()
                .and_then(|metadata| {
                    metadata
                        .pointer("/artPackage/lockedArts/publisher.test~1locked-child/metadata/artPackage/version")
                })
                .and_then(serde_json::Value::as_str),
            Some("1.0.0")
        );

    install_art_from_zip(&parent_zip("2.0.0"), &root, &framework, &registry)
        .expect("refresh parent lock for child v2");
    let parent_v2 = registry.get_tool("locked-parent").unwrap().unwrap();
    verify_art_package_integrity(&root, &parent_v2, &framework).expect("verify refreshed parent");

    rollback_art_package(&root, "locked-child", &registry, &framework)
        .expect("rollback child to v1");
    verify_art_package_integrity(&root, &parent_v2, &framework)
        .expect("parent v2 remains bound to installed child v2");
    let parent_rolled_back = rollback_art_package(&root, "locked-parent", &registry, &framework)
        .expect("rollback parent to lock matching child v1");
    verify_art_package_integrity(&root, &parent_rolled_back, &framework)
        .expect("verify rolled-back parent and child lock");

    let child_root = root.join("arts/publisher.test/locked-child");
    let child_activation =
        read_art_activation(&child_root.join("active.json")).expect("child activation");
    let child_dir = child_root.join(&child_activation.active.path);
    set_tree_readonly(&child_dir, false).expect("unlock child fixture");
    std::fs::write(child_dir.join("payload.bin"), b"tampered").expect("tamper child");
    assert!(verify_art_package_integrity(&root, &parent_rolled_back, &framework).is_err());
    std::fs::write(child_dir.join("payload.bin"), b"child-one").expect("restore child");

    let mut tampered_activation = child_activation.clone();
    tampered_activation.active.version = "9.9.9".to_owned();
    write_art_activation(&child_root.join("active.json"), &tampered_activation)
        .expect("tamper child activation");
    verify_art_package_integrity(&root, &parent_rolled_back, &framework)
        .expect("active pointer metadata does not change an immutable child lock");
    write_art_activation(&child_root.join("active.json"), &child_activation)
        .expect("restore child activation");

    uninstall_art_package(&root, "locked-child", &registry).expect("uninstall child");
    assert!(verify_art_package_integrity(&root, &parent_rolled_back, &framework).is_err());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn parent_art_lock_uses_publisher_qualified_child_identity() {
    let root = temp_root();
    let framework = FrameworkRegistry::new(&root);
    install_test_framework(&framework, "workflow");
    install_test_framework(&framework, "cloud_api");
    let registry = ToolRegistry::new(root.join("tools"));
    let child = serde_json::json!({
        "id": "shared-child",
        "name": "Qualified Child",
        "description": "child",
        "enabled": true,
        "execution": {
            "type": "cloud_api",
            "endpoint": "https://example.invalid/process",
            "method": "POST"
        },
        "metadata": {
            "packageSecurity": {
                "version": "1.0.0",
                "publisher": { "id": "publisher.alpha" }
            }
        }
    });
    install_art_from_zip(
        &build_zip(&child.to_string(), &[]),
        &root,
        &framework,
        &registry,
    )
    .expect("install qualified child");
    let parent = serde_json::json!({
        "id": "qualified-parent",
        "name": "Qualified Parent",
        "description": "parent",
        "enabled": true,
        "execution": { "type": "workflow", "workflowId": "qualified" },
        "metadata": {
            "packageSecurity": {
                "version": "1.0.0",
                "publisher": { "id": "publisher.parent" }
            },
            "dependencies": {
                "framework": "workflow",
                "arts": ["publisher.alpha/shared-child"]
            }
        }
    });
    install_art_from_zip(
        &build_zip(&parent.to_string(), &[]),
        &root,
        &framework,
        &registry,
    )
    .expect("install parent");

    let activation =
        read_art_activation(&root.join("arts/publisher.parent/qualified-parent/active.json"))
            .expect("parent activation");
    let lock: PluginLockfile = serde_json::from_slice(
        &std::fs::read(&activation.active.lockfile).expect("parent lockfile"),
    )
    .expect("parse parent lockfile");
    assert!(lock.resolved.iter().any(|dependency| {
        dependency.kind == "art" && dependency.id == "publisher.alpha/shared-child"
    }));
    let tool = registry
        .get_tool("publisher.parent/qualified-parent")
        .unwrap()
        .unwrap();
    verify_art_package_integrity(&root, &tool, &framework).expect("verify qualified lock");

    let mut bare_parent_lock = lock;
    bare_parent_lock.package_id = "qualified-parent".to_owned();
    std::fs::write(
        &activation.active.lockfile,
        serde_json::to_vec_pretty(&bare_parent_lock).unwrap(),
    )
    .expect("tamper parent lock identity");
    assert!(verify_art_package_integrity(&root, &tool, &framework).is_err());
    std::fs::remove_dir_all(&root).ok();
}
