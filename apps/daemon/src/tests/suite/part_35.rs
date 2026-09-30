// Capability API package and external process fixture helpers.
fn capability_api_fixture(
    root: &Path,
    key: &loom_plugin_security::SigningKeyDocument,
) -> Vec<u8> {
    capability_api_fixture_version(
        root,
        key,
        "1.0.0",
        &["hook.unit.attachments.write", "hook.notice.show"],
    )
}

fn capability_api_fixture_version(
    root: &Path,
    key: &loom_plugin_security::SigningKeyDocument,
    version: &str,
    permissions: &[&str],
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
        "version": version,
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
            "commands": [
                {
                    "id": "publisher.example/api-fixture.run",
                    "title": "Run fixture",
                    "permissions": permissions
                },
                {
                    "id": "publisher.example/api-fixture.notify",
                    "title": "Notify from fixture",
                    "permissions": ["hook.notice.show"]
                }
            ],
            "shortcuts": [{
                "id": "publisher.example/api-fixture.default-shortcut",
                "command": "publisher.example/api-fixture.run",
                "title": "Run fixture",
                "payload": { "keys": "Ctrl+Alt+A" }
            }],
            "menus": [{
                "id": "publisher.example/api-fixture.unit-toolbar",
                "command": "publisher.example/api-fixture.run",
                "title": "Run fixture",
                "placement": "hook.unit.toolbar",
                "order": 20
            }],
            "settings": [{
                "id": "publisher.example/api-fixture.density",
                "title": "Result density",
                "payload": {
                    "type": "enum",
                    "options": ["balanced", "comfortable", "compact"],
                    "default": "balanced",
                    "description": "Controls result spacing"
                }
            }],
            "dataTypes": [{
                "id": "publisher.example/api-fixture.result.v1",
                "schema": "data-type.v1",
                "payload": {
                    "jsonSchema": {
                        "type": "object",
                        "properties": { "text": { "type": "string" } },
                        "additionalProperties": false
                    }
                }
            }],
            "renderers": [{
                "id": "publisher.example/api-fixture.result-card",
                "schema": "renderer.v1",
                "payload": {
                    "typeId": "publisher.example/api-fixture.result.v1",
                    "bounds": { "x": 8, "y": 8, "width": 180, "height": 44 },
                    "scene": {
                        "id": "fixture-result",
                        "type": "text",
                        "props": { "text": "Fixture result attached" }
                    }
                }
            }],
            "unitOverlays": [{
                "id": "publisher.example/api-fixture.notify-overlay",
                "command": "publisher.example/api-fixture.notify",
                "schema": "unit-overlay.v1",
                "payload": {
                    "typeId": "publisher.example/api-fixture.result.v1",
                    "bounds": { "x": 8, "y": 56, "width": 180, "height": 36 },
                    "scene": {
                        "id": "fixture-notify",
                        "type": "button",
                        "props": { "label": "Notify" },
                        "events": { "click": "publisher.example/api-fixture.notify" }
                    }
                }
            }]
        },
        "permissions": permissions,
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

fn array_field(input: &str, key: &str) -> String {
    let marker = format!("\"{}\":[", key);
    let Some(start) = input.find(&marker) else { return "[]".to_owned(); };
    let rest = &input[start + marker.len()..];
    format!("[{}]", &rest[..rest.find(']').unwrap()])
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
        let effects = if request.contains("publisher.example/api-fixture.notify") {
            r#"[{"type":"notice.show","payload":{"title":"Fixture","message":"Overlay command completed"}}]"#.to_owned()
        } else {
            r#"[{"type":"attachment.upsert","payload":{"attachmentId":"publisher.example/api-fixture.fixture-result","typeId":"publisher.example/api-fixture.result.v1","schemaVersion":"1.0","priorRevision":0,"revision":1,"rendererId":"publisher.example/api-fixture.result-card","payload":{"text":"hello"},"resourceRefs":__RESOURCE_REFS__}},{"type":"notice.show","payload":{"title":"Fixture","message":"Attachment created"}}]"#
                .replace("__RESOURCE_REFS__", &array_field(&request, "resourceRefs"))
        };
        let response = format!(
            "{{\"type\":\"response\",\"protocol\":\"loom.capability.runtime.v1\",\"apiVersion\":\"1.0\",\"requestId\":\"{}\",\"status\":\"succeeded\",\"payload\":{{\"output\":{{\"ok\":true}},\"effects\":{}}}}}",
            request_id, effects
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
