use super::*;

static TEMP_ROOT_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub(super) fn temp_root() -> PathBuf {
    for _ in 0..1024 {
        let sequence = TEMP_ROOT_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "loom-art-install-test-{}-{sequence}",
            std::process::id()
        ));
        match std::fs::create_dir(&path) {
            Ok(()) => return path,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("create Art install test root: {error}"),
        }
    }
    panic!("cannot reserve a unique Art install test root")
}

pub(super) fn build_zip(manifest: &str, extra: &[(&str, &[u8])]) -> Vec<u8> {
    let mut manifest: serde_json::Value = serde_json::from_str(manifest).unwrap();
    let art_id = manifest
        .get("id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let metadata = manifest
        .as_object_mut()
        .unwrap()
        .entry("metadata")
        .or_insert_with(|| serde_json::json!({}));
    let metadata = metadata.as_object_mut().unwrap();
    let security = metadata
        .entry("packageSecurity")
        .or_insert_with(|| serde_json::json!({}));
    security
        .as_object_mut()
        .unwrap()
        .entry("publisher")
        .or_insert_with(|| serde_json::json!({ "id": "publisher.test" }));
    security
        .as_object_mut()
        .unwrap()
        .entry("version")
        .or_insert_with(|| serde_json::json!("0.1.0"));
    let publisher_id = security
        .get("publisher")
        .and_then(|publisher| publisher.get("id"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("publisher.test")
        .to_owned();
    if !metadata.contains_key("art") {
        metadata.insert(
            "art".to_owned(),
            serde_json::json!({ "qualifiedId": format!("{publisher_id}/{art_id}") }),
        );
    }
    let mut buf = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut buf));
        let opts = SimpleFileOptions::default();
        writer.start_file(MANIFEST_NAME, opts).unwrap();
        writer
            .write_all(serde_json::to_string(&manifest).unwrap().as_bytes())
            .unwrap();
        for (name, bytes) in extra {
            writer.start_file(*name, opts).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap();
    }
    buf
}

pub(super) fn install_test_mcp_package(root: &Path) -> loom_mcp::McpServerConfig {
    let manifest = r#"{
            "schemaVersion":1,
            "id":"fixture-server",
            "name":"Fixture Server",
            "version":"1.2.3",
            "publisher":{"id":"publisher.test","name":"Publisher Test"},
            "transport":"stdio",
            "entry":{"command":"runtime/server.ps1","args":[]}
        }"#;
    let mut bytes = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut bytes));
        let options = SimpleFileOptions::default();
        writer
            .start_file(loom_mcp::package::MCP_SERVER_PACKAGE_MANIFEST, options)
            .unwrap();
        writer.write_all(manifest.as_bytes()).unwrap();
        writer.start_file("runtime/server.ps1", options).unwrap();
        writer.write_all(b"Write-Output ready").unwrap();
        writer.finish().unwrap();
    }
    let config =
        loom_mcp::package::install_server_package(root, &bytes).expect("install test MCP package");
    std::fs::create_dir_all(root.join("mcp")).expect("MCP registry directory");
    std::fs::write(
        root.join("mcp/servers.json"),
        serde_json::to_vec_pretty(std::slice::from_ref(&config)).unwrap(),
    )
    .expect("MCP server registry");
    config
}

pub(super) fn signed_art_zip(
    id: &str,
    version: &str,
    publisher: &str,
    payload: &[u8],
    key: &loom_plugin_security::SigningKeyDocument,
) -> Vec<u8> {
    let package_root = temp_root();
    let package = package_root.join("signed-art");
    std::fs::create_dir_all(package.join("bin")).expect("package directory");
    let manifest = serde_json::json!({
        "id": id,
        "name": "Signed Art",
        "description": "signed rollback fixture",
        "enabled": true,
        "execution": { "type": "framework_art", "framework": "process" },
        "metadata": {
            "art": { "qualifiedId": format!("{publisher}/{id}") },
            "packageSecurity": {
                "version": version,
                "publisher": { "id": publisher, "keyId": key.key_id.clone() },
                "signature": {
                    "algorithm": "ed25519",
                    "keyId": key.key_id.clone(),
                    "file": "signature.json"
                }
            }
        }
    });
    std::fs::write(
        package.join(MANIFEST_NAME),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .expect("manifest");
    std::fs::write(package.join("bin/tool.exe"), payload).expect("payload");
    loom_plugin_security::sign_package(&package, "signature.json", key).expect("sign Art");

    let mut bytes = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut bytes));
        let options = SimpleFileOptions::default();
        for relative in [MANIFEST_NAME, "bin/tool.exe", "signature.json"] {
            writer.start_file(relative, options).unwrap();
            writer
                .write_all(&std::fs::read(package.join(relative)).unwrap())
                .unwrap();
        }
        writer.finish().unwrap();
    }
    std::fs::remove_dir_all(package_root).ok();
    bytes
}

