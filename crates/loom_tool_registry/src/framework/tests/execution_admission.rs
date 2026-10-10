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
