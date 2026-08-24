//! Strict-policy, package resolution, and durable-state tests.
use super::*;

#[test]
fn flat_framework_directory_is_not_resolved() {
    let root = temp_root();
    let flat = root.join("frameworks").join("flat-framework");
    let active = flat.join("versions").join("1.0.0-flat");
    std::fs::create_dir_all(&active).expect("flat framework directory");
    std::fs::write(
        flat.join(FRAMEWORK_ACTIVE_FILE),
        serde_json::to_vec(&FrameworkActivationState {
            active: "versions/1.0.0-flat".to_owned(),
            previous: None,
        })
        .unwrap(),
    )
    .expect("flat activation");
    std::fs::write(active.join(FRAMEWORK_MANIFEST_FILE), b"{}").expect("flat manifest");

    assert!(resolve_framework_package_dir(&root.join("frameworks"), "flat-framework").is_err());
    std::fs::remove_dir_all(root).ok();
}

/// Writes a package directory whose manifest is unreadable, under a publisher name that sorts
/// before the healthy publishers a test installs, so a scan that stopped at the first failure
/// would stop before reaching them.
fn write_damaged_framework_package(packages_root: &Path, id: &str) {
    let broken = packages_root.join("aaa.broken.publisher").join(id);
    let version = broken.join("versions").join("0.0.1");
    std::fs::create_dir_all(&version).expect("damaged package directory");
    std::fs::write(
        broken.join(FRAMEWORK_ACTIVE_FILE),
        serde_json::to_vec(&FrameworkActivationState {
            active: "versions/0.0.1".to_owned(),
            previous: None,
        })
        .unwrap(),
    )
    .expect("damaged activation");
    std::fs::write(version.join(FRAMEWORK_MANIFEST_FILE), b"{ truncated")
        .expect("damaged manifest");
}

