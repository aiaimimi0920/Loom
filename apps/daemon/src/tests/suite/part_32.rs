// Capability Plugin lifecycle API coverage.
#[test]
fn capability_plugin_api_installs_configures_enables_and_uninstalls() {
    let root = unique_temp_dir("capability-api");
    fs::create_dir_all(&root).expect("control root");
    let key = loom_plugin_security::generate_signing_key("release-1");
    let mut trust = loom_plugin_security::TrustStore::default();
    trust.trust(PublisherTrustRecord {
        publisher_id: "publisher.example".to_owned(),
        key_id: key.key_id.clone(),
        public_key: key.public_key.clone(),
        revoked: false,
    });
    trust
        .write_atomic(&root.join("plugin-trust.json"))
        .expect("trust store");
    let archive = capability_api_fixture(&root, &key);
    let daemon_runtime = test_daemon_runtime(&root, None);
    let runtime = Arc::clone(&daemon_runtime.capability_runtime);
    let resources = Arc::clone(&daemon_runtime.capability_resources);
    let body = json!({
        "zipBase64": format!(
            "data:application/zip;base64,{}",
            BASE64.encode(&archive)
        )
    })
    .to_string();

    let (status, installed) =
        install_capability_plugin(&body, &root).expect("install response");
    assert_eq!(status, 200, "{installed}");
    let installed: Value = serde_json::from_str(&installed).expect("install JSON");
    let digest = installed["package"]["digest"]
        .as_str()
        .expect("package digest");
    assert_eq!(
        installed["package"]["qualifiedId"],
        "publisher.example/api-fixture"
    );
    loom_tool_registry::capability::CapabilityPluginRegistry::new(&root)
        .verify_installed_version("publisher.example/api-fixture", digest)
        .expect("installed package remains verifiable");

    let (extension_events, _extension_subscription) = enable_api_fixture_through_extension_route(
        &daemon_runtime,
        &root,
        &runtime,
        &resources,
        digest,
    );
    let (status, snapshot) = capability_extension_snapshot(&runtime).expect("extension snapshot");
    assert_eq!(status, 200);
    let snapshot: Value = serde_json::from_str(&snapshot).unwrap();
    assert_eq!(snapshot["snapshot"]["generation"], 1);
    assert_eq!(
        snapshot["snapshot"]["plugins"][0]["id"],
        "publisher.example/api-fixture"
    );
    assert_eq!(
        snapshot["snapshot"]["contributions"]["commands"][0]["id"],
        "publisher.example/api-fixture.run"
    );

    assert_api_fixture_extension_invocation(&runtime);

    let invoked = expect_json_text_route_response(
        route_request(
            &daemon_runtime,
            &parsed_request(
                "POST",
                "/v1/invoke",
                &[],
                Some(
                    &json!({
                        "requestId": "api-fixture-success",
                        "caller": "hook",
                        "capability": "publisher.example/api-fixture.run",
                        "input": { "text": "hello" }
                    })
                    .to_string(),
                ),
            ),
        ),
        200,
    );
    assert_eq!(invoked["status"], "succeeded");
    assert_eq!(invoked["pluginId"], "publisher.example/api-fixture");
    assert_eq!(invoked["output"]["ok"], true);

    let timed_out = expect_json_text_route_response(
        route_request(
            &daemon_runtime,
            &parsed_request(
                "POST",
                "/v1/invoke",
                &[],
                Some(
                    &json!({
                        "requestId": "api-fixture-timeout",
                        "caller": "hook",
                        "capability": "publisher.example/api-fixture.run",
                        "input": { "hang": true },
                        "timeoutMs": 100
                    })
                    .to_string(),
                ),
            ),
        ),
        504,
    );
    assert_eq!(timed_out["error"]["code"], "capability_timeout");
    assert!(runtime.process_ids().is_empty());
    assert_eq!(
        loom_tool_registry::capability::CapabilityPluginRegistry::new(&root)
            .get("publisher.example/api-fixture")
            .unwrap()
            .unwrap()
            .runtime_failures
            .count,
        1
    );

    let crashed = expect_json_text_route_response(
        route_request(
            &daemon_runtime,
            &parsed_request(
                "POST",
                "/v1/invoke",
                &[],
                Some(
                    &json!({
                        "requestId": "api-fixture-crash",
                        "caller": "hook",
                        "capability": "publisher.example/api-fixture.run",
                        "input": { "crash": true }
                    })
                    .to_string(),
                ),
            ),
        ),
        503,
    );
    assert_eq!(
        crashed["error"]["code"],
        "capability_runtime_unavailable"
    );
    assert!(runtime.process_ids().is_empty());
    assert_eq!(
        loom_tool_registry::capability::CapabilityPluginRegistry::new(&root)
            .get("publisher.example/api-fixture")
            .unwrap()
            .unwrap()
            .runtime_failures
            .count,
        2
    );

    let (status, config) = update_capability_config(
        "publisher.example/api-fixture",
        &json!({
            "expectedRevision": 0,
            "values": { "language": "zh-CN" }
        })
        .to_string(),
        &root,
    )
    .expect("config response");
    assert_eq!(status, 200);
    assert_eq!(
        serde_json::from_str::<Value>(&config).unwrap()["config"]["revision"],
        1
    );

    let (status, listed) = list_capability_plugins(&root).expect("list response");
    assert_eq!(status, 200);
    assert_eq!(
        serde_json::from_str::<Value>(&listed).unwrap()["plugins"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    for _ in 2..loom_tool_registry::capability::CAPABILITY_MAX_RUNTIME_FAILURES {
        record_capability_runtime_failure(
            &root,
            "publisher.example/api-fixture",
            &runtime,
            &resources,
            &loom_capability_runtime::CapabilityHostError::Timeout,
        );
    }
    let faulted = loom_tool_registry::capability::CapabilityPluginRegistry::new(&root)
        .get("publisher.example/api-fixture")
        .unwrap()
        .unwrap();
    assert_eq!(
        faulted.status,
        loom_tool_registry::capability::CapabilityLifecycleStatus::Faulted
    );
    assert!(runtime.contribution_snapshot().unwrap().plugins.is_empty());
    let (_, retried) = enable_capability_plugin(
        "publisher.example/api-fixture",
        &json!({ "digest": digest }).to_string(),
        &root,
        &runtime,
    )
    .expect("retry faulted plugin");
    assert_eq!(
        serde_json::from_str::<Value>(&retried).unwrap()["plugin"]["runtimeFailures"]["count"],
        0
    );
    disable_api_fixture_through_extension_route(
        &daemon_runtime,
        &root,
        &runtime,
        &resources,
        &extension_events,
    );
    let (status, _) = uninstall_capability_plugin(
        "publisher.example/api-fixture",
        &root,
        &runtime,
        &resources,
    )
    .expect("uninstall");
    assert_eq!(status, 200);
    assert!(runtime.process_ids().is_empty());
    assert!(
        loom_tool_registry::capability::CapabilityPluginRegistry::new(&root)
            .list()
            .expect("registry")
            .is_empty()
    );
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn capability_plugin_api_rejects_unknown_fields_and_routes_only_its_namespace() {
    let root = unique_temp_dir("capability-api-invalid");
    let runtime = Arc::new(CapabilityRuntimeHost::new(RuntimeHostLimits::default()));
    let resources = CapabilityResourceBroker::open(root.join("capability-resources"))
        .expect("Capability resource broker");
    let (status, body) = install_capability_plugin(
        r#"{"zipBase64":"bad","unexpected":true}"#,
        &root,
    )
    .expect("error response");
    assert_eq!(status, 400);
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["error"]["code"],
        "invalid_capability_install"
    );
    let request = ParsedHttpRequest {
        method: "GET".to_owned(),
        path: "/v1/unrelated".to_owned(),
        headers: Vec::new(),
        body: String::new(),
    };
    assert!(
        route_capability_plugins(
            &request,
            "/v1/unrelated",
            &root,
            &runtime,
            &resources,
            &Arc::new(Mutex::new(HookBridgeRuntime::new(root.clone()))),
        )
        .is_none()
    );
}

fn capability_api_fixture(
    root: &Path,
    key: &loom_plugin_security::SigningKeyDocument,
) -> Vec<u8> {
    let package = root.join("api-fixture-package");
    fs::create_dir_all(package.join("runtime")).expect("package dirs");
    fs::write(package.join("ui.surface.json"), b"{}\n").expect("Surface manifest");
    let (platform, executable_name) = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => ("windows-x64", "api-fixture.exe"),
        ("linux", "x86_64") => ("linux-x64", "api-fixture"),
        ("macos", "x86_64") => ("macos-x64", "api-fixture"),
        ("macos", "aarch64") => ("macos-arm64", "api-fixture"),
        other => panic!("unsupported capability API test platform: {other:?}"),
    };
    compile_capability_api_fixture(&package.join("runtime").join(executable_name));
    let mut targets = serde_json::Map::new();
    targets.insert(
        platform.to_owned(),
        json!({ "command": format!("runtime/{executable_name}") }),
    );
    let manifest = json!({
        "schemaVersion": 1,
        "kind": "capability",
        "id": "api-fixture",
        "name": "API Fixture",
        "description": "Lifecycle API fixture",
        "version": "1.0.0",
        "publisher": { "id": "publisher.example", "keyId": key.key_id },
        "hostCompatibility": {
            "loomCapabilityApi": { "minimum": "1.0" },
            "hookExtensionApi": { "minimum": "1.0" }
        },
        "entrypoints": {
            "service": {
                "targets": Value::Object(targets),
                "processModel": "on_demand"
            },
            "hookUi": { "kind": "surface", "manifest": "ui.surface.json" }
        },
        "contributes": {
            "commands": [{
                "id": "publisher.example/api-fixture.run",
                "title": "Run fixture"
            }]
        },
        "permissions": [],
        "resources": { "memoryMiB": 64, "maxProcesses": 1, "timeoutSeconds": 10 },
        "dependencies": [],
        "signature": {
            "algorithm": "ed25519",
            "keyId": key.key_id,
            "file": "signature.json"
        }
    });
    fs::write(
        package.join("capability.manifest.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .expect("manifest");
    loom_plugin_security::sign_package(&package, "signature.json", key).expect("sign fixture");
    let mut bytes = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut bytes));
        for relative in [
            "capability.manifest.json".to_owned(),
            "ui.surface.json".to_owned(),
            format!("runtime/{executable_name}"),
            "signature.json".to_owned(),
        ] {
            let options = zip::write::SimpleFileOptions::default();
            #[cfg(unix)]
            let options = {
                use std::os::unix::fs::PermissionsExt as _;
                options.unix_permissions(
                    fs::metadata(package.join(&relative))
                        .expect("package metadata")
                        .permissions()
                        .mode(),
                )
            };
            writer.start_file(&relative, options).expect("zip entry");
            writer
                .write_all(&fs::read(package.join(&relative)).expect("package file"))
                .expect("zip content");
        }
        writer.finish().expect("finish zip");
    }
    bytes
}

