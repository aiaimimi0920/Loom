use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use loom_plugin_security::{
    canonical_package_digest, generate_signing_key, sign_package, TrustPolicy, TrustStore,
};
use loom_protocol::{
    parse_capability_manifest, CapabilityRuntimeStatus, ExtensionTarget, PackageTrustStatus,
    PublisherTrustRecord,
};
use serde_json::json;
use uuid::Uuid;

use super::*;

#[cfg(windows)]
mod process_tree_tests;
mod runtime_health_tests;
mod schema_tests;
mod snapshot_tests;

#[test]
fn activates_invokes_and_tears_down_an_unknown_runtime() {
    let root = temp_root("invoke");
    let executable = compile_fixture(&root);
    let host = CapabilityRuntimeHost::new(RuntimeHostLimits::default());
    host.activate(package(&root, &executable, &[]))
        .expect("activate runtime");
    assert_eq!(host.active_plugin_count(), 1);
    assert_eq!(host.process_ids().len(), 1);

    let output = host
        .invoke(CapabilityInvocation {
            request_id: "invoke-unknown-1".to_owned(),
            command_id: "publisher.example/fixture.run".to_owned(),
            input: json!({ "value": 7 }),
            target: None,
            resource_refs: Vec::new(),
            user_gesture_token: None,
            timeout: Some(Duration::from_secs(2)),
        })
        .expect("invoke runtime");
    assert_eq!(output.status, CapabilityRuntimeStatus::Succeeded);
    assert_eq!(output.payload, Some(json!({ "ok": true })));

    assert!(host
        .deactivate("publisher.example/fixture")
        .expect("deactivate"));
    assert_eq!(host.active_plugin_count(), 0);
    assert!(host.process_ids().is_empty());
    cleanup(&root);
}

#[test]
fn rejects_dynamic_registration_outside_the_signed_envelope() {
    let root = temp_root("dynamic-envelope");
    let executable = compile_fixture(&root);
    let host = CapabilityRuntimeHost::new(RuntimeHostLimits::default());
    let error = host
        .activate(package(&root, &executable, &["extra"]))
        .expect_err("dynamic expansion must fail");
    assert!(error.to_string().contains("static envelope"));
    assert_eq!(host.active_plugin_count(), 0);
    assert!(host.process_ids().is_empty());
    cleanup(&root);
}

#[test]
fn rejects_dynamic_mutation_of_a_signed_command() {
    let root = temp_root("dynamic-mutation");
    let executable = compile_fixture(&root);
    let host = CapabilityRuntimeHost::new(RuntimeHostLimits::default());
    let error = host
        .activate(package(&root, &executable, &["mutate"]))
        .expect_err("dynamic mutation must fail");
    assert!(error.to_string().contains("signed static envelope"));
    assert_eq!(host.active_plugin_count(), 0);
    cleanup(&root);
}

#[test]
fn failed_replacement_preserves_the_previous_runtime() {
    let old_root = temp_root("replacement-old");
    let new_root = temp_root("replacement-new");
    let old_executable = compile_fixture(&old_root);
    let new_executable = compile_fixture(&new_root);
    let host = CapabilityRuntimeHost::new(RuntimeHostLimits::default());
    host.activate(package(&old_root, &old_executable, &[]))
        .expect("activate old runtime");

    host.activate(package(&new_root, &new_executable, &["extra"]))
        .expect_err("replacement must fail before commit");
    let output = invoke_fixture(&host, None, None).expect("old runtime remains callable");
    assert_eq!(output.status, CapabilityRuntimeStatus::Succeeded);
    assert_eq!(host.active_plugin_count(), 1);
    host.deactivate_all();
    cleanup(&old_root);
    cleanup(&new_root);
}

#[test]
fn rejects_package_tampering_before_spawn() {
    let root = temp_root("tampered");
    let executable = compile_fixture(&root);
    let package = package(&root, &executable, &[]);
    fs::write(&executable, b"tampered runtime").unwrap();
    let host = CapabilityRuntimeHost::new(RuntimeHostLimits::default());
    let error = host
        .activate(package)
        .expect_err("tampering must fail closed");
    assert!(error.to_string().contains("signature") || error.to_string().contains("digest"));
    assert!(host.process_ids().is_empty());
    cleanup(&root);
}

