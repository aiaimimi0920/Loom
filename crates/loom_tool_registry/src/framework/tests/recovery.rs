//! Interrupted lifecycle and tombstone recovery tests.
use super::*;

#[test]
fn framework_tree_cleanup_rejects_links_without_touching_the_target() {
    let root = temp_root();
    let outside = temp_root();
    let sentinel = outside.join("sentinel.txt");
    std::fs::write(&sentinel, b"keep writable").expect("write outside sentinel");
    let link = root.join("linked-tree");
    create_directory_link(&outside, &link);

    let error = remove_framework_tree(&link).expect_err("cleanup must reject directory links");

    assert!(matches!(error, FrameworkError::InvalidPackage { .. }));
    assert_eq!(
        std::fs::read(&sentinel).expect("read sentinel"),
        b"keep writable"
    );
    std::fs::write(&sentinel, b"still writable").expect("rewrite sentinel");
    #[cfg(windows)]
    std::fs::remove_dir(&link).expect("remove junction");
    #[cfg(unix)]
    std::fs::remove_file(&link).expect("remove symlink");
    std::fs::remove_dir_all(&root).ok();
    std::fs::remove_dir_all(&outside).ok();
}

#[test]
fn framework_recovery_restores_previous_activation_and_removes_orphan_target() {
    let root = temp_root();
    let registry = FrameworkRegistry::new(&root);
    registry
        .install_framework_package_from_zip(&fake_framework_package_zip("process"))
        .expect("install framework");
    let package_root = root
        .join(FRAMEWORK_PACKAGES_DIR)
        .join("publisher.test")
        .join("process");
    let old = registry.activation("process").expect("activation");
    let orphan_relative = "versions/interrupted-orphan".to_owned();
    let orphan = package_root.join(&orphan_relative);
    std::fs::create_dir_all(&orphan).expect("orphan target");
    std::fs::write(orphan.join("partial.bin"), b"partial").expect("partial payload");
    registry
        .write_lifecycle_journal(
            "process",
            &FrameworkLifecycleJournal {
                old_activation: Some(old.clone()),
                next_activation: FrameworkActivationState {
                    active: orphan_relative.clone(),
                    previous: Some(old.active.clone()),
                },
                target: orphan_relative,
                created_target: true,
            },
        )
        .expect("write lifecycle journal");

    let recovered = FrameworkRegistry::new(&root);
    assert_eq!(recovered.activation("process"), Some(old));
    assert!(!orphan.exists());
    assert!(!package_root.join(FRAMEWORK_LIFECYCLE_FILE).exists());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn framework_recovery_keeps_a_version_the_interrupted_operation_did_not_create() {
    let root = temp_root();
    let registry = FrameworkRegistry::new(&root);
    registry
        .install_framework_package_from_zip(&fake_framework_package_zip_with_version(
            "process", "1.0.0",
        ))
        .expect("install v1");
    registry
        .install_framework_package_from_zip(&fake_framework_package_zip_with_version(
            "process", "2.0.0",
        ))
        .expect("install v2");
    let package_root = root
        .join(FRAMEWORK_PACKAGES_DIR)
        .join("publisher.test")
        .join("process");
    let live = registry.activation("process").expect("activation");
    let older_relative = live.previous.clone().expect("previous version");
    let older = package_root.join(&older_relative);
    assert!(older.is_dir());

    // A rollback interrupted between its journal write and its activation write. The journal
    // names the older version, which has been on disk since it was installed.
    registry
        .write_lifecycle_journal(
            "process",
            &FrameworkLifecycleJournal {
                old_activation: Some(live.clone()),
                next_activation: FrameworkActivationState {
                    active: older_relative.clone(),
                    previous: Some(live.active.clone()),
                },
                target: older_relative,
                created_target: false,
            },
        )
        .expect("write lifecycle journal");

    let recovered = FrameworkRegistry::new(&root);
    assert_eq!(recovered.activation("process"), Some(live));
    assert!(
        older.is_dir(),
        "recovery deleted a version it did not create"
    );
    assert!(!package_root.join(FRAMEWORK_LIFECYCLE_FILE).exists());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_failed_rollback_activation_leaves_no_lifecycle_journal_behind() {
    let root = temp_root();
    let registry = FrameworkRegistry::new(&root);
    registry
        .install_framework_package_from_zip(&fake_framework_package_zip_with_version(
            "process", "1.0.0",
        ))
        .expect("install v1");
    registry
        .install_framework_package_from_zip(&fake_framework_package_zip_with_version(
            "process", "2.0.0",
        ))
        .expect("install v2");
    let package_root = root
        .join(FRAMEWORK_PACKAGES_DIR)
        .join("publisher.test")
        .join("process");
    let live = registry.activation("process").expect("activation");
    let older = package_root.join(live.previous.clone().expect("previous version"));

    // Occupy the staging path the activation write needs, so that write fails after the
    // journal has already been written.
    std::fs::create_dir_all(package_root.join("active.json.tmp")).expect("block staging path");
    // A sentinel journal, so that "no journal on disk" can only mean the rollback reached its
    // own journal write and then cleaned up — not that it failed before ever journalling.
    registry
        .write_lifecycle_journal(
            "process",
            &FrameworkLifecycleJournal {
                old_activation: None,
                next_activation: FrameworkActivationState {
                    active: "versions/sentinel".to_owned(),
                    previous: None,
                },
                target: "versions/sentinel".to_owned(),
                created_target: false,
            },
        )
        .expect("write sentinel journal");
    assert!(registry.rollback("process").is_err());
    assert!(
        !package_root.join(FRAMEWORK_LIFECYCLE_FILE).exists(),
        "a rollback that never activated left its journal behind"
    );
    assert_eq!(registry.activation("process"), Some(live));
    assert!(older.is_dir());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn framework_recovery_quarantines_unsafe_journal_paths() {
    let root = temp_root();
    let package_root = root
        .join(FRAMEWORK_PACKAGES_DIR)
        .join("publisher.test")
        .join("process");
    std::fs::create_dir_all(&package_root).expect("package root");
    let outside = root.join("outside.txt");
    std::fs::write(&outside, b"keep").expect("outside sentinel");
    let journal = serde_json::json!({
        "oldActivation": null,
        "nextActivation": { "active": "../../outside.txt" },
        "target": "../../outside.txt"
    });
    std::fs::write(
        package_root.join(FRAMEWORK_LIFECYCLE_FILE),
        serde_json::to_vec(&journal).unwrap(),
    )
    .expect("unsafe journal");

    let _ = FrameworkRegistry::new(&root);
    assert_eq!(std::fs::read(&outside).unwrap(), b"keep");
    assert!(package_root.join("lifecycle.corrupt").is_file());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn framework_readiness_rejects_tampered_lockfile() {
    let root = temp_root();
    let registry = FrameworkRegistry::new(&root);
    registry
        .install_framework_package_from_zip(&fake_framework_package_zip("process"))
        .expect("install framework");
    let locks = root
        .join(FRAMEWORK_PACKAGES_DIR)
        .join("publisher.test")
        .join("process")
        .join("locks");
    let lockfile = std::fs::read_dir(&locks)
        .expect("locks")
        .next()
        .expect("lockfile")
        .expect("lock entry")
        .path();
    let mut lock: loom_protocol::PluginLockfile =
        serde_json::from_slice(&std::fs::read(&lockfile).unwrap()).unwrap();
    lock.package_id = "other-framework".to_owned();
    std::fs::write(&lockfile, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let (ready, detail) = registry.readiness("process");
    assert!(!ready);
    assert!(detail.contains("锁文件"), "detail={detail}");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn framework_version_retention_keeps_active_previous_and_history_limit() {
    let root = temp_root();
    let registry = FrameworkRegistry::new(&root);
    for version in ["0.1.0", "0.2.0", "0.3.0", "0.4.0", "0.5.0"] {
        registry
            .install_framework_package_from_zip(&fake_framework_package_zip_with_version(
                "process", version,
            ))
            .unwrap_or_else(|error| panic!("install {version}: {error}"));
    }
    let package_root = root
        .join(FRAMEWORK_PACKAGES_DIR)
        .join("publisher.test")
        .join("process");
    let activation = registry.activation("process").expect("activation");
    let versions = std::fs::read_dir(package_root.join(FRAMEWORK_VERSIONS_DIR))
        .expect("versions")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    assert!(versions.len() <= framework_history_limit());
    assert!(package_root.join(&activation.active).is_dir());
    assert!(package_root
        .join(activation.previous.expect("previous version"))
        .is_dir());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn framework_uninstall_tombstone_recovery_restores_or_finishes_from_registry_state() {
    let root = temp_root();
    let registry = FrameworkRegistry::new(&root);
    registry
        .install_framework_package_from_zip(&fake_framework_package_zip("process"))
        .expect("install framework");
    let live = root
        .join(FRAMEWORK_PACKAGES_DIR)
        .join("publisher.test")
        .join("process");
    let interrupted = uninstall_tombstone_path(&live, FRAMEWORK_UNINSTALL_TOMBSTONE_PREFIX)
        .expect("tombstone path");
    std::fs::rename(&live, &interrupted).expect("simulate pre-state crash");

    let recovered = FrameworkRegistry::new(&root);
    assert!(recovered.is_installed("process"));
    assert!(live.is_dir());
    assert!(!interrupted.exists());

    let committed = uninstall_tombstone_path(&live, FRAMEWORK_UNINSTALL_TOMBSTONE_PREFIX)
        .expect("tombstone path");
    std::fs::rename(&live, &committed).expect("simulate committed uninstall");
    recovered
        .write_installed(&BTreeMap::new())
        .expect("commit registry removal");
    let finished = FrameworkRegistry::new(&root);
    assert!(!finished.is_installed("process"));
    assert!(!live.exists());
    assert!(!committed.exists());
    std::fs::remove_dir_all(&root).ok();
}
