use std::fs;

use loom_plugin_security::{generate_signing_key, TrustPolicy};
use loom_protocol::PackageTrustStatus;
use serde_json::json;

use super::*;

mod fixtures;
mod runtime_integrity_tests;

use fixtures::*;

#[test]
fn installs_trusted_package_disabled_and_reuses_identical_version() {
    let root = temp_root("trusted");
    let key = generate_signing_key("release-1");
    write_trust_store(&root, &key, TrustPolicy::RequireTrusted);
    let registry = CapabilityPluginRegistry::new(&root);
    let package = signed_package(&root, &key, manifest("capability", "1.0.0"), b"runtime");

    let first = install_capability_from_zip(&package, &registry).expect("first install");
    let second = install_capability_from_zip(&package, &registry).expect("idempotent install");

    assert_eq!(first.qualified_id, "publisher.example/text-tools");
    assert_eq!(first.digest, second.digest);
    assert_eq!(first.trust_status, PackageTrustStatus::Trusted);
    assert!(first.package_dir.join("runtime/text-tools.exe").is_file());
    assert!(
        fs::metadata(first.package_dir.join("capability.manifest.json"))
            .expect("manifest metadata")
            .permissions()
            .readonly()
    );
    let records = registry.list().expect("registry list");
    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0].status,
        CapabilityLifecycleStatus::InstalledDisabled
    );
    assert!(!records[0].enabled_intent);
    assert_eq!(records[0].versions.len(), 1);
    cleanup(&root);
}

#[test]
fn rejects_type_confusion_and_removes_partial_staging_tree() {
    let root = temp_root("type-confusion");
    let key = generate_signing_key("release-1");
    write_trust_store(&root, &key, TrustPolicy::RequireTrusted);
    let registry = CapabilityPluginRegistry::new(&root);
    let package = signed_package(&root, &key, manifest("art", "1.0.0"), b"runtime");

    let error = install_capability_from_zip(&package, &registry).expect_err("reject Art package");

    assert!(error.to_string().contains("kind must be `capability`"));
    assert!(registry.list().expect("registry list").is_empty());
    assert_staging_empty(&registry);
    cleanup(&root);
}

#[test]
fn rejects_tampered_signature_without_registry_or_immutable_target() {
    let root = temp_root("tampered");
    let key = generate_signing_key("release-1");
    write_trust_store(&root, &key, TrustPolicy::RequireTrusted);
    let registry = CapabilityPluginRegistry::new(&root);
    let package = signed_package(&root, &key, manifest("capability", "1.0.0"), b"one");
    let package = replace_zip_entry(&package, "runtime/text-tools.exe", b"two");

    let error = install_capability_from_zip(&package, &registry).expect_err("tamper rejected");

    assert!(error.to_string().contains("digest"));
    assert!(registry.list().expect("registry list").is_empty());
    assert!(!registry
        .packages_root()
        .join("publisher.example/text-tools/versions")
        .exists());
    assert_staging_empty(&registry);
    cleanup(&root);
}

