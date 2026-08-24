use super::*;

#[test]
fn package_roundtrips_installed_art() {
    let root = temp_root();
    let framework = FrameworkRegistry::new(&root);
    install_test_framework(&framework, "process");
    let registry = ToolRegistry::new(root.join("tools"));
    let manifest = r#"{"id":"pkg-art","name":"Pkg","description":"d","enabled":true,
            "execution":{"type":"framework_art","framework":"process"}}"#;
    let zip = build_zip(manifest, &[("bin/tool.exe", b"binary")]);
    let report = install_art_from_zip(&zip, &root, &framework, &registry).expect("install");

    let saved = registry.get_tool("pkg-art").unwrap().unwrap();
    let packaged = package_art_to_zip(&saved, &report.art_dir).expect("package");
    // The packaged zip is re-readable and carries the bundled binary.
    let manifest_back = read_manifest_from_zip(&packaged).expect("read back");
    assert_eq!(manifest_back.id, "pkg-art");
    let mut archive = zip::ZipArchive::new(Cursor::new(&packaged)).unwrap();
    assert!(archive.by_name("bin/tool.exe").is_ok());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn installs_framework_art_and_records_external_package_directory() {
    let root = temp_root();
    let framework = FrameworkRegistry::new(&root);
    install_test_framework(&framework, "process");
    let registry = ToolRegistry::new(root.join("tools"));
    let manifest = r#"{
            "id":"external-script-art","name":"External Script Art","description":"d","enabled":true,
            "execution":{"type":"framework_art","framework":"process"},
            "metadata":{"dependencies":{"framework":"process"}}
        }"#;
    let zip = build_zip(manifest, &[("resources/input.txt", b"fixture")]);
    let report = install_art_from_zip(&zip, &root, &framework, &registry).expect("install");
    let saved = registry
        .get_tool("external-script-art")
        .expect("get tool")
        .expect("saved tool");
    assert!(matches!(
        saved.execution,
        ToolExecution::FrameworkArt { ref framework } if framework == "process"
    ));
    let expected_dir = report.art_dir.to_string_lossy().to_string();
    assert_eq!(
        saved
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("artPackage"))
            .and_then(|package| package.get("dir"))
            .and_then(serde_json::Value::as_str),
        Some(expected_dir.as_str())
    );
    std::fs::remove_dir_all(&root).ok();
}