fn compile_capability_api_fixture(executable: &Path) {
    let source = executable.with_extension("rs");
    fs::write(&source, CAPABILITY_API_FIXTURE_SOURCE).expect("runtime source");
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let status = Command::new(rustc)
        .arg(&source)
        .arg("-O")
        .arg("-o")
        .arg(executable)
        .status()
        .expect("run rustc");
    assert!(status.success(), "compile Capability API fixture runtime");
    fs::remove_file(source).expect("remove runtime source");
    let _ = fs::remove_file(executable.with_extension("pdb"));
}

const CAPABILITY_API_FIXTURE_SOURCE: &str = r###"
use std::io::{Read, Write};

fn field(input: &str, key: &str) -> String {
    let marker = format!("\"{}\":\"", key);
    let rest = &input[input.find(&marker).unwrap() + marker.len()..];
    rest[..rest.find('\"').unwrap()].to_owned()
}

fn main() {
    let mut input = std::io::stdin();
    let mut output = std::io::stdout();
    loop {
        let mut length = [0u8; 4];
        if input.read_exact(&mut length).is_err() {
            break;
        }
        let mut bytes = vec![0u8; u32::from_be_bytes(length) as usize];
        input.read_exact(&mut bytes).unwrap();
        let request = String::from_utf8(bytes).unwrap();
        let request_id = field(&request, "requestId");
        let method = field(&request, "method");
        if method == "command" && request.contains("\"crash\":true") {
            std::process::exit(19);
        }
        if method == "command" && request.contains("\"hang\":true") {
            std::thread::sleep(std::time::Duration::from_secs(30));
        }
        let response = format!(
            "{{\"type\":\"response\",\"protocol\":\"loom.capability.runtime.v1\",\"apiVersion\":\"1.0\",\"requestId\":\"{}\",\"status\":\"succeeded\",\"payload\":{{\"ok\":true}}}}",
            request_id
        );
        output.write_all(&(response.len() as u32).to_be_bytes()).unwrap();
        output.write_all(response.as_bytes()).unwrap();
        output.flush().unwrap();
        if method == "deactivate" {
            break;
        }
    }
}
"###;