#[test]
fn gesture_tokens_are_target_bound_single_use_and_hidden_from_the_plugin() {
    let root = temp_root("gesture");
    let executable = compile_fixture(&root);
    let host = CapabilityRuntimeHost::new(RuntimeHostLimits::default());
    host.activate(package_with_gesture(&root, &executable, &[], true))
        .expect("activate runtime");
    let target = UserGestureTarget {
        unit_id: "unit-1".to_owned(),
        revision: 7,
    };
    let token = host
        .issue_user_gesture("publisher.example/fixture.run", Some(target.clone()))
        .expect("issue token");
    invoke_fixture(
        &host,
        Some(ExtensionTarget {
            unit_id: "unit-1".to_owned(),
            revision: 8,
        }),
        Some(token.clone()),
    )
    .expect_err("wrong target consumes token");
    invoke_fixture(
        &host,
        Some(ExtensionTarget {
            unit_id: "unit-1".to_owned(),
            revision: 7,
        }),
        Some(token),
    )
    .expect_err("consumed token cannot be replayed");

    let token = host
        .issue_user_gesture("publisher.example/fixture.run", Some(target))
        .expect("issue replacement token");
    invoke_fixture(
        &host,
        Some(ExtensionTarget {
            unit_id: "unit-1".to_owned(),
            revision: 7,
        }),
        Some(token.clone()),
    )
    .expect("bound token succeeds");
    invoke_fixture(
        &host,
        Some(ExtensionTarget {
            unit_id: "unit-1".to_owned(),
            revision: 7,
        }),
        Some(token),
    )
    .expect_err("successful token cannot be replayed");
    host.deactivate_all();
    cleanup(&root);
}

#[test]
fn timeout_terminates_the_owned_runtime_process() {
    let root = temp_root("timeout");
    let executable = compile_fixture(&root);
    let host = CapabilityRuntimeHost::new(RuntimeHostLimits::default());
    host.activate(package(&root, &executable, &["hang"]))
        .expect("activate runtime");
    let error = host
        .invoke(CapabilityInvocation {
            request_id: "invoke-timeout-1".to_owned(),
            command_id: "publisher.example/fixture.run".to_owned(),
            input: json!({}),
            target: None,
            resource_refs: Vec::new(),
            user_gesture_token: None,
            timeout: Some(Duration::from_millis(100)),
        })
        .expect_err("command must time out");
    assert!(matches!(error, CapabilityHostError::Timeout));
    assert!(host.process_ids().is_empty());
    host.deactivate_all();
    cleanup(&root);
}

