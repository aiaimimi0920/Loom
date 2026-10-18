#[cfg(windows)]
fn application_test_package(
    root: &Path,
    version: &str,
    resource: &str,
) -> asset_library::InstallPackage {
    let source = root.join("TextNormalize.cs");
    fs::write(&source, include_bytes!("fixtures/TextNormalize.cs")).unwrap();
    let executable = root.join(format!("TextNormalize-{version}.exe"));
    let compiler = PathBuf::from(std::env::var_os("WINDIR").unwrap())
        .join("Microsoft.NET/Framework64/v4.0.30319/csc.exe");
    let mut command = std::process::Command::new(compiler);
    command
        .args(["/nologo", "/target:exe", "/platform:x64", "/optimize+"])
        .arg(format!("/out:{}", executable.display()));
    if version == "1.1.0" {
        command.arg("/define:VERSION2");
    }
    let output = command.arg(&source).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let files = [
        ("application.json", serde_json::to_vec(&json!({"schemaVersion":1, "version":version, "platform":"windows-x86_64", "entry":"TextNormalize.exe"})).unwrap()),
        ("TextNormalize.exe", fs::read(executable).unwrap()),
    ];
    let manifest = json!({"schema":"neuro-software-package-v1", "kind":"application", "files":files.iter().map(|(path, bytes)| json!({"path":path, "size":bytes.len(), "sha256":sha256_bytes(bytes)})).collect::<Vec<_>>()});
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options: zip::write::SimpleFileOptions = zip::write::FileOptions::default();
    writer.start_file("manifest.json", options).unwrap();
    writer
        .write_all(&serde_json::to_vec(&manifest).unwrap())
        .unwrap();
    for (name, bytes) in files {
        writer.start_file(name, options).unwrap();
        writer.write_all(&bytes).unwrap();
    }
    let bytes = writer.finish().unwrap().into_inner();
    let download = json!({"resourceId":resource, "publicationId":uuid::Uuid::new_v4().to_string(), "label":version, "sha256":sha256_bytes(&bytes)});
    asset_library::InstallPackage {
        archive: None,
        kind: "application".into(),
        bytes,
        download,
    }
}

