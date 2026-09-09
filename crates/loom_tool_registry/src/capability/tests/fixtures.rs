use std::fs;
use std::io::{Cursor, Write as _};
use std::path::{Path, PathBuf};

use loom_plugin_security::{sign_package, TrustPolicy, TrustStore};
use loom_protocol::PublisherTrustRecord;
use serde_json::{json, Value};
use zip::write::SimpleFileOptions;

use super::*;

pub(super) fn manifest(kind: &str, version: &str) -> Value {
    json!({
        "schemaVersion": 1,
        "kind": kind,
        "id": "text-tools",
        "name": "Text Tools",
        "description": "Test package",
        "version": version,
        "publisher": { "id": "publisher.example", "keyId": "release-1" },
        "hostCompatibility": {
            "loomCapabilityApi": { "minimum": "1.0" },
            "hookExtensionApi": { "minimum": "1.0" }
        },
        "entrypoints": {
            "service": {
                "targets": {
                    "windows-x64": { "command": "runtime/text-tools.exe" }
                },
                "processModel": "on_demand"
            }
        },
        "activationEvents": ["onCommand:publisher.example/text-tools.transform"],
        "contributes": {
            "commands": [{
                "id": "publisher.example/text-tools.transform",
                "title": "Transform text"
            }]
        },
        "permissions": [],
        "resources": { "memoryMiB": 64, "maxProcesses": 1, "timeoutSeconds": 10 },
        "dependencies": [],
        "signature": {
            "algorithm": "ed25519",
            "keyId": "release-1",
            "file": "signature.json"
        }
    })
}

pub(super) fn signed_package(
    root: &Path,
    key: &loom_plugin_security::SigningKeyDocument,
    manifest: Value,
    payload: &[u8],
) -> Vec<u8> {
    let package = root.join("fixture-package");
    let _ = fs::remove_dir_all(&package);
    fs::create_dir_all(package.join("runtime/resources/ocr")).expect("package dirs");
    fs::write(
        package.join("capability.manifest.json"),
        serde_json::to_vec_pretty(&manifest).expect("manifest JSON"),
    )
    .expect("manifest");
    fs::write(package.join("runtime/text-tools.exe"), payload).expect("runtime");
    fs::write(
        package.join("runtime/resources/ocr/text-model.onnx"),
        b"model",
    )
    .expect("model");
    sign_package(&package, "signature.json", key).expect("sign package");
    zip_directory(&package)
}

fn zip_directory(directory: &Path) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut bytes));
        let options = SimpleFileOptions::default();
        for name in [
            "capability.manifest.json",
            "runtime/text-tools.exe",
            "runtime/resources/ocr/text-model.onnx",
            "signature.json",
        ] {
            writer.start_file(name, options).expect("zip entry");
            writer
                .write_all(&fs::read(directory.join(name)).expect("package file"))
                .expect("zip content");
        }
        writer.finish().expect("finish zip");
    }
    bytes
}

pub(super) fn replace_zip_entry(bytes: &[u8], target: &str, replacement: &[u8]) -> Vec<u8> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).expect("read zip");
    let mut output = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut output));
        let options = SimpleFileOptions::default();
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).expect("zip entry");
            let name = entry.name().to_owned();
            writer.start_file(&name, options).expect("start entry");
            if name == target {
                writer.write_all(replacement).expect("replacement");
            } else {
                std::io::copy(&mut entry, &mut writer).expect("copy entry");
            }
        }
        writer.finish().expect("finish zip");
    }
    output
}

pub(super) fn write_trust_store(
    root: &Path,
    key: &loom_plugin_security::SigningKeyDocument,
    policy: TrustPolicy,
) {
    fs::create_dir_all(root).expect("control root");
    let mut store = TrustStore::default();
    store.set_policy(policy);
    store.trust(PublisherTrustRecord {
        publisher_id: "publisher.example".to_owned(),
        key_id: key.key_id.clone(),
        public_key: key.public_key.clone(),
        revoked: false,
    });
    store
        .write_atomic(&root.join("plugin-trust.json"))
        .expect("trust store");
}

pub(super) fn assert_staging_empty(registry: &CapabilityPluginRegistry) {
    let staging = registry.packages_root().join(".staging");
    assert_eq!(fs::read_dir(staging).expect("staging").count(), 0);
}

pub(super) fn assert_lifecycle_journals_empty(registry: &CapabilityPluginRegistry) {
    let lifecycle = registry.packages_root().join(".lifecycle");
    let journals = fs::read_dir(lifecycle)
        .expect("lifecycle")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().and_then(|value| value.to_str()) == Some("json"))
        .count();
    assert_eq!(journals, 0);
}

pub(super) fn temp_root(name: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "loom-capability-{name}-{}-{nonce}",
        std::process::id()
    ))
}

pub(super) fn cleanup(root: &Path) {
    let _ = crate::install::fs_safety::set_tree_readonly(root, false);
    let _ = fs::remove_dir_all(root);
}