#[test]
fn a_damaged_framework_package_does_not_hide_a_healthy_one() {
    let root = temp_root();
    let registry = FrameworkRegistry::new(&root);
    let id = "resolver-framework";
    registry
        .install_framework_package_from_zip(&fake_framework_package_zip_with_identity(
            id,
            "0.1.0",
            Some("publisher.alpha"),
        ))
        .expect("install the healthy package");
    let packages_root = root.join(FRAMEWORK_PACKAGES_DIR);
    write_damaged_framework_package(&packages_root, id);

    let resolved = resolve_framework_package_dir(&packages_root, id)
        .expect("the healthy package must still resolve");
    assert!(
        resolved.starts_with(packages_root.join("publisher.alpha")),
        "expected the intact publisher's package, got {}",
        resolved.display()
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_local_id_shipped_by_two_publishers_resolves_as_ambiguous_not_missing() {
    let root = temp_root();
    let registry = FrameworkRegistry::new(&root);
    let id = "contested-framework";
    for publisher in ["publisher.alpha", "publisher.beta"] {
        registry
            .install_framework_package_from_zip(&fake_framework_package_zip_with_identity(
                id,
                "0.1.0",
                Some(publisher),
            ))
            .expect("install a competing package");
    }

    let error = resolve_framework_package_dir(&root.join(FRAMEWORK_PACKAGES_DIR), id)
        .expect_err("a bare id carried by two publishers cannot resolve");
    assert!(
        matches!(&error, FrameworkError::AmbiguousFramework(reported) if reported == id),
        "expected an ambiguity error, got {error:?}"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn the_persisted_trust_policy_blocks_framework_readiness_and_installs() {
    let root = temp_root();
    let registry = FrameworkRegistry::new(&root);
    let id = "policy-framework";
    registry
        .install_framework_package_from_zip(&fake_framework_package_zip_with_identity(
            id,
            "0.1.0",
            Some("publisher.alpha"),
        ))
        .expect("install while unsigned packages are still allowed");
    let packages_root = root.join(FRAMEWORK_PACKAGES_DIR);
    let (ready, _) = framework_ready_in("publisher.alpha/policy-framework", Some(&packages_root));
    assert!(
        ready,
        "the package must be runnable under the default policy"
    );

    registry
        .set_trust_policy(TrustPolicy::RequireTrusted)
        .expect("persist the strict policy");

    // The policy lives in the trust store, not in the environment: an operator who requires
    // trusted packages must not have frameworks keep executing unsigned code.
    let (ready, reason) =
        framework_ready_in("publisher.alpha/policy-framework", Some(&packages_root));
    assert!(!ready, "readiness must honour the persisted policy");
    assert!(
        reason.contains("信任策略"),
        "expected a trust policy refusal, got {reason}"
    );
    let error = registry
        .install_framework_package_from_zip(&fake_framework_package_zip_with_identity(
            "second-policy-framework",
            "0.1.0",
            Some("publisher.beta"),
        ))
        .expect_err("installing an unsigned package must be refused too");
    assert!(
        matches!(error, FrameworkError::Security(_)),
        "expected a trust policy refusal from the installer, got {error:?}"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_corrupt_state_file_is_reported_and_never_silently_rewritten() {
    let root = temp_root();
    let registry = FrameworkRegistry::new(&root);
    let id = "corrupt-state-framework";
    registry
        .install_framework_package_from_zip(&fake_framework_package_zip_with_identity(
            id,
            "0.1.0",
            Some("publisher.alpha"),
        ))
        .expect("install a healthy package first");
    let qualified = format!("publisher.alpha/{id}");
    let state_path = root.join(FRAMEWORKS_FILE);
    let damaged = "{ \"publisher.alpha/corrupt-state-framework\": ";
    std::fs::write(&state_path, damaged).expect("damage the state file");

    // Not "未安装": the packages are intact on disk and the operator has to be told which file
    // to repair.
    let (ready, reason) = registry.readiness(&qualified);
    assert!(
        !ready,
        "a corrupt state file cannot report a ready framework"
    );
    assert!(
        reason.contains(FRAMEWORKS_FILE),
        "expected the corrupt state file to be named, got {reason}"
    );

    for (label, result) in [
        ("disable", registry.set_enabled(&qualified, false)),
        ("uninstall", registry.uninstall(&qualified)),
    ] {
        let error = result.expect_err("a mutating call must refuse a corrupt state file");
        assert!(
            matches!(error, FrameworkError::CorruptState { .. }),
            "expected {label} to report the corrupt state, got {error:?}"
        );
    }
    let installed = registry
        .install_framework_package_from_zip(&fake_framework_package_zip_with_identity(
            "second-corrupt-state-framework",
            "0.1.0",
            Some("publisher.beta"),
        ))
        .expect_err("an install must not rewrite a state file it could not read");
    assert!(
        matches!(installed, FrameworkError::CorruptState { .. }),
        "expected the installer to report the corrupt state, got {installed:?}"
    );

    assert_eq!(
        std::fs::read_to_string(&state_path).expect("read the state file back"),
        damaged,
        "the damaged file must survive verbatim so it can be repaired"
    );
    assert!(
        root.join(FRAMEWORK_PACKAGES_DIR)
            .join("publisher.alpha")
            .join(id)
            .is_dir(),
        "the installed package must still be on disk"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn invalid_persisted_state_keys_are_rejected_without_path_escape() {
    let root = temp_root();
    let state_path = root.join(FRAMEWORKS_FILE);
    std::fs::write(
        &state_path,
        serde_json::to_vec(&serde_json::json!({
            "../outside": { "version": "1.0.0", "enabled": true }
        }))
        .unwrap(),
    )
    .expect("write invalid state");
    let registry = FrameworkRegistry::new(&root);

    let error = registry
        .installation_states()
        .expect_err("unsafe state keys must be corrupt state");

    assert!(matches!(error, FrameworkError::CorruptState { .. }));
    assert_eq!(
        registry.runtime_dir("../outside"),
        root.join(FRAMEWORK_PACKAGES_DIR)
            .join(".unresolved")
            .join("invalid")
    );
    assert!(!root.join("outside").exists());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn bounded_framework_metadata_reads_reject_oversized_files() {
    let root = temp_root();
    let metadata = root.join("oversized.json");
    std::fs::write(&metadata, vec![b'x'; 17]).expect("write metadata");

    let error = read_bounded_file(&metadata, 16).expect_err("metadata must stay bounded");

    assert_eq!(error.kind(), ErrorKind::InvalidData);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_corrupt_state_file_leaves_an_interrupted_uninstall_recoverable() {
    let root = temp_root();
    let registry = FrameworkRegistry::new(&root);
    let id = "tombstone-framework";
    registry
        .install_framework_package_from_zip(&fake_framework_package_zip_with_identity(
            id,
            "0.1.0",
            Some("publisher.alpha"),
        ))
        .expect("install a healthy package first");
    let state_path = root.join(FRAMEWORKS_FILE);
    let intact = std::fs::read_to_string(&state_path).expect("read the healthy state file");

    // Stage the state a crash mid-uninstall leaves behind: the package is in a tombstone and
    // the state file still lists it, so recovery must put it back.
    let publisher_root = root.join(FRAMEWORK_PACKAGES_DIR).join("publisher.alpha");
    let live = publisher_root.join(id);
    let tombstone = publisher_root.join(format!("{FRAMEWORK_UNINSTALL_TOMBSTONE_PREFIX}{id}--1"));
    std::fs::rename(&live, &tombstone).expect("stage the tombstone");
    std::fs::write(&state_path, "{ truncated").expect("damage the state file");

    let _ = FrameworkRegistry::new(&root);
    assert!(
        tombstone.is_dir(),
        "recovery read an unusable state file, so it must not decide to delete"
    );
    assert!(!live.exists(), "nothing should have been restored yet");

    std::fs::write(&state_path, intact).expect("repair the state file");
    let _ = FrameworkRegistry::new(&root);
    assert!(
        live.is_dir(),
        "once the state file is readable the package must be restored"
    );
    assert!(!tombstone.exists(), "the tombstone must be consumed");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn permission_modes_audit_by_default_and_strictly_reject_unenforced_capabilities() {
    assert_eq!(
        parse_plugin_permission_mode(None).unwrap(),
        PluginPermissionMode::Audit
    );
    assert_eq!(
        parse_plugin_permission_mode(Some("strict")).unwrap(),
        PluginPermissionMode::Strict
    );
    assert!(parse_plugin_permission_mode(Some("permissive")).is_err());
    let manifest: FrameworkPackageManifest = serde_json::from_value(serde_json::json!({
        "id": "permission-test",
        "name": "Permission Test",
        "description": "permission fixture",
        "version": "1.0.0",
        "publisher": { "id": "publisher.test" },
        "protocolVersion": FRAMEWORK_PROTOCOL_VERSION,
        "platforms": [WINDOWS_X64_PLATFORM],
        "entry": {
            "kind": "process",
            "command": "runtime.exe",
            "args": [],
            "processModel": "per_execution"
        },
        "permissions": ["network.connect", "file.read", "process.spawn"],
        "permissionPolicy": {
            "network": { "domains": ["example.com"] },
            "filesystem": { "read": ["inputs"], "write": ["outputs"] },
            "process": { "spawn": true, "maxProcesses": 2 },
            "gpu": true,
            "clipboard": true
        },
        "artExecution": {
            "requestSchema": "loom.art.execute.v1",
            "responseSchema": "loom.art.result.v1"
        }
    }))
    .unwrap();
    assert_eq!(
        unsupported_permission_findings(&manifest),
        vec!["direct_network", "arbitrary_filesystem", "gpu", "clipboard"]
    );
    assert!(enforce_framework_permission_mode(&manifest, PluginPermissionMode::Audit).is_ok());
    assert!(enforce_framework_permission_mode(&manifest, PluginPermissionMode::Strict).is_err());
}