#[test]
fn lifecycle_upgrade_rollback_disable_and_uninstall_are_registry_driven() {
    let root = temp_root("lifecycle");
    let key = generate_signing_key("release-1");
    write_trust_store(&root, &key, TrustPolicy::RequireTrusted);
    let registry = CapabilityPluginRegistry::new(&root);
    let grants = CapabilityGrantStore::new(&root);
    let config = CapabilityConfigStore::new(&root);
    let v1 = signed_package(&root, &key, manifest("capability", "1.0.0"), b"one");
    let v1 = install_capability_from_zip(&v1, &registry).expect("install v1");
    let active = registry
        .enable(&grants, &v1.qualified_id, Some(&v1.digest))
        .expect("enable v1");
    assert_eq!(active.active_digest.as_deref(), Some(v1.digest.as_str()));

    let v2 = signed_package(&root, &key, manifest("capability", "2.0.0"), b"two");
    let v2 = install_capability_from_zip(&v2, &registry).expect("install v2");
    let upgraded = registry
        .upgrade(&grants, &v2.qualified_id, &v2.digest)
        .expect("upgrade");
    assert_eq!(upgraded.active_digest.as_deref(), Some(v2.digest.as_str()));
    assert_eq!(
        upgraded.previous_digest.as_deref(),
        Some(v1.digest.as_str())
    );

    let rolled_back = registry
        .rollback(&grants, &v1.qualified_id)
        .expect("rollback");
    assert_eq!(
        rolled_back.active_digest.as_deref(),
        Some(v1.digest.as_str())
    );
    let disabled = registry.disable(&v1.qualified_id).expect("disable");
    assert_eq!(
        disabled.status,
        CapabilityLifecycleStatus::InstalledDisabled
    );
    assert!(!disabled.enabled_intent);
    assert!(disabled.active_digest.is_none());

    registry
        .uninstall(&grants, &config, &v1.qualified_id)
        .expect("uninstall");
    assert!(registry.get(&v1.qualified_id).expect("get").is_none());
    assert!(!registry
        .packages_root()
        .join("publisher.example/text-tools")
        .exists());
    cleanup(&root);
}

#[test]
fn disabling_and_re_enabling_keeps_the_rollback_target() {
    let root = temp_root("lifecycle-rollback-target");
    let key = generate_signing_key("release-1");
    write_trust_store(&root, &key, TrustPolicy::RequireTrusted);
    let registry = CapabilityPluginRegistry::new(&root);
    let grants = CapabilityGrantStore::new(&root);
    let v1 = signed_package(&root, &key, manifest("capability", "1.0.0"), b"one");
    let v1 = install_capability_from_zip(&v1, &registry).expect("install v1");
    registry
        .enable(&grants, &v1.qualified_id, Some(&v1.digest))
        .expect("enable v1");
    let v2 = signed_package(&root, &key, manifest("capability", "2.0.0"), b"two");
    let v2 = install_capability_from_zip(&v2, &registry).expect("install v2");
    registry
        .upgrade(&grants, &v2.qualified_id, &v2.digest)
        .expect("upgrade to v2");

    registry.disable(&v1.qualified_id).expect("disable");
    let enabled = registry
        .enable(&grants, &v1.qualified_id, None)
        .expect("re-enable");

    // The rollback target is the version the upgrade displaced, and switching the plugin off and
    // back on is not an activation that displaces anything.
    assert_eq!(enabled.active_digest.as_deref(), Some(v2.digest.as_str()));
    assert_eq!(enabled.previous_digest.as_deref(), Some(v1.digest.as_str()));
    let rolled_back = registry
        .rollback(&grants, &v1.qualified_id)
        .expect("rollback after the round trip");
    assert_eq!(
        rolled_back.active_digest.as_deref(),
        Some(v1.digest.as_str())
    );
    cleanup(&root);
}

#[test]
fn prepared_lifecycle_journal_restores_the_previous_record() {
    let root = temp_root("lifecycle-prepared-recovery");
    let key = generate_signing_key("release-1");
    write_trust_store(&root, &key, TrustPolicy::RequireTrusted);
    let registry = CapabilityPluginRegistry::new(&root);
    let grants = CapabilityGrantStore::new(&root);
    let package = signed_package(&root, &key, manifest("capability", "1.0.0"), b"runtime");
    let installed = install_capability_from_zip(&package, &registry).expect("install");
    let old = registry
        .get(&installed.qualified_id)
        .expect("get old")
        .expect("old record");
    let next = registry
        .enable(&grants, &installed.qualified_id, Some(&installed.digest))
        .expect("enable");

    registry
        .write_lifecycle_test_journal(old.clone(), next.clone(), false)
        .expect("write prepared journal");
    registry
        .restore_record(next)
        .expect("simulate interrupted transition");

    assert_eq!(registry.recover_capability_lifecycle().unwrap(), 1);
    assert_eq!(registry.get(&installed.qualified_id).unwrap(), Some(old));
    assert_lifecycle_journals_empty(&registry);
    cleanup(&root);
}

