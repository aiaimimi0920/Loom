// 两版程序将版本写入真实进程响应；不能只凭清单版本判断升级成功。
fn cloud_versioned_capability(
    root: &Path,
    key: &loom_plugin_security::SigningKeyDocument,
    version: &str,
) -> Vec<u8> {
    capability_api_fixture_version(
        root,
        key,
        version,
        &["hook.unit.attachments.write", "hook.notice.show"],
    );
    let package = root.join("api-fixture-package");
    let executable = package.join("runtime/api-fixture.exe");
    let source = root.join("versioned_capability.rs");
    let replacement = format!(r#"\"ok\":true,\"version\":\"{version}\""#);
    let code = CAPABILITY_API_FIXTURE_SOURCE.replace(r#"\"ok\":true"#, &replacement);
    assert_ne!(code, CAPABILITY_API_FIXTURE_SOURCE);
    fs::write(&source, code).unwrap();
    let status = Command::new("rustc")
        .arg(&source)
        .arg("-O")
        .arg("-o")
        .arg(&executable)
        .status()
        .unwrap();
    assert!(status.success());
    let pdb = executable.with_extension("pdb");
    if pdb.exists() {
        fs::remove_file(pdb).unwrap();
    }
    loom_plugin_security::sign_package(&package, "signature.json", key).unwrap();
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for name in [
        "capability.manifest.json",
        "ui.surface.json",
        "signature.json",
        "runtime/api-fixture.exe",
    ] {
        zip.start_file(name, cloud_capability_zip_options())
            .unwrap();
        zip.write_all(&fs::read(package.join(name)).unwrap())
            .unwrap();
    }
    zip.finish().unwrap().into_inner()
}

fn cloud_capability_zip_options() -> zip::write::SimpleFileOptions {
    // Windows 默认写 DOS 来源；Unix 权限必须同时声明 Unix 来源。
    zip::write::SimpleFileOptions::default()
        .system(zip::System::Unix)
        .unix_permissions(0o644)
}

#[test]
fn cloud_capability_zip_attributes_match_declared_system() {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file("fixture.txt", cloud_capability_zip_options())
        .unwrap();
    zip.write_all(b"fixture").unwrap();
    let bytes = zip.finish().unwrap().into_inner();
    let end = bytes.len() - 22;
    let central = u32::from_le_bytes(bytes[end + 16..end + 20].try_into().unwrap()) as usize;
    assert_eq!(&bytes[central..central + 4], b"PK\x01\x02");
    assert_eq!(bytes[central + 5], 3);
    let attributes = u32::from_le_bytes(bytes[central + 38..central + 42].try_into().unwrap());
    assert_eq!(attributes >> 16, 0o100644);
    assert_eq!(attributes & 0xffff, 0);
}

#[cfg(windows)]
#[test]
#[ignore = "explicit signed two-version fixture export"]
fn cloud_generate_capability_update_fixture() {
    let output = PathBuf::from(std::env::var("LOOM_CLOUD_SIGNED_FIXTURE_ROOT").unwrap());
    let key = loom_plugin_security::generate_signing_key("cloud-update-fixture");
    let mut trust = loom_plugin_security::TrustStore::default();
    trust.trust(PublisherTrustRecord {
        publisher_id: "publisher.example".into(),
        key_id: key.key_id.clone(),
        public_key: key.public_key.clone(),
        revoked: false,
    });
    assert!(!output.join("capability-trust.json").exists());
    for version in ["1.0.0", "1.1.0"] {
        let build = output.join(format!("build-{version}"));
        fs::create_dir_all(&build).unwrap();
        let archive = cloud_versioned_capability(&build, &key, version);
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output.join(format!("capability-{version}.zip")))
            .unwrap();
        file.write_all(&archive).unwrap();
        file.sync_all().unwrap();
    }
    trust
        .write_atomic(&output.join("capability-trust.json"))
        .unwrap();
}

#[cfg(windows)]
#[test]
#[ignore = "requires two published trial versions and an isolated trusted signing fixture"]
fn cloud_install_capability_updates_and_executes_selected_version() {
    let _env_guard = lock_ignoring_poison(&ENV_LOCK);
    let config: Value = serde_json::from_slice(
        &fs::read(std::env::var("LOOM_ASSETLIBRARY_CAPABILITY_UPDATE_FIXTURE").unwrap()).unwrap(),
    )
    .unwrap();
    let fixture = PathBuf::from(std::env::var("LOOM_CLOUD_SIGNED_FIXTURE_ROOT").unwrap());
    let root = unique_temp_dir("cloud-capability-update");
    fs::create_dir_all(&root).unwrap();
    fs::copy(
        fixture.join("capability-trust.json"),
        root.join("plugin-trust.json"),
    )
    .unwrap();
    let runtime = test_daemon_runtime(&root, None);
    let mut previous = None;
    let mut observations = Vec::new();
    for version in ["1.0.0", "1.1.0"] {
        let package = asset_library::prepare_trial_install(&config[version]).unwrap();
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
        let installed: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(installed["download"], receipt);
        let digest = installed["installation"]["package"]["digest"]
            .as_str()
            .unwrap();
        let select = json!({"digest":digest}).to_string();
        let activate = if previous.is_some() {
            upgrade_capability_plugin
        } else {
            enable_capability_plugin
        };
        assert_ne!(
            activate(
                "publisher.example/api-fixture",
                &select,
                &root,
                &runtime.capability_runtime
            )
            .unwrap()
            .0,
            200,
            "new digest requires explicit approval"
        );
        assert_eq!(approve_capability_plugin("publisher.example/api-fixture",
            &json!({"digest":digest,"permissions":["hook.unit.attachments.write","hook.notice.show"]}).to_string(),
            &root).unwrap().0, 200);
        let (status, body) = activate(
            "publisher.example/api-fixture",
            &select,
            &root,
            &runtime.capability_runtime,
        )
        .unwrap();
        assert_eq!(status, 200, "{body}");
        let invoked = expect_json_text_route_response(route_request(&runtime, &parsed_request(
            "POST", "/v1/invoke", &[], Some(&json!({"requestId":format!("cloud-update-{version}"),
                "caller":"hook","capability":"publisher.example/api-fixture.run","input":{"text":"hello"}}).to_string()))), 200);
        assert_eq!(invoked["status"], "succeeded", "{invoked}");
        assert_eq!(
            invoked["output"]["output"]["version"], version,
            "actual executable must be updated"
        );
        if let Some(old) = previous {
            assert_ne!(old, digest);
        }
        previous = Some(digest.to_owned());
        observations
            .push(json!({"version":version,"digest":digest,"download":receipt,"result":invoked}));
    }
    fs::write(
        fixture.join("capability-execution-evidence.json"),
        serde_json::to_vec_pretty(&observations).unwrap(),
    )
    .unwrap();
    runtime
        .capability_runtime
        .deactivate("publisher.example/api-fixture")
        .unwrap();
    drop(runtime);
    remove_test_dir(&root);
}
