use std::fs;
use std::io::{Cursor, Write as _};
use std::path::{Path, PathBuf};

use loom_plugin_security::{generate_signing_key, sign_package, TrustPolicy, TrustStore};
use loom_protocol::{PackageTrustStatus, PublisherTrustRecord};
use serde_json::{json, Value};
use zip::write::SimpleFileOptions;

use super::*;

#[test]
fn installs_trusted_package_disabled_and_reuses_identical_version() {
    let root = temp_root("trusted");
    let key = generate_signing_key("release-1");
    write_trust_store(&root, &key, TrustPolicy::RequireTrusted);
    let registry = CapabilityPluginRegistry::new(&root);
    let package = signed_package(&root, &key, manifest("capability", "1.0.0"), b"runtime");

    let first = install_capability_from_zip(&package, &registry).expect("first install");
    let second = install_capability_from_zip(&package, &registry).expect("idempotent install");

    assert_eq!(first.qualified_id, "publisher.example/text-tools");
    assert_eq!(first.digest, second.digest);
    assert_eq!(first.trust_status, PackageTrustStatus::Trusted);
    assert!(first.package_dir.join("runtime/text-tools.exe").is_file());
    assert!(
        fs::metadata(first.package_dir.join("capability.manifest.json"))
            .expect("manifest metadata")
            .permissions()
            .readonly()
    );
    let records = registry.list().expect("registry list");
    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0].status,
        CapabilityLifecycleStatus::InstalledDisabled
    );
    assert!(!records[0].enabled_intent);
    assert_eq!(records[0].versions.len(), 1);
    cleanup(&root);
}

#[test]
fn rejects_type_confusion_and_removes_partial_staging_tree() {
    let root = temp_root("type-confusion");
    let key = generate_signing_key("release-1");
    write_trust_store(&root, &key, TrustPolicy::RequireTrusted);
    let registry = CapabilityPluginRegistry::new(&root);
    let package = signed_package(&root, &key, manifest("art", "1.0.0"), b"runtime");

    let error = install_capability_from_zip(&package, &registry).expect_err("reject Art package");

    assert!(error.to_string().contains("kind must be `capability`"));
    assert!(registry.list().expect("registry list").is_empty());
    assert_staging_empty(&registry);
    cleanup(&root);
}

#[test]
fn rejects_tampered_signature_without_registry_or_immutable_target() {
    let root = temp_root("tampered");
    let key = generate_signing_key("release-1");
    write_trust_store(&root, &key, TrustPolicy::RequireTrusted);
    let registry = CapabilityPluginRegistry::new(&root);
    let package = signed_package(&root, &key, manifest("capability", "1.0.0"), b"one");
    let package = replace_zip_entry(&package, "runtime/text-tools.exe", b"two");

    let error = install_capability_from_zip(&package, &registry).expect_err("tamper rejected");

    assert!(error.to_string().contains("digest"));
    assert!(registry.list().expect("registry list").is_empty());
    assert!(!registry
        .packages_root()
        .join("publisher.example/text-tools/versions")
        .exists());
    assert_staging_empty(&registry);
    cleanup(&root);
}

#[test]
fn lifecycle_upgrade_rollback_disable_and_uninstall_are_registry_driven() {
    let root = temp_root("lifecycle");
    let key = generate_signing_key("release-1");
    write_trust_store(&root, &key, TrustPolicy::RequireTrusted);
    let registry = CapabilityPluginRegistry::new(&root);
    let grants = CapabilityGrantStore::new(&root);
    let config = CapabilityConfigStore::new(&root);
    let v1 = signed_package(&root, &key, manifest("capability", "1.0.0"), b"one");
    let v1 = install_capability_from_zip(&v1, &registry).expect("install v1");
    let active = registry
        .enable(&grants, &v1.qualified_id, Some(&v1.digest))
        .expect("enable v1");
    assert_eq!(active.active_digest.as_deref(), Some(v1.digest.as_str()));

    let v2 = signed_package(&root, &key, manifest("capability", "2.0.0"), b"two");
    let v2 = install_capability_from_zip(&v2, &registry).expect("install v2");
    let upgraded = registry
        .upgrade(&grants, &v2.qualified_id, &v2.digest)
        .expect("upgrade");
    assert_eq!(upgraded.active_digest.as_deref(), Some(v2.digest.as_str()));
    assert_eq!(
        upgraded.previous_digest.as_deref(),
        Some(v1.digest.as_str())
    );

    let rolled_back = registry
        .rollback(&grants, &v1.qualified_id)
        .expect("rollback");
    assert_eq!(
        rolled_back.active_digest.as_deref(),
        Some(v1.digest.as_str())
    );
    let disabled = registry.disable(&v1.qualified_id).expect("disable");
    assert_eq!(
        disabled.status,
        CapabilityLifecycleStatus::InstalledDisabled
    );
    assert!(!disabled.enabled_intent);
    assert!(disabled.active_digest.is_none());

    registry
        .uninstall(&grants, &config, &v1.qualified_id)
        .expect("uninstall");
    assert!(registry.get(&v1.qualified_id).expect("get").is_none());
    assert!(!registry
        .packages_root()
        .join("publisher.example/text-tools")
        .exists());
    cleanup(&root);
}