#[test]
fn request_specific_cancel_forces_an_unresponsive_runtime_to_exit() {
    let root = temp_root("cancel");
    let executable = compile_fixture(&root);
    let host = Arc::new(CapabilityRuntimeHost::new(RuntimeHostLimits::default()));
    host.activate(package(&root, &executable, &["hang"]))
        .expect("activate runtime");
    let invoking_host = Arc::clone(&host);
    let invocation = thread::spawn(move || {
        invoking_host.invoke(CapabilityInvocation {
            request_id: "invoke-cancel-1".to_owned(),
            command_id: "publisher.example/fixture.run".to_owned(),
            input: json!({}),
            target: None,
            resource_refs: Vec::new(),
            user_gesture_token: None,
            timeout: Some(Duration::from_secs(30)),
        })
    });
    let mut cancelled = false;
    for _ in 0..100 {
        if host
            .cancel_request("invoke-cancel-1")
            .expect("cancel invocation")
        {
            cancelled = true;
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(cancelled, "invocation never became cancellable");
    assert!(invocation.join().expect("invocation thread").is_err());
    assert!(host.process_ids().is_empty());
    host.deactivate_all();
    cleanup(&root);
}

#[test]
fn oversized_frame_length_is_rejected_before_allocation() {
    let length = u32::try_from(loom_protocol::CAPABILITY_RUNTIME_FRAME_BYTES + 1).unwrap();
    let error =
        read_runtime_frame(&mut Cursor::new(length.to_be_bytes())).expect_err("oversized frame");
    assert!(matches!(error, CapabilityHostError::Protocol(_)));
}

fn package(root: &Path, executable: &Path, args: &[&str]) -> CapabilityRuntimePackage {
    package_with_gesture(root, executable, args, false)
}

fn package_with_gesture(
    root: &Path,
    executable: &Path,
    args: &[&str],
    requires_user_gesture: bool,
) -> CapabilityRuntimePackage {
    package_with_contract(
        root,
        executable,
        args,
        requires_user_gesture,
        None,
        None,
        &[],
        1,
    )
}

#[allow(clippy::too_many_arguments)]
fn package_with_contract(
    root: &Path,
    executable: &Path,
    args: &[&str],
    requires_user_gesture: bool,
    input_schema: Option<serde_json::Value>,
    output_schema: Option<serde_json::Value>,
    permissions: &[&str],
    max_processes: u32,
) -> CapabilityRuntimePackage {
    let file_name = executable.file_name().unwrap().to_string_lossy();
    let platform = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "windows-x64",
        ("linux", "x86_64") => "linux-x64",
        ("macos", "x86_64") => "macos-x64",
        ("macos", "aarch64") => "macos-arm64",
        other => panic!("unsupported test platform: {other:?}"),
    };
    let mut command = json!({
        "id": "publisher.example/fixture.run",
        "title": "Run fixture",
        "requiresUserGesture": requires_user_gesture,
        "permissions": permissions,
    });
    if input_schema.is_some() || output_schema.is_some() {
        fs::create_dir_all(root.join("schemas")).unwrap();
    }
    if let Some(schema) = input_schema {
        fs::write(
            root.join("schemas/input.schema.json"),
            serde_json::to_vec_pretty(&schema).unwrap(),
        )
        .unwrap();
        command["inputSchema"] = json!("schemas/input.schema.json");
    }
    if let Some(schema) = output_schema {
        fs::write(
            root.join("schemas/output.schema.json"),
            serde_json::to_vec_pretty(&schema).unwrap(),
        )
        .unwrap();
        command["outputSchema"] = json!("schemas/output.schema.json");
    }
    let manifest = json!({
        "schemaVersion": 1,
        "kind": "capability",
        "id": "fixture",
        "name": "Fixture",
        "description": "Runtime host fixture",
        "version": "1.0.0",
        "publisher": { "id": "publisher.example", "keyId": "test-key" },
        "hostCompatibility": {
            "loomCapabilityApi": { "minimum": "1.0" },
            "hookExtensionApi": { "minimum": "1.0" }
        },
        "entrypoints": {
            "service": {
                "targets": {
                    platform: {
                        "command": format!("runtime/{file_name}"),
                        "args": args
                    }
                },
                "processModel": "persistent"
            }
        },
        "contributes": {
            "commands": [command]
        },
        "permissions": permissions,
        "resources": { "memoryMiB": 64, "maxProcesses": max_processes, "timeoutSeconds": 5 },
        "dependencies": [],
        "signature": { "algorithm": "ed25519", "keyId": "test-key", "file": "signature.json" }
    });
    let manifest_bytes = serde_json::to_vec_pretty(&manifest).unwrap();
    fs::write(root.join("capability.manifest.json"), &manifest_bytes).unwrap();
    let key = generate_signing_key("test-key");
    sign_package(root, "signature.json", &key).unwrap();
    let trust_store_path = root.with_extension("trust.json");
    let mut trust = TrustStore::default();
    trust.set_policy(TrustPolicy::RequireTrusted);
    trust.trust(PublisherTrustRecord {
        publisher_id: "publisher.example".to_owned(),
        key_id: key.key_id.clone(),
        public_key: key.public_key.clone(),
        revoked: false,
    });
    trust.write_atomic(&trust_store_path).unwrap();
    let digest = canonical_package_digest(root, Some("signature.json")).unwrap();
    CapabilityRuntimePackage {
        manifest: parse_capability_manifest(&manifest_bytes).unwrap(),
        package_dir: root.to_path_buf(),
        digest: digest.clone(),
        trust_store_path,
        trust_status: PackageTrustStatus::Trusted,
        permission_grant_digest: digest,
    }
}

fn package_with_process_limit(
    root: &Path,
    executable: &Path,
    args: &[&str],
    max_processes: u32,
) -> CapabilityRuntimePackage {
    package_with_contract(
        root,
        executable,
        args,
        false,
        None,
        None,
        &[],
        max_processes,
    )
}

fn invoke_fixture(
    host: &CapabilityRuntimeHost,
    target: Option<ExtensionTarget>,
    user_gesture_token: Option<String>,
) -> Result<CapabilityInvocationOutput, CapabilityHostError> {
    host.invoke(CapabilityInvocation {
        request_id: format!("invoke-fixture-{}", Uuid::new_v4().simple()),
        command_id: "publisher.example/fixture.run".to_owned(),
        input: json!({ "value": 7 }),
        target,
        resource_refs: Vec::new(),
        user_gesture_token,
        timeout: Some(Duration::from_secs(2)),
    })
}

fn compile_fixture(root: &Path) -> PathBuf {
    let runtime = root.join("runtime");
    fs::create_dir_all(&runtime).expect("runtime directory");
    let source = runtime.join("fixture.rs");
    let executable = runtime.join(if cfg!(windows) {
        "fixture.exe"
    } else {
        "fixture"
    });
    fs::write(&source, FIXTURE_SOURCE).expect("fixture source");
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let status = Command::new(rustc)
        .arg(&source)
        .arg("-O")
        .arg("-o")
        .arg(&executable)
        .status()
        .expect("run rustc");
    assert!(status.success(), "compile fixture runtime");
    executable
}

fn temp_root(name: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "loom-capability-runtime-{name}-{}-{nonce}",
        std::process::id()
    ))
}

