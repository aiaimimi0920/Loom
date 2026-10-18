// 显式提供真实构建产物，不能用 MZ 文本夹具冒充 Art 执行成功。
#[cfg(windows)]
#[test]
#[ignore = "requires real signed Art/framework archives and an isolated test trust store"]
fn cloud_install_real_art_blends_image_with_packaged_framework() {
    let _env_guard = lock_ignoring_poison(&ENV_LOCK);
    let root = unique_temp_dir("cloud-real-art-execution");
    fs::create_dir_all(&root).unwrap();
    struct ControlRootGuard(Option<std::ffi::OsString>);
    impl Drop for ControlRootGuard {
        fn drop(&mut self) {
            if let Some(value) = &self.0 {
                std::env::set_var("LOOM_CONTROL_PLANE_ROOT", value);
            } else {
                std::env::remove_var("LOOM_CONTROL_PLANE_ROOT");
            }
        }
    }
    let _control_root_guard = ControlRootGuard(std::env::var_os("LOOM_CONTROL_PLANE_ROOT"));
    std::env::set_var("LOOM_CONTROL_PLANE_ROOT", &root);
    let fixture = PathBuf::from(
        std::env::var("LOOM_CLOUD_SIGNED_FIXTURE_ROOT").expect("explicit fixture root"),
    );
    let package = if let Ok(config_path) = std::env::var("LOOM_ASSETLIBRARY_TEST_FIXTURE") {
        let config: Value = serde_json::from_slice(&fs::read(config_path).unwrap()).unwrap();
        let downloaded = asset_library::prepare_trial_install(&config);
        if config["expected"] == "denied" {
            match downloaded {
                Ok(_) => panic!("撤销或下架后不得取得安装包"),
                Err(error) => assert_eq!(error.status, 404),
            }
            assert!(!Path::new(config["downloadRoot"].as_str().unwrap())
                .join("asset-library-downloads")
                .exists());
            remove_test_dir(&root);
            return;
        }
        assert_eq!(config["expected"], "saved");
        let package = downloaded.unwrap();
        assert_eq!(package.bytes.len() as u64, config["size"].as_u64().unwrap());
        package
    } else {
        let archive =
            fs::read(fixture.join("arts-correct-publisher/custom-image-blend-script.zip")).unwrap();
        let receipt = json!({"publicationId":"isolated-real-art", "sha256":sha256_bytes(&archive), "size":archive.len()});
        asset_library::InstallPackage {
            archive: None,
            kind: "art".into(),
            bytes: archive,
            download: receipt,
        }
    };
    fs::copy(
        fixture.join("test-trust.json"),
        root.join("plugin-trust.json"),
    )
    .unwrap();
    let runtime = test_daemon_runtime(&root, None);
    let framework = fs::read(fixture.join("frameworks-cloud-install/process.zip")).unwrap();
    runtime
        .framework_registry
        .install_framework_package_from_zip(&framework)
        .expect("install actual process framework");
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
    let image = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII=";
    let executed = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request(
                "POST",
                "/v1/tools/custom-image-blend-script/execute",
                &[],
                Some(
                    &json!({"arguments":{
                        "inputs":{"input":image,"reference":image}, "params":{"mix_ratio":50}
                    }})
                    .to_string(),
                ),
            ),
        ),
        200,
    );
    assert_eq!(executed["status"], "succeeded", "{executed}");
    let result = &executed["result"];
    let output = result["output_base64"]
        .as_str()
        .or_else(|| result["output"]["output_base64"].as_str())
        .or_else(|| result["content"][0]["data"].as_str())
        .expect("actual image output");
    let bytes = loom_image_io::decode_data_url_bytes(output).expect("decode actual image");
    assert!(
        bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "输出必须是真实 PNG"
    );
    println!("real Art output: {} PNG bytes", bytes.len());
    drop(runtime);
    remove_test_dir(&root);
}