#[test]
fn committed_lifecycle_journal_restores_the_next_record() {
    let root = temp_root("lifecycle-committed-recovery");
    let key = generate_signing_key("release-1");
    write_trust_store(&root, &key, TrustPolicy::RequireTrusted);
    let registry = CapabilityPluginRegistry::new(&root);
    let grants = CapabilityGrantStore::new(&root);
    let package = signed_package(&root, &key, manifest("capability", "1.0.0"), b"runtime");
    let installed = install_capability_from_zip(&package, &registry).expect("install");
    let old = registry
        .get(&installed.qualified_id)
        .expect("get old")
        .expect("old record");
    let next = registry
        .enable(&grants, &installed.qualified_id, Some(&installed.digest))
        .expect("enable");

    registry
        .write_lifecycle_test_journal(old.clone(), next.clone(), true)
        .expect("write committed journal");
    registry
        .restore_record(old)
        .expect("simulate stale registry record");

    assert_eq!(registry.recover_capability_lifecycle().unwrap(), 1);
    assert_eq!(registry.get(&installed.qualified_id).unwrap(), Some(next));
    assert_lifecycle_journals_empty(&registry);
    cleanup(&root);
}

#[test]
fn committed_uninstall_recovery_finishes_all_plugin_side_state() {
    let root = temp_root("lifecycle-uninstall-recovery");
    let key = generate_signing_key("release-1");
    write_trust_store(&root, &key, TrustPolicy::RequireTrusted);
    let registry = CapabilityPluginRegistry::new(&root);
    let grants = CapabilityGrantStore::new(&root);
    let config = CapabilityConfigStore::new(&root);
    let mut requested = manifest("capability", "1.0.0");
    requested["permissions"] = json!(["hook.notice.show"]);
    let package = signed_package(&root, &key, requested, b"runtime");
    let installed = install_capability_from_zip(&package, &registry).expect("install");
    registry
        .approve_permissions(
            &grants,
            &installed.qualified_id,
            &installed.digest,
            &["hook.notice.show".to_owned()],
        )
        .expect("grant permission");
    let mut values = serde_json::Map::new();
    values.insert("language".to_owned(), json!("zh-CN"));
    config
        .write(&installed.qualified_id, 0, values)
        .expect("write config");
    let old = registry
        .get(&installed.qualified_id)
        .expect("get old")
        .expect("old record");
    let tombstone = registry
        .write_uninstall_test_journal(old, true)
        .expect("write committed uninstall journal");
    let live = registry
        .packages_root()
        .join("publisher.example/text-tools");
    fs::rename(&live, &tombstone).expect("simulate committed package removal");
    registry
        .remove_record(&installed.qualified_id)
        .expect("simulate committed registry removal");

    assert_eq!(registry.recover_capability_lifecycle().unwrap(), 1);
    assert!(registry.get(&installed.qualified_id).unwrap().is_none());
    assert!(!tombstone.exists());
    assert!(grants
        .list()
        .unwrap()
        .iter()
        .all(|grant| grant.qualified_id != installed.qualified_id));
    assert_eq!(config.read(&installed.qualified_id).unwrap().revision, 0);
    assert_lifecycle_journals_empty(&registry);
    cleanup(&root);
}

