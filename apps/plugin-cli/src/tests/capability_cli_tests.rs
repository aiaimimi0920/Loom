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
    let report = run_cli(&[
        "loom-plugin".into(), "install".into(), "capability".into(),
        archive_path.to_string_lossy().into_owned(), "--control-plane".into(),
        root.to_string_lossy().into_owned(),
    ]).expect("install capability through public CLI");
    let report: Value = serde_json::from_str(&report).expect("install report JSON");
    assert_eq!(report["qualifiedId"], "publisher.example/sample-capability");
    assert_eq!(report["trustStatus"], "trusted");
    let record = registry.get("publisher.example/sample-capability").unwrap().unwrap();
    assert!(!record.enabled_intent, "local installation cannot implicitly enable code");
    cleanup_test_tree(&root);
}

#[test]
fn capability_local_install_rejects_relative_and_oversized_sources() {
    let root = temp_root("local-install-bounds");
    assert!(install_local_capability(Path::new("relative.zip"), &root).is_err());
    let archive = root.join("too-large.zip");
    let file = File::create(&archive).unwrap();
    file.set_len(loom_tool_registry::capability::MAX_CAPABILITY_PACKAGE_DOWNLOAD_BYTES as u64 + 1).unwrap();
    drop(file);
    let error = install_local_capability(&archive, &root).unwrap_err();
    assert!(error.to_string().contains("size limit"));
    cleanup_test_tree(&root);
}
