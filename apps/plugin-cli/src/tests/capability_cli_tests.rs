#[test]
fn capability_cli_init_sign_validate_pack_and_install() {
    let root = temp_root("capability-e2e");
    let package_dir = root.join("capability-package");
    let key_path = root.join("publisher-key.json");
    let trust_path = root.join("plugin-trust.json");
    let archive_path = root.join("capability.zip");
    run_cli(&[
        "loom-plugin".to_owned(),
        "init".to_owned(),
        "capability".to_owned(),
        package_dir.to_string_lossy().into_owned(),
        "sample-capability".to_owned(),
        "publisher.example".to_owned(),
    ])
    .expect("init capability");
    fs::write(
        package_dir.join("runtime/sample-capability.exe"),
        b"MZ-capability-fixture",
    )
    .expect("capability runtime");
    run_cli(&[
        "loom-plugin".to_owned(),
        "keygen".to_owned(),
        key_path.to_string_lossy().into_owned(),
        "release-key".to_owned(),
    ])
    .expect("keygen");
    run_cli(&[
        "loom-plugin".to_owned(),
        "sign".to_owned(),
        package_dir.to_string_lossy().into_owned(),
        key_path.to_string_lossy().into_owned(),
        "publisher.example".to_owned(),
    ])
    .expect("sign capability");
    run_cli(&[
        "loom-plugin".to_owned(),
        "trust".to_owned(),
        "add".to_owned(),
        trust_path.to_string_lossy().into_owned(),
        "publisher.example".to_owned(),
        key_path.to_string_lossy().into_owned(),
    ])
    .expect("trust publisher");
    let validated = run_cli(&[
        "loom-plugin".to_owned(),
        "validate".to_owned(),
        package_dir.to_string_lossy().into_owned(),
        "--trust-store".to_owned(),
        trust_path.to_string_lossy().into_owned(),
    ])
    .expect("validate capability");
    assert!(validated.contains("trust=Trusted"), "{validated}");
    run_cli(&[
        "loom-plugin".to_owned(),
        "pack".to_owned(),
        package_dir.to_string_lossy().into_owned(),
        archive_path.to_string_lossy().into_owned(),
    ])
    .expect("pack capability");
    let registry = loom_tool_registry::capability::CapabilityPluginRegistry::new(&root);
    let report = loom_tool_registry::capability::install_capability_from_zip(
        &fs::read(archive_path).expect("archive"),
        &registry,
    )
    .expect("install capability");
    assert_eq!(report.qualified_id, "publisher.example/sample-capability");
    assert_eq!(report.trust_status, PackageTrustStatus::Trusted);
    cleanup_test_tree(&root);
}