#[test]
fn permission_expansion_requires_digest_bound_exact_approval() {
    let root = temp_root("permissions");
    let key = generate_signing_key("release-1");
    write_trust_store(&root, &key, TrustPolicy::RequireTrusted);
    let registry = CapabilityPluginRegistry::new(&root);
    let grants = CapabilityGrantStore::new(&root);
    let permission = "hook.clipboard.write".to_owned();
    let mut requested = manifest("capability", "1.0.0");
    requested["permissions"] = json!([permission]);
    let package = signed_package(&root, &key, requested, b"runtime");
    let installed = install_capability_from_zip(&package, &registry).expect("install");

    let error = registry
        .enable(&grants, &installed.qualified_id, Some(&installed.digest))
        .expect_err("approval required");
    assert!(matches!(
        error,
        CapabilityInstallError::PermissionRequired(_)
    ));
    assert_eq!(
        registry
            .get(&installed.qualified_id)
            .expect("get")
            .expect("record")
            .status,
        CapabilityLifecycleStatus::ApprovalRequired
    );
    assert!(registry
        .approve_permissions(
            &grants,
            &installed.qualified_id,
            &installed.digest,
            &["hook.notice.show".to_owned()]
        )
        .is_err());
    registry
        .approve_permissions(
            &grants,
            &installed.qualified_id,
            &installed.digest,
            &["hook.clipboard.write".to_owned()],
        )
        .expect("approve exact request");
    assert_eq!(
        registry
            .enable(&grants, &installed.qualified_id, Some(&installed.digest))
            .expect("enable")
            .status,
        CapabilityLifecycleStatus::Active
    );
    cleanup(&root);
}

#[test]
fn config_store_is_revisioned_bounded_and_rejects_secret_keys() {
    let root = temp_root("config");
    let store = CapabilityConfigStore::new(&root);
    let mut values = serde_json::Map::new();
    values.insert("language".to_owned(), json!("zh-CN"));
    let saved = store
        .write("publisher.example/text-tools", 0, values)
        .expect("write config");
    assert_eq!(saved.revision, 1);
    assert!(store
        .write("publisher.example/text-tools", 0, serde_json::Map::new())
        .is_err());
    let mut secret = serde_json::Map::new();
    secret.insert("apiToken".to_owned(), json!("do-not-store"));
    assert!(store
        .write("publisher.example/text-tools", 1, secret)
        .is_err());
    for unsafe_id in [
        "publisher.example/con",
        "publisher.example/text.tools",
        "con/text-tools",
    ] {
        assert!(store.read(unsafe_id).is_err(), "{unsafe_id}");
    }
    cleanup(&root);
}

#[test]
fn runtime_failure_window_persists_backoff_and_faults_at_the_bound() {
    let root = temp_root("runtime-failures");
    let key = generate_signing_key("release-1");
    write_trust_store(&root, &key, TrustPolicy::RequireTrusted);
    let registry = CapabilityPluginRegistry::new(&root);
    let grants = CapabilityGrantStore::new(&root);
    let package = signed_package(&root, &key, manifest("capability", "1.0.0"), b"runtime");
    let installed = install_capability_from_zip(&package, &registry).expect("install");
    registry
        .enable(&grants, &installed.qualified_id, Some(&installed.digest))
        .expect("enable");

    let first = registry
        .record_runtime_failure_at(&installed.qualified_id, 10_000)
        .expect("first failure");
    assert_eq!(first.runtime_failures.count, 1);
    assert_eq!(first.runtime_failures.restart_not_before_ms, Some(11_000));
    let persisted = CapabilityPluginRegistry::new(&root)
        .get(&installed.qualified_id)
        .unwrap()
        .unwrap();
    assert_eq!(persisted.runtime_failures, first.runtime_failures);

    let mut faulted = first;
    for offset in 1..CAPABILITY_MAX_RUNTIME_FAILURES {
        faulted = registry
            .record_runtime_failure_at(&installed.qualified_id, 10_000 + u64::from(offset))
            .expect("bounded failure");
    }
    assert_eq!(faulted.status, CapabilityLifecycleStatus::Faulted);
    assert!(faulted.enabled_intent);
    assert!(!registry.runtime_restart_allowed(&faulted));

    let retried = registry
        .enable(&grants, &installed.qualified_id, Some(&installed.digest))
        .expect("explicit retry");
    assert_eq!(
        retried.runtime_failures,
        CapabilityRuntimeFailureState::default()
    );
    assert_eq!(retried.status, CapabilityLifecycleStatus::Active);
    cleanup(&root);
}
