//! Admission snapshots must not survive changes to registry, trust or package state.
use super::*;

fn capture(registry: &FrameworkRegistry, reference: &str) -> FrameworkExecutionAdmission {
    let package = registry.runtime_dir(reference);
    let text = fs::read_to_string(package.join(FRAMEWORK_MANIFEST_FILE)).unwrap();
    FrameworkExecutionAdmission::capture(&registry.root.join("frameworks"), &package, &text)
        .expect("installed fixture admission")
}

#[test]
fn framework_execution_admission_rechecks_disabled_state_and_corrupt_registry() {
    let root = temp_root();
    let registry = FrameworkRegistry::new(&root);
    registry
        .install_framework_package_from_zip(&fake_framework_package_zip_with_version(
            "process", "1.0.0",
        ))
        .unwrap();
    let admission = capture(&registry, "publisher.test/process");
    registry.disable("publisher.test/process").unwrap();
    assert!(admission.revalidate().unwrap_err().contains("disabled"));
    registry.enable("publisher.test/process").unwrap();
    admission.revalidate().unwrap();
    fs::write(root.join(FRAMEWORKS_FILE), b"invalid-json").unwrap();
    assert!(admission.revalidate().is_err());
    set_framework_tree_readonly(&root, false).unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn framework_execution_admission_rechecks_revocation_after_resolution() {
    let root = temp_root();
    let registry = FrameworkRegistry::new(&root);
    let key = loom_plugin_security::generate_signing_key("admission-key");
    registry
        .trust_publisher(PublisherTrustRecord {
            publisher_id: "publisher.admission".to_owned(),
            key_id: key.key_id.clone(),
            public_key: key.public_key.clone(),
            revoked: false,
        })
        .unwrap();
    registry
        .install_framework_package_from_zip(&signed_framework_package_zip(
            "admission",
            "1.0.0",
            "publisher.admission",
            &key,
        ))
        .unwrap();
    let admission = capture(&registry, "publisher.admission/admission");
    registry
        .revoke_publisher("publisher.admission", &key.key_id)
        .unwrap();
    let error = admission.revalidate().unwrap_err();
    assert!(error.contains("Revoked"), "{error}");
    set_framework_tree_readonly(&root, false).unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn framework_execution_admission_rejects_tampering_and_activation_changes() {
    let root = temp_root();
    let registry = FrameworkRegistry::new(&root);
    registry
        .install_framework_package_from_zip(&fake_framework_package_zip_with_version(
            "process", "1.0.0",
        ))
        .unwrap();
    let admission = capture(&registry, "publisher.test/process");
    let package = registry.runtime_dir("publisher.test/process");
    let manifest = registry.package_manifest("publisher.test/process").unwrap();
    set_framework_tree_readonly(&package, false).unwrap();
    fs::write(package.join(&manifest.entry.command), b"tampered").unwrap();
    assert!(admission.revalidate().is_err());
    registry
        .install_framework_package_from_zip(&fake_framework_package_zip_with_version(
            "process", "2.0.0",
        ))
        .unwrap();
    assert!(admission
        .revalidate()
        .unwrap_err()
        .contains("version changed"));
    set_framework_tree_readonly(&root, false).unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn framework_admission_rejects_preexisting_tampering_with_a_forged_digest_lock() {
    let root = temp_root();
    let registry = FrameworkRegistry::new(&root);
    registry
        .install_framework_package_from_zip(&fake_framework_package_zip_with_version(
            "process", "1.0.0",
        ))
        .unwrap();
    let package = registry.runtime_dir("publisher.test/process");
    let manifest = registry.package_manifest("publisher.test/process").unwrap();
    let original_digest = canonical_package_digest(&package, None).unwrap();
    set_framework_tree_readonly(&package, false).unwrap();
    fs::write(
        package.join(&manifest.entry.command),
        b"tampered-before-capture",
    )
    .unwrap();
    let changed_digest = canonical_package_digest(&package, None).unwrap();
    let locks = package.parent().unwrap().parent().unwrap().join("locks");
    fs::copy(
        locks.join(format!("{original_digest}.json")),
        locks.join(format!("{changed_digest}.json")),
    )
    .unwrap();
    let text = fs::read_to_string(package.join(FRAMEWORK_MANIFEST_FILE)).unwrap();
    let error = FrameworkExecutionAdmission::capture(&root.join("frameworks"), &package, &text)
        .err()
        .expect("immutable install identity must reject tampering before capture");
    assert!(error.contains("immutable version path"), "{error}");
    set_framework_tree_readonly(&root, false).unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn framework_admission_accepts_installer_recovery_version_identity() {
    let root = temp_root();
    let registry = FrameworkRegistry::new(&root);
    registry
        .install_framework_package_from_zip(&fake_framework_package_zip_with_version(
            "process", "1.0.0",
        ))
        .unwrap();
    let package = registry.runtime_dir("publisher.test/process");
    let recovered_name = format!(
        "{}-recovered-123456",
        package.file_name().unwrap().to_str().unwrap()
    );
    let recovered = package.parent().unwrap().join(&recovered_name);
    fs::rename(&package, &recovered).unwrap();
    fs::write(
        recovered
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join(FRAMEWORK_ACTIVE_FILE),
        serde_json::to_vec(&serde_json::json!({ "active": format!("versions/{recovered_name}") }))
            .unwrap(),
    )
    .unwrap();
    capture(&registry, "publisher.test/process")
        .revalidate()
        .unwrap();
    set_framework_tree_readonly(&root, false).unwrap();
    fs::remove_dir_all(root).unwrap();
}
