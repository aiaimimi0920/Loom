use super::*;

#[cfg(unix)]
fn create_directory_link(target: &Path, link: &Path) {
    std::os::unix::fs::symlink(target, link).expect("create directory symlink");
}

#[cfg(windows)]
fn create_directory_link(target: &Path, link: &Path) {
    let status = std::process::Command::new("cmd.exe")
        .args(["/d", "/c", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .status()
        .expect("run mklink");
    assert!(status.success(), "create directory junction");
}

#[test]
fn tree_cleanup_rejects_filesystem_links_without_touching_the_target() {
    let root = temp_root();
    let outside = temp_root();
    let sentinel = outside.join("sentinel.txt");
    std::fs::write(&sentinel, b"keep writable").expect("write outside sentinel");
    std::fs::create_dir_all(&root).expect("create cleanup root");
    let link = root.join("linked-tree");
    create_directory_link(&outside, &link);

    let error = remove_tree(&link).expect_err("cleanup must reject directory links");

    assert!(matches!(error, ArtInstallError::InvalidPackage(_)));
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
fn art_recovery_restores_activation_and_rejects_unsafe_journal_paths() {
    let root = temp_root();
    let framework = FrameworkRegistry::new(&root);
    install_test_framework(&framework, "process");
    let registry = ToolRegistry::new(root.join("tools"));
    let manifest = r#"{"id":"recover-art","name":"Recover","description":"d","enabled":true,
            "execution":{"type":"framework_art","framework":"process"}}"#;
    install_art_from_zip(
        &build_zip(manifest, &[("bin/tool.exe", b"original")]),
        &root,
        &framework,
        &registry,
    )
    .expect("install Art");
    let art_root = root.join("arts/publisher.test/recover-art");
    let active_path = art_root.join("active.json");
    let old = read_art_activation(&active_path).unwrap();
    let orphan_relative = "versions/interrupted-orphan".to_owned();
    let orphan = art_root.join(&orphan_relative);
    std::fs::create_dir_all(&orphan).unwrap();
    std::fs::write(orphan.join("partial.bin"), b"partial").unwrap();
    let mut next_pointer = old.active.clone();
    next_pointer.path = orphan_relative.clone();
    write_art_lifecycle(
        &art_root,
        &ArtLifecycleJournal {
            old_activation: Some(old.clone()),
            next_activation: ArtActivationState {
                active: next_pointer,
                previous: Some(old.active.clone()),
                local_authoring: old.local_authoring,
                bundled_catalog: old.bundled_catalog,
            },
            target: orphan_relative,
            created_target: true,
        },
    )
    .unwrap();
    recover_art_lifecycle(&root).expect("recover interrupted Art");
    assert_eq!(read_art_activation(&active_path), Some(old));
    assert!(!orphan.exists());

    let outside = root.join("outside.txt");
    std::fs::write(&outside, b"keep").unwrap();
    let unsafe_journal = serde_json::json!({
        "oldActivation": null,
        "nextActivation": {
            "active": {
                "path": "../../outside.txt",
                "version": "0.0.0",
                "digest": "deadbeef",
                "lockfile": "outside.json"
            },
            "previous": null
        },
        "target": "../../outside.txt"
    });
    std::fs::write(
        art_root.join(ART_LIFECYCLE_FILE),
        serde_json::to_vec(&unsafe_journal).unwrap(),
    )
    .unwrap();
    recover_art_lifecycle(&root).expect("quarantine unsafe journal");
    assert_eq!(std::fs::read(&outside).unwrap(), b"keep");
    assert!(art_root.join("lifecycle.corrupt").is_file());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn art_recovery_keeps_a_version_the_interrupted_operation_did_not_create() {
    let root = temp_root();
    let framework = FrameworkRegistry::new(&root);
    install_test_framework(&framework, "process");
    let registry = ToolRegistry::new(root.join("tools"));
    let manifest = r#"{"id":"keep-art","name":"Keep","description":"d","enabled":true,
            "execution":{"type":"framework_art","framework":"process"},
            "metadata":{"packageSecurity":{"version":"1.0.0"}}}"#;
    install_art_from_zip(
        &build_zip(manifest, &[("bin/tool.exe", b"version-one")]),
        &root,
        &framework,
        &registry,
    )
    .expect("install first Art");
    install_art_from_zip(
        &build_zip(manifest, &[("bin/tool.exe", b"version-two")]),
        &root,
        &framework,
        &registry,
    )
    .expect("install second Art");
    let art_root = root.join("arts/publisher.test/keep-art");
    let active_path = art_root.join("active.json");
    let live = read_art_activation(&active_path).expect("activation");
    let older = live.previous.clone().expect("previous version");
    let older_dir = art_root.join(&older.path);
    assert!(older_dir.is_dir());

    // A rollback interrupted between its journal write and its activation write. The journal
    // names the older version, which has been on disk since it was installed.
    write_art_lifecycle(
        &art_root,
        &ArtLifecycleJournal {
            old_activation: Some(live.clone()),
            next_activation: ArtActivationState {
                active: older.clone(),
                previous: Some(live.active.clone()),
                local_authoring: live.local_authoring,
                bundled_catalog: live.bundled_catalog,
            },
            target: older.path.clone(),
            created_target: false,
        },
    )
    .expect("write lifecycle journal");

    recover_art_lifecycle(&root).expect("recover interrupted rollback");
    assert_eq!(read_art_activation(&active_path), Some(live));
    assert!(
        older_dir.is_dir(),
        "recovery deleted a version it did not create"
    );
    assert!(!art_root.join(ART_LIFECYCLE_FILE).exists());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_failed_art_activation_write_leaves_no_lifecycle_journal_behind() {
    let root = temp_root();
    let framework = FrameworkRegistry::new(&root);
    install_test_framework(&framework, "process");
    let registry = ToolRegistry::new(root.join("tools"));
    let manifest = r#"{"id":"journal-art","name":"Journal","description":"d","enabled":true,
            "execution":{"type":"framework_art","framework":"process"},
            "metadata":{"packageSecurity":{"version":"1.0.0"}}}"#;
    install_art_from_zip(
        &build_zip(manifest, &[("bin/tool.exe", b"version-one")]),
        &root,
        &framework,
        &registry,
    )
    .expect("install first Art");
    install_art_from_zip(
        &build_zip(manifest, &[("bin/tool.exe", b"version-two")]),
        &root,
        &framework,
        &registry,
    )
    .expect("install second Art");
    let art_root = root.join("arts/publisher.test/journal-art");
    let active_path = art_root.join("active.json");
    let live = read_art_activation(&active_path).expect("activation");
    let older_dir = art_root.join(&live.previous.as_ref().expect("previous version").path);

    // Occupy the staging path the activation write needs, so that write fails after the journal
    // has already been written.
    std::fs::create_dir_all(art_root.join("active.json.tmp")).expect("block staging path");
    // A sentinel journal, so that "no journal on disk" can only mean the rollback reached its
    // own journal write and then cleaned up — not that it failed before ever journalling.
    write_art_lifecycle(
        &art_root,
        &ArtLifecycleJournal {
            old_activation: None,
            next_activation: live.clone(),
            target: "versions/sentinel".to_owned(),
            created_target: false,
        },
    )
    .expect("write sentinel journal");
    assert!(rollback_art_package(&root, "journal-art", &registry, &framework).is_err());
    assert!(
        !art_root.join(ART_LIFECYCLE_FILE).exists(),
        "a rollback that never activated left its journal behind"
    );
    assert_eq!(read_art_activation(&active_path), Some(live));
    assert!(older_dir.is_dir());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn art_rollback_rejects_unsafe_previous_pointer() {
    let root = temp_root();
    let framework = FrameworkRegistry::new(&root);
    install_test_framework(&framework, "process");
    let registry = ToolRegistry::new(root.join("tools"));
    let manifest = r#"{"id":"unsafe-rollback-art","name":"Unsafe","description":"d","enabled":true,
            "execution":{"type":"framework_art","framework":"process"}}"#;
    install_art_from_zip(
        &build_zip(manifest, &[("bin/tool.exe", b"one")]),
        &root,
        &framework,
        &registry,
    )
    .unwrap();
    install_art_from_zip(
        &build_zip(manifest, &[("bin/tool.exe", b"two")]),
        &root,
        &framework,
        &registry,
    )
    .unwrap();
    let art_root = root.join("arts/publisher.test/unsafe-rollback-art");
    let active_path = art_root.join("active.json");
    let mut activation = read_art_activation(&active_path).unwrap();
    activation.previous.as_mut().unwrap().path = "../../outside".to_owned();
    write_art_activation(&active_path, &activation).unwrap();
    let error = rollback_art_package(&root, "unsafe-rollback-art", &registry, &framework)
        .expect_err("unsafe previous pointer must be rejected");
    assert!(matches!(error, ArtInstallError::InvalidPackage(_)));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn art_version_retention_keeps_active_previous_and_writable_state() {
    let root = temp_root();
    let framework = FrameworkRegistry::new(&root);
    install_test_framework(&framework, "process");
    let registry = ToolRegistry::new(root.join("tools"));
    for (version, payload) in [
        ("1.0.0", b"one".as_slice()),
        ("2.0.0", b"two".as_slice()),
        ("3.0.0", b"three".as_slice()),
        ("4.0.0", b"four".as_slice()),
        ("5.0.0", b"five".as_slice()),
    ] {
        let manifest = serde_json::json!({
            "id": "retained-art",
            "name": "Retained",
            "description": "retention",
            "enabled": true,
            "execution": { "type": "framework_art", "framework": "process" },
            "metadata": { "packageSecurity": { "version": version } }
        });
        install_art_from_zip(
            &build_zip(&manifest.to_string(), &[("bin/tool.exe", payload)]),
            &root,
            &framework,
            &registry,
        )
        .unwrap_or_else(|error| panic!("install {version}: {error}"));
    }
    let art_root = root.join("arts/publisher.test/retained-art");
    let activation = read_art_activation(&art_root.join("active.json")).expect("activation");
    let versions = std::fs::read_dir(art_root.join("versions"))
        .expect("versions")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    assert!(versions.len() <= art_history_limit());
    assert!(art_root.join(&activation.active.path).is_dir());
    assert!(art_root
        .join(activation.previous.expect("previous version").path)
        .is_dir());
    assert!(
        std::fs::metadata(art_root.join(&activation.active.path).join("bin/tool.exe"))
            .expect("code metadata")
            .permissions()
            .readonly()
    );
    for writable in ["state", "cache", "outputs"] {
        assert!(art_root.join(writable).is_dir());
        assert!(
            !std::fs::metadata(art_root.join(writable))
                .expect("state metadata")
                .permissions()
                .readonly(),
            "{writable} must remain writable"
        );
    }
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn art_uninstall_tombstone_recovery_restores_or_finishes_from_registry_state() {
    let root = temp_root();
    let framework = FrameworkRegistry::new(&root);
    install_test_framework(&framework, "process");
    let registry = ToolRegistry::new(root.join("tools"));
    let manifest = r#"{"id":"recover-uninstall-art","name":"Recover","description":"d","enabled":true,
            "execution":{"type":"framework_art","framework":"process"}}"#;
    install_art_from_zip(
        &build_zip(manifest, &[("bin/tool.exe", b"payload")]),
        &root,
        &framework,
        &registry,
    )
    .expect("install Art");
    let live = root.join("arts/publisher.test/recover-uninstall-art");
    let interrupted = uninstall_tombstone_path(&live, ART_UNINSTALL_TOMBSTONE_PREFIX).unwrap();
    std::fs::rename(&live, &interrupted).expect("simulate pre-registry crash");
    recover_art_uninstall_tombstones(&root).expect("restore tombstone");
    assert!(live.is_dir());
    assert!(!interrupted.exists());

    let committed = uninstall_tombstone_path(&live, ART_UNINSTALL_TOMBSTONE_PREFIX).unwrap();
    std::fs::rename(&live, &committed).expect("simulate committed uninstall");
    registry
        .delete_tool("recover-uninstall-art")
        .expect("commit registry removal");
    recover_art_uninstall_tombstones(&root).expect("finish tombstone deletion");
    assert!(!live.exists());
    assert!(!committed.exists());
    std::fs::remove_dir_all(&root).ok();
}