pub(super) fn install_test_framework(framework: &FrameworkRegistry, id: &str) {
    let command = match id {
        "process" => "runtime/loom-framework-process.exe",
        "cloud_api" => "runtime/loom-framework-cloud-api.exe",
        "mcp" => "runtime/loom-framework-mcp.exe",
        "workflow" => "runtime/loom-framework-workflow.exe",
        other => panic!("unknown test framework: {other}"),
    };
    let manifest = serde_json::json!({
        "id": id,
        "name": format!("{id} test framework"),
        "description": "test framework",
        "version": "0.1.0",
        "publisher": { "id": "publisher.test", "name": "Publisher Test" },
        "protocolVersion": "loom.framework.v1",
        "platforms": ["windows-x64"],
        "entry": {
            "kind": "process",
            "command": command,
            "args": ["--stdio"],
            "processModel": "per_execution"
        },
        "permissions": ["process.spawn"],
        "artExecution": {
            "requestSchema": "loom.art.execute.v1",
            "responseSchema": "loom.art.result.v1"
        }
    });
    let mut bytes = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut bytes));
        let opts = SimpleFileOptions::default();
        writer.start_file("framework.manifest.json", opts).unwrap();
        writer
            .write_all(serde_json::to_string(&manifest).unwrap().as_bytes())
            .unwrap();
        writer.start_file(command, opts).unwrap();
        writer.write_all(b"MZ-test-framework").unwrap();
        writer.finish().unwrap();
    }
    framework
        .install_framework_package_from_zip(&bytes)
        .expect("install test framework");
}

/// Installs the same test framework as `install_test_framework`, but signed.
///
/// A framework package is checked against the trust policy every time its readiness is probed,
/// not only when it is installed, so a test that sets a strict policy needs a framework whose
/// signature satisfies that policy — otherwise every Art install in the test fails on framework
/// readiness before reaching the behaviour under test.
pub(super) fn install_signed_test_framework(
    framework: &FrameworkRegistry,
    id: &str,
    key: &loom_plugin_security::SigningKeyDocument,
) {
    let command = match id {
        "process" => "runtime/loom-framework-process.exe",
        "cloud_api" => "runtime/loom-framework-cloud-api.exe",
        "mcp" => "runtime/loom-framework-mcp.exe",
        "workflow" => "runtime/loom-framework-workflow.exe",
        other => panic!("unknown test framework: {other}"),
    };
    let package_root = temp_root();
    let package = package_root.join("signed-framework");
    std::fs::create_dir_all(package.join("runtime")).expect("package directory");
    let manifest = serde_json::json!({
        "id": id,
        "name": format!("{id} signed test framework"),
        "description": "signed test framework",
        "version": "0.1.0",
        "publisher": {
            "id": "publisher.test",
            "name": "Publisher Test",
            "keyId": key.key_id.clone()
        },
        "signature": {
            "algorithm": "ed25519",
            "keyId": key.key_id.clone(),
            "file": "signature.json"
        },
        "protocolVersion": "loom.framework.v1",
        "platforms": ["windows-x64"],
        "entry": {
            "kind": "process",
            "command": command,
            "args": ["--stdio"],
            "processModel": "per_execution"
        },
        "permissions": ["process.spawn"],
        "artExecution": {
            "requestSchema": "loom.art.execute.v1",
            "responseSchema": "loom.art.result.v1"
        }
    });
    std::fs::write(
        package.join("framework.manifest.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .expect("framework manifest");
    std::fs::write(package.join(command), b"MZ-test-framework").expect("framework entry");
    loom_plugin_security::sign_package(&package, "signature.json", key)
        .expect("sign framework package");

    let mut bytes = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut bytes));
        let options = SimpleFileOptions::default();
        for relative in ["framework.manifest.json", command, "signature.json"] {
            writer.start_file(relative, options).unwrap();
            writer
                .write_all(&std::fs::read(package.join(relative)).unwrap())
                .unwrap();
        }
        writer.finish().unwrap();
    }
    std::fs::remove_dir_all(package_root).ok();
    framework
        .install_framework_package_from_zip(&bytes)
        .expect("install signed test framework");
}
