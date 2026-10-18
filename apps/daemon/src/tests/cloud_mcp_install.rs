// 真实宿主安装与 stdio 进程验证；下载回执在此是夹具，不冒充线上账号下载。
fn cloud_mcp_archive(version: &str) -> Vec<u8> {
    let manifest = json!({
        "schemaVersion":1, "id":"cloud-version", "name":"Cloud Version",
        "version":version, "publisher":{"id":"test.cloud", "name":"Test"},
        "transport":"stdio", "entry":{"command":"runtime/server.ps1", "args":[version]},
        "tools":["version"], "credentials":[]
    });
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options: zip::write::SimpleFileOptions = zip::write::FileOptions::default();
    writer.start_file("mcp.server.json", options).unwrap();
    writer
        .write_all(&serde_json::to_vec(&manifest).unwrap())
        .unwrap();
    writer.start_file("runtime/server.ps1", options).unwrap();
    writer
        .write_all(include_bytes!("fixtures/cloud-mcp-version.ps1"))
        .unwrap();
    writer.finish().unwrap().into_inner()
}

#[test]
fn cloud_install_mcp_failed_commit_restores_active_and_keeps_versions() {
    let root =
        std::env::temp_dir().join(format!("loom-cloud-mcp-rollback-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    let runtime = test_daemon_runtime(&root, None);
    let body = |version| json!({"zipBase64":BASE64.encode(cloud_mcp_archive(version))}).to_string();
    let store = mcp_server_store_path(&root);
    assert_eq!(
        install_mcp_server_package(&body("1.0.0"), &runtime.mcp_servers, &root, &store)
            .unwrap()
            .0,
        200
    );
    let active = root.join("mcp/packages/test.cloud/cloud-version/active.json");
    let before_active = fs::read(&active).unwrap();
    let before_store = fs::read(&store).unwrap();
    let before = runtime
        .mcp_servers
        .lock()
        .unwrap()
        .get("cloud-version")
        .unwrap()
        .clone();
    let blocked = root.join("blocked-store");
    fs::create_dir(&blocked).unwrap();
    let (status, response) =
        install_mcp_server_package(&body("1.1.0"), &runtime.mcp_servers, &root, &blocked).unwrap();
    assert_eq!(status, 500, "{response}");
    assert_eq!(fs::read(&active).unwrap(), before_active);
    assert_eq!(fs::read(&store).unwrap(), before_store);
    assert_eq!(
        runtime
            .mcp_servers
            .lock()
            .unwrap()
            .get("cloud-version")
            .unwrap()
            .command,
        before.command
    );
    loom_mcp::package::verify_installed_entry(&before).unwrap();
    // 冲突不能删除候选包家族或其历史版本。
    runtime
        .mcp_servers
        .lock()
        .unwrap()
        .get_mut("cloud-version")
        .unwrap()
        .package = None;
    let (status, _) =
        install_mcp_server_package(&body("1.2.0"), &runtime.mcp_servers, &root, &store).unwrap();
    assert_eq!(status, 409);
    assert_eq!(fs::read(&active).unwrap(), before_active);
    assert!(Path::new(&before.command).is_file());
    drop(runtime);
    fs::remove_dir_all(root).unwrap();
}

#[cfg(windows)]
#[test]
fn cloud_install_mcp_updates_and_executes_selected_version() {
    let root = std::env::temp_dir().join(format!("loom-cloud-mcp-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    let runtime = test_daemon_runtime(&root, None);
    let mut prior_command = None;
    let trial: Option<Value> = std::env::var("LOOM_ASSETLIBRARY_MCP_UPDATE_FIXTURE")
        .ok()
        .map(|path| serde_json::from_slice(&fs::read(path).unwrap()).unwrap());
    for version in ["1.0.0", "1.1.0"] {
        let bytes = cloud_mcp_archive(version);
        let receipt =
            json!({"publicationId":format!("test-{version}"), "sha256":sha256_bytes(&bytes)});
        let package = match &trial {
            Some(config) => asset_library::prepare_trial_install(&config[version]).unwrap(),
            None => asset_library::InstallPackage {
                archive: None,
                kind: "mcp".into(),
                bytes,
                download: receipt,
            },
        };
        let receipt = package.download.clone();
        let (status, body) = install_asset_library_package(
            package,
            &root,
            &runtime.capability_runtime,
            &runtime.capability_resources,
            &runtime.hook_bridge,
            &runtime.tool_registry,
            &runtime.framework_registry,
            &runtime.workflow_store,
            &runtime.mcp_servers,
        )
        .unwrap();
        assert_eq!(status, 200, "{body}");
        let response: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(response["download"], receipt);
        let config = runtime
            .mcp_servers
            .lock()
            .unwrap()
            .get("cloud-version")
            .unwrap()
            .clone();
        assert_eq!(config.package.as_ref().unwrap().version, version);
        if let Some(previous) = prior_command {
            assert_ne!(config.command, previous);
            assert!(Path::new(&previous).is_file(), "更新保留旧版包体");
        }
        let mut client = McpClient::connect(&config).expect("spawn installed MCP");
        client.initialize().expect("initialize installed MCP");
        assert_eq!(client.list_tools().unwrap()["tools"][0]["name"], "version");
        let result = client.call_tool("version", json!({})).unwrap();
        assert_eq!(result["content"][0]["text"], version);
        drop(client);
        prior_command = Some(config.command);
    }
    drop(runtime);
    fs::remove_dir_all(root).unwrap();
}