#[test]
fn permission_expansion_requires_digest_bound_exact_approval() {
    let root = temp_root("permissions");
    let key = generate_signing_key("release-1");
    write_trust_store(&root, &key, TrustPolicy::RequireTrusted);
    let registry = CapabilityPluginRegistry::new(&root);
    let grants = CapabilityGrantStore::new(&root);
    let permission = "hook.clipboard.write".to_owned();
    let mut requested = manifest("capability", "1.0.0");
    requested["permissions"] = json!([permission]);
    let package = signed_package(&root, &key, requested, b"runtime");
    let installed = install_capability_from_zip(&package, &registry).expect("install");

    let error = registry
        .enable(&grants, &installed.qualified_id, Some(&installed.digest))
        .expect_err("approval required");
    assert!(matches!(
        error,
        CapabilityInstallError::PermissionRequired(_)
    ));
    assert_eq!(
        registry
            .get(&installed.qualified_id)
            .expect("get")
            .expect("record")
            .status,
        CapabilityLifecycleStatus::ApprovalRequired
    );
    assert!(registry
        .approve_permissions(
            &grants,
            &installed.qualified_id,
            &installed.digest,
            &["hook.notice.show".to_owned()]
        )
        .is_err());
    registry
        .approve_permissions(
            &grants,
            &installed.qualified_id,
            &installed.digest,
            &["hook.clipboard.write".to_owned()],
        )
        .expect("approve exact request");
    assert_eq!(
        registry
            .enable(&grants, &installed.qualified_id, Some(&installed.digest))
            .expect("enable")
            .status,
        CapabilityLifecycleStatus::Active
    );
    cleanup(&root);
}

#[test]
fn config_store_is_revisioned_bounded_and_rejects_secret_keys() {
    let root = temp_root("config");
    let store = CapabilityConfigStore::new(&root);
    let mut values = serde_json::Map::new();
    values.insert("language".to_owned(), json!("zh-CN"));
    let saved = store
        .write("publisher.example/text-tools", 0, values)
        .expect("write config");
    assert_eq!(saved.revision, 1);
    assert!(store
        .write("publisher.example/text-tools", 0, serde_json::Map::new())
        .is_err());
    let mut secret = serde_json::Map::new();
    secret.insert("apiToken".to_owned(), json!("do-not-store"));
    assert!(store
        .write("publisher.example/text-tools", 1, secret)
        .is_err());
    cleanup(&root);
}

fn manifest(kind: &str, version: &str) -> Value {
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

fn signed_package(
    root: &Path,
    key: &loom_plugin_security::SigningKeyDocument,
    manifest: Value,
    payload: &[u8],
) -> Vec<u8> {
    let package = root.join("fixture-package");
    let _ = fs::remove_dir_all(&package);
    fs::create_dir_all(package.join("runtime")).expect("package dirs");
    fs::write(
        package.join("capability.manifest.json"),
        serde_json::to_vec_pretty(&manifest).expect("manifest JSON"),
    )
    .expect("manifest");
    fs::write(package.join("runtime/text-tools.exe"), payload).expect("runtime");
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

fn replace_zip_entry(bytes: &[u8], target: &str, replacement: &[u8]) -> Vec<u8> {
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

fn write_trust_store(
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

fn assert_staging_empty(registry: &CapabilityPluginRegistry) {
    let staging = registry.packages_root().join(".staging");
    assert_eq!(fs::read_dir(staging).expect("staging").count(), 0);
}

fn temp_root(name: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "loom-capability-{name}-{}-{nonce}",
        std::process::id()
    ))
}

fn cleanup(root: &Path) {
    let _ = crate::install::fs_safety::set_tree_readonly(root, false);
    let _ = fs::remove_dir_all(root);
}
