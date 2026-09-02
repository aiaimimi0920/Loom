use super::*;

#[test]
fn activation_and_rollback_reject_tampered_installed_capability_versions() {
    let root = temp_root("installed-tamper");
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
        .expect("upgrade v2");

    crate::install::fs_safety::set_tree_readonly(&v1.package_dir, false)
        .expect("unlock installed fixture");
    fs::write(
        v1.package_dir.join("runtime/resources/ocr/text-model.onnx"),
        b"tampered",
    )
    .expect("tamper installed model");
    assert!(registry
        .verify_installed_version(&v1.qualified_id, &v1.digest)
        .is_err());
    assert!(registry.rollback(&grants, &v1.qualified_id).is_err());
    assert_eq!(
        registry
            .get(&v1.qualified_id)
            .unwrap()
            .unwrap()
            .active_digest,
        Some(v2.digest)
    );
    cleanup(&root);
}