fn cleanup(root: &Path) {
    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_file(root.with_extension("trust.json"));
}

const FIXTURE_SOURCE: &str = r###"
use std::fs::OpenOptions;
use std::io::{Read, Write};
fn field(input: &str, key: &str) -> String {
    let marker = format!("\"{}\":\"", key);
    let rest = &input[input.find(&marker).unwrap() + marker.len()..];
    rest[..rest.find('\"').unwrap()].to_owned()
}
fn append_pid(path: &str) {
    let mut file = OpenOptions::new().create(true).append(true).open(path).unwrap();
    writeln!(file, "{}", std::process::id()).unwrap();
    file.flush().unwrap();
}
fn main() {
    let mode = std::env::args().nth(1).unwrap_or_default();
    if mode == "tree-child" || mode == "tree-leaf" {
        let pid_file = std::env::args().nth(2).unwrap();
        append_pid(&pid_file);
        if mode == "tree-child" {
            std::process::Command::new(std::env::current_exe().unwrap())
                .arg("tree-leaf")
                .arg(&pid_file)
                .spawn()
                .unwrap();
        }
        loop { std::thread::sleep(std::time::Duration::from_secs(30)); }
    }
    let mut input = std::io::stdin();
    let mut output = std::io::stdout();
    loop {
        let mut length = [0u8; 4];
        if input.read_exact(&mut length).is_err() { break; }
        let mut bytes = vec![0u8; u32::from_be_bytes(length) as usize];
        input.read_exact(&mut bytes).unwrap();
        let request = String::from_utf8(bytes).unwrap();
        let id = field(&request, "requestId");
        let method = field(&request, "method");
        if (mode == "tree" || mode == "crash-tree") && method == "command" {
            let pid_file = std::env::args().nth(2).unwrap();
            std::process::Command::new(std::env::current_exe().unwrap())
                .arg("tree-child")
                .arg(&pid_file)
                .spawn()
                .unwrap();
            for _ in 0..100 {
                let ready = std::fs::read_to_string(&pid_file)
                    .map(|value| value.lines().count() >= 2)
                    .unwrap_or(false);
                if ready { break; }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            if mode == "crash-tree" { std::process::exit(19); }
            std::thread::sleep(std::time::Duration::from_secs(30));
        }
        if mode == "hang" && method == "command" {
            std::thread::sleep(std::time::Duration::from_secs(30));
        }
        let (status, payload, error) = if mode == "fail-secret" && method == "command" {
            ("failed", "null", r#","error":{"code":"runtime_fault","message":"token=fixture-secret","retryable":false}"#)
        } else if mode == "extra" && method == "initialize" {
            ("succeeded", r#"{"contributions":{"commands":[{"id":"publisher.example/fixture.extra","title":"Extra"}]}}"#, "")
        } else if mode == "mutate" && method == "initialize" {
            ("succeeded", r#"{"contributions":{"commands":[{"id":"publisher.example/fixture.run","title":"Mutated"}]}}"#, "")
        } else if mode == "notice" {
            ("succeeded", r#"{"output":{"ok":true},"effects":[{"type":"notice.show","payload":{"message":"ready"}}]}"#, "")
        } else if mode == "clipboard" {
            ("succeeded", r#"{"output":{"ok":true},"effects":[{"type":"clipboard.writeText","payload":{"text":"ready"}}]}"#, "")
        } else if mode == "bad-output" {
            ("succeeded", r#"{"output":{"ok":"wrong"}}"#, "")
        } else { ("succeeded", r#"{"ok":true}"#, "") };
        let response = format!(
            "{{\"type\":\"response\",\"protocol\":\"loom.capability.runtime.v1\",\"apiVersion\":\"1.0\",\"requestId\":\"{}\",\"status\":\"{}\",\"payload\":{}{} }}",
            id, status, payload, error
        );
        output.write_all(&(response.len() as u32).to_be_bytes()).unwrap();
        output.write_all(response.as_bytes()).unwrap();
        output.flush().unwrap();
        if method == "deactivate" { break; }
    }
}
"###;