#[cfg(windows)]
#[test]
#[ignore = "requires explicitly supplied complete Loom package"]
fn cloud_install_complete_loom_file_and_run_packaged_daemon() {
    let source = std::env::var_os("LOOM_LARGE_PACKAGE").expect("complete package path");
    let label = std::env::var("LOOM_LARGE_PACKAGE_LABEL").expect("package version label");
    let root = unique_temp_dir("cloud-complete-loom");
    fs::create_dir_all(&root).unwrap();
    let file = fs::File::open(source).unwrap();
    let size = file.metadata().unwrap().len();
    let mut hash = Sha256::new();
    let mut reader = &file;
    let mut block = [0u8; 65536];
    loop {
        let n = reader.read(&mut block).unwrap();
        if n == 0 {
            break;
        }
        hash.update(&block[..n]);
    }
    let mut package = asset_library::InstallPackage {
        archive: Some(file),
        bytes: Vec::new(),
        kind: "application".into(),
        download: json!({"resourceId":uuid::Uuid::new_v4().to_string(),
            "publicationId":uuid::Uuid::new_v4().to_string(), "label":label,
            "sha256":format!("{:x}",hash.finalize()), "size":size,
            "policy":"application-large-zip-format-v1"}),
    };
    let mut previous_state = None;
    if let Some(config_path) = std::env::var_os("LOOM_LARGE_TRIAL_CONFIG") {
        let config: serde_json::Value =
            serde_json::from_slice(&fs::read(config_path).unwrap()).unwrap();
        let expected = package.download["sha256"].clone();
        package = asset_library::prepare_trial_install(&config).unwrap();
        assert_eq!(package.download["sha256"], expected);
        assert_eq!(package.download["downloadEngine"], "libaria2");
        assert!(package.bytes.is_empty());
        assert!(package.archive.is_some());
        if let Some(previous) = config.get("previous") {
            let old = asset_library::prepare_trial_install(previous).unwrap();
            assert_eq!(old.download["downloadEngine"], "libaria2");
            assert_eq!(old.download["resourceId"], package.download["resourceId"]);
            assert_ne!(old.download["sha256"], package.download["sha256"]);
            let installed = asset_library::application::install(&root, &old).unwrap();
            let old_entry = PathBuf::from(installed["entry"].as_str().unwrap());
            let output = std::process::Command::new(
                old_entry.parent().unwrap().join("runtime/loom-daemon.exe"),
            )
            .arg("--version")
            .output()
            .unwrap();
            assert!(output.status.success());
            previous_state = Some(installed);
        }
    }
    let state = asset_library::application::install(&root, &package).unwrap();
    let entry = PathBuf::from(state["entry"].as_str().unwrap());
    assert_eq!(entry.file_name().unwrap(), "Loom.exe");
    let directory = entry.parent().unwrap();
    assert!(directory
        .join("runtime/download-engine/loom_aria2.dll")
        .is_file());
    assert!(directory.join("packages/frameworks/process.zip").is_file());
    let output = std::process::Command::new(directory.join("runtime/loom-daemon.exe"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("0.2.2"));
    if let Some(previous) = &previous_state {
        let old_entry = PathBuf::from(previous["entry"].as_str().unwrap());
        assert!(old_entry.is_file());
        assert_ne!(old_entry, entry);
        assert_ne!(
            sha256_bytes(
                &fs::read(old_entry.parent().unwrap().join("runtime/loom-daemon.exe")).unwrap()
            ),
            sha256_bytes(&fs::read(directory.join("runtime/loom-daemon.exe")).unwrap())
        );
    }
    fs::write(
        root.join("installation-evidence.json"),
        serde_json::to_vec_pretty(&json!({
            "download":package.download, "installation":state,
            "daemonVersion":String::from_utf8_lossy(&output.stdout),
            "previousInstallation":previous_state,
            "desktopUiVerified":false, "selfUpdateVerified":false
        }))
        .unwrap(),
    )
    .unwrap();
    eprintln!("Complete Loom installation evidence: {}", root.display());
}

#[cfg(windows)]
#[test]
fn cloud_install_application_large_file_updates_and_preserves_old_executable() {
    let root = unique_temp_dir("cloud-large-application");
    fs::create_dir_all(&root).unwrap();
    let resource = uuid::Uuid::new_v4().to_string();
    let mut previous: Option<PathBuf> = None;
    for (version, expected) in [("1.0.0", "alpha  beta"), ("1.1.0", "alpha beta")] {
        let mut package = application_test_package(&root, version, &resource);
        let mut old_zip = zip::ZipArchive::new(Cursor::new(&package.bytes)).unwrap();
        let mut files = Vec::new();
        for i in 1..old_zip.len() {
            let mut entry = old_zip.by_index(i).unwrap();
            let name = entry.name().unwrap().into_owned();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            files.push((name, bytes));
        }
        files.push(("data/large.bin".into(), vec![37u8; 80_000_000]));
        let manifest = json!({"schema":"neuro-software-package-v1","kind":"application","files":files.iter().map(|(path,bytes)|json!({"path":path,"size":bytes.len(),"sha256":sha256_bytes(bytes)})).collect::<Vec<_>>()});
        let path = root.join(format!("large-{version}.zip"));
        let mut writer = zip::ZipWriter::new(fs::File::create(&path).unwrap());
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        writer.start_file("manifest.json", options).unwrap();
        writer
            .write_all(&serde_json::to_vec(&manifest).unwrap())
            .unwrap();
        for (name, bytes) in files {
            writer.start_file(name, options).unwrap();
            writer.write_all(&bytes).unwrap();
        }
        writer.finish().unwrap();
        drop(old_zip);
        package.bytes.clear();
        let file = fs::File::open(&path).unwrap();
        let mut hash = Sha256::new();
        let mut read = &file;
        let mut buffer = [0u8; 65536];
        loop {
            let n = read.read(&mut buffer).unwrap();
            if n == 0 {
                break;
            }
            hash.update(&buffer[..n]);
        }
        package.download["sha256"] = json!(format!("{:x}", hash.finalize()));
        package.download["policy"] = json!("application-large-zip-format-v1");
        package.archive = Some(file);
        let state = asset_library::application::install(&root, &package).unwrap();
        let entry = PathBuf::from(state["entry"].as_str().unwrap());
        let output = std::process::Command::new(&entry)
            .arg("  alpha  beta  ")
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
        if let Some(old) = previous {
            assert!(old.is_file());
            assert_ne!(entry, old);
        }
        previous = Some(entry);
    }
    remove_test_dir(&root);
}

#[cfg(windows)]
#[test]
fn cloud_install_application_updates_and_runs_real_executable() {
    let root = unique_temp_dir("cloud-application");
    fs::create_dir_all(&root).unwrap();
    let resource = uuid::Uuid::new_v4().to_string();
    let runtime = test_daemon_runtime(&root, None);
    let trial: Option<Value> = std::env::var("LOOM_ASSETLIBRARY_APPLICATION_UPDATE_FIXTURE")
        .ok()
        .map(|path| serde_json::from_slice(&fs::read(path).unwrap()).unwrap());
    let mut old_entry = None;
    for (version, expected) in [("1.0.0", "alpha  beta"), ("1.1.0", "alpha beta")] {
        let package = match &trial {
            Some(config) => asset_library::prepare_trial_install(&config[version]).unwrap(),
            None => application_test_package(&root, version, &resource),
        };
        if trial.is_some() && std::env::var_os("LOOM_TEST_LIBARIA2").is_some() {
            assert_eq!(package.download["downloadEngine"], "libaria2");
        }
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
        let result: Value = serde_json::from_str(&body).unwrap();
        let entry = result["installation"]["entry"].as_str().unwrap();
        assert_eq!(result["installation"]["version"], version);
        assert_eq!(result["installation"]["trustStatus"], "unsigned");
        let output = std::process::Command::new(entry)
            .arg("  alpha  beta  ")
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
        if let Some(old) = old_entry {
            assert_ne!(entry, old);
            assert!(Path::new(&old).is_file());
        }
        old_entry = Some(entry.to_owned());
    }
    drop(runtime);
    remove_test_dir(&root);
}

#[cfg(windows)]
#[test]
fn cloud_install_application_rejects_wrong_version_and_unsigned_policy_without_switching() {
    let root = unique_temp_dir("cloud-application-failure");
    fs::create_dir_all(&root).unwrap();
    let resource = uuid::Uuid::new_v4().to_string();
    let mut package = application_test_package(&root, "1.0.0", &resource);
    let state = asset_library::application::install(&root, &package).unwrap();
    let active = root
        .join("applications")
        .join(&resource)
        .join("active.json");
    let before = fs::read(&active).unwrap();
    let valid_bytes = package.bytes.clone();
    package.bytes = b"not a zip".to_vec();
    package.download["sha256"] = json!(sha256_bytes(&package.bytes));
    assert!(asset_library::application::install(&root, &package).is_err());
    assert_eq!(fs::read(&active).unwrap(), before);
    package.bytes = valid_bytes;
    package.download["sha256"] = json!(sha256_bytes(&package.bytes));
    package.download["label"] = json!("2.0.0");
    assert!(asset_library::application::install(&root, &package).is_err());
    assert_eq!(fs::read(&active).unwrap(), before);
    package.download["label"] = json!("1.0.0");
    let mut trust = loom_plugin_security::TrustStore::default();
    trust.set_policy(loom_plugin_security::TrustPolicy::RequireTrusted);
    trust.write_atomic(&root.join("plugin-trust.json")).unwrap();
    assert!(asset_library::application::install(&root, &package).is_err());
    assert_eq!(fs::read(&active).unwrap(), before);
    assert!(Path::new(state["entry"].as_str().unwrap()).is_file());
    remove_test_dir(&root);
}
