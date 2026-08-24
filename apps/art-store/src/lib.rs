//! Local Loom Art Store core with transport-independent catalog and publish APIs.
//!
//! Public names remain re-exported from this facade so the server and daemon clients do not
//! depend on the internal persistence, validation, package or publisher ownership boundaries.

mod archive;
mod catalog;
mod filesystem;
mod indexes;
mod manifest;
mod model;
mod persistence;
mod publish;
mod publisher;
mod signature;
mod storage;
mod validation;

pub use archive::MAX_PUBLISHED_ZIP_BYTES;
pub use catalog::build_catalog;
pub use indexes::{GLOBAL_ART_IDS_FILE, OFFICIAL_ART_CERTIFICATIONS_FILE};
pub use manifest::{catalog_entry_from_manifest, read_manifest_bytes};
pub use model::{
    CatalogEntry, CatalogVersion, PublishedArt, PublisherDirectoryEntry, PublisherKeyStatus,
    PublisherPublicKey, PublisherRotationRequest, StoreError,
};
pub use publish::{store_published_zip, store_verified_published_zip};
pub use publisher::{
    is_platform_publisher_id, publisher_rotation_message, read_publisher, register_publisher,
    register_publisher_with_id, rotate_publisher_key, PUBLISHER_DIRECTORY_FILE,
};
pub use storage::{
    art_version_zip_path, binary_path, framework_package_path, read_art_zip_version,
    read_art_zip_version_sha256, read_binary, read_framework_package, ARTS_DIR, BINARIES_DIR,
    FRAMEWORKS_DIR,
};
pub use validation::{is_safe_art_id, is_safe_resource_name};

#[cfg(test)]
use manifest::MANIFEST_NAME;
#[cfg(test)]
use sha2::{Digest, Sha256};
#[cfg(test)]
use std::path::{Path, PathBuf};
#[cfg(test)]
use storage::art_zip_sha256_sidecar;

#[cfg(test)]
mod security_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_root() -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "loom-art-store-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&base).unwrap();
        base
    }

    fn build_zip(manifest: &str) -> Vec<u8> {
        let mut manifest: serde_json::Value = serde_json::from_str(manifest).unwrap();
        let execution_framework = manifest
            .get("execution")
            .and_then(|execution| execution.get("framework"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("process")
            .to_owned();
        let metadata = manifest
            .as_object_mut()
            .unwrap()
            .entry("metadata")
            .or_insert_with(|| serde_json::json!({}));
        let metadata = metadata.as_object_mut().unwrap();
        metadata
            .entry("dependencies")
            .or_insert_with(|| serde_json::json!({ "framework": execution_framework }));
        let security = metadata
            .entry("packageSecurity")
            .or_insert_with(|| serde_json::json!({}));
        security
            .as_object_mut()
            .unwrap()
            .entry("publisher")
            .or_insert_with(|| serde_json::json!({ "id": "publisher.test" }));
        let mut buf = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
            writer.start_file(MANIFEST_NAME, opts).unwrap();
            writer
                .write_all(serde_json::to_string(&manifest).unwrap().as_bytes())
                .unwrap();
            writer.finish().unwrap();
        }
        buf
    }

    fn build_signed_zip(
        root: &Path,
        art_id: &str,
        version: &str,
        user_id: &str,
        key: &loom_plugin_security::SigningKeyDocument,
    ) -> Vec<u8> {
        let package = root.join(format!("package-{version}"));
        std::fs::create_dir_all(&package).unwrap();
        let manifest = serde_json::json!({
            "id": art_id,
            "name": "Signed",
            "description": version,
            "execution": { "type": "framework_art", "framework": "process" },
            "metadata": {
                "dependencies": { "framework": "process" },
                "packageSecurity": {
                    "version": version,
                    "publisher": { "id": user_id, "keyId": key.key_id },
                    "signature": {
                        "algorithm": "ed25519",
                        "keyId": key.key_id,
                        "file": "signature.json"
                    }
                }
            }
        });
        std::fs::write(
            package.join(MANIFEST_NAME),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        std::fs::write(package.join("payload.txt"), version.as_bytes()).unwrap();
        loom_plugin_security::sign_package(&package, "signature.json", key).unwrap();
        let mut bytes = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut bytes));
            let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
            for name in [MANIFEST_NAME, "payload.txt", "signature.json"] {
                writer.start_file(name, options).unwrap();
                writer
                    .write_all(&std::fs::read(package.join(name)).unwrap())
                    .unwrap();
            }
            writer.finish().unwrap();
        }
        bytes
    }

    fn write_official_certification(root: &Path, qualified_id: &str, version: &str, digest: &str) {
        let document = serde_json::json!({
            "schemaVersion": 1,
            "certifications": {
                (qualified_id): {
                    (version): digest,
                }
            }
        });
        std::fs::write(
            root.join(OFFICIAL_ART_CERTIFICATIONS_FILE),
            serde_json::to_vec_pretty(&document).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn safe_art_id_rejects_traversal_and_separators() {
        assert!(is_safe_art_id("pingo-art"));
        assert!(!is_safe_art_id("../evil"));
        assert!(!is_safe_art_id("a/b"));
        assert!(!is_safe_art_id("a\\b"));
        assert!(!is_safe_art_id(""));
    }

    #[test]
    fn safe_resource_name_allows_nesting_but_rejects_escape() {
        assert!(is_safe_resource_name("pingo.exe"));
        assert!(is_safe_resource_name("bin/pingo.exe"));
        assert!(!is_safe_resource_name("../pingo.exe"));
        assert!(!is_safe_resource_name("bin/../../pingo.exe"));
        assert!(!is_safe_resource_name("/etc/passwd"));
    }

    #[test]
    fn catalog_entry_requires_declared_framework_and_publisher() {
        let manifest = r#"{"id":"wf","name":"WF","description":"d",
            "execution":{"type":"workflow","workflowId":"x"},
            "metadata":{"packageSecurity":{"version":"1.0.0","publisher":{"id":"publisher.test"}},"dependencies":{"framework":"workflow"}}}"#;
        let entry = catalog_entry_from_manifest(manifest.as_bytes()).unwrap();
        assert_eq!(entry.id, "wf");
        assert_eq!(entry.framework, "workflow");
    }

    #[test]
    fn catalog_entry_rejects_missing_canonical_framework_dependency() {
        let manifest = r#"{"id":"p","name":"P","description":"d",
            "execution":{"type":"framework_art","framework":"process"},
            "metadata":{"packageSecurity":{"version":"1.0.0","publisher":{"id":"publisher.test"}}}}"#;
        assert!(matches!(
            catalog_entry_from_manifest(manifest.as_bytes()),
            Err(StoreError::MissingFramework(id)) if id == "p"
        ));
    }

    #[test]
    fn build_catalog_lists_published_zips_sorted() {
        let root = temp_root();
        let a = build_zip(
            r#"{"id":"b-art","name":"B","description":"d",
            "execution":{"type":"framework_art","framework":"process"},
            "metadata":{"packageSecurity":{"version":"1.0.0"}}}"#,
        );
        let b = build_zip(
            r#"{"id":"a-art","name":"A","description":"d",
            "execution":{"type":"cloud_api","endpoint":"https://x","method":"POST"},
            "metadata":{"packageSecurity":{"version":"1.0.0"}}}"#,
        );
        std::fs::create_dir_all(root.join(ARTS_DIR).join("b-art")).unwrap();
        std::fs::create_dir_all(root.join(ARTS_DIR).join("a-art")).unwrap();
        std::fs::write(root.join(ARTS_DIR).join("b-art/1.0.0.zip"), &a).unwrap();
        std::fs::write(root.join(ARTS_DIR).join("a-art/1.0.0.zip"), &b).unwrap();
        let catalog = build_catalog(&root).unwrap();
        assert_eq!(catalog.len(), 2);
        assert_eq!(catalog[0].id, "a-art");
        assert_eq!(catalog[1].id, "b-art");
        assert_eq!(catalog[1].framework, "process");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn package_manifest_cannot_self_certify_as_official() {
        let root = temp_root();
        let zip = build_zip(
            r#"{"id":"claimed-official","name":"Claimed","description":"d",
            "execution":{"type":"framework_art","framework":"process"},
            "official":true,
            "metadata":{"official":true,"packageSecurity":{"version":"1.0.0","publisher":{"id":"neuro.official","name":"Neuro"}}}}"#,
        );
        store_published_zip(&root, Some("claimed-official"), &zip).unwrap();

        let catalog = build_catalog(&root).unwrap();
        assert_eq!(catalog.len(), 1);
        assert!(!catalog[0].official);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn official_certification_requires_the_exact_latest_package_digest() {
        let root = temp_root();
        let zip = build_zip(
            r#"{"id":"certified-art","name":"Certified","description":"d",
            "execution":{"type":"framework_art","framework":"process"},
            "metadata":{"packageSecurity":{"version":"1.0.0","publisher":{"id":"neuro.official","name":"Neuro"}}}}"#,
        );
        store_published_zip(&root, Some("certified-art"), &zip).unwrap();
        let digest = format!("{:x}", Sha256::digest(&zip));
        write_official_certification(&root, "neuro.official/certified-art", "1.0.0", &digest);

        let catalog = build_catalog(&root).unwrap();
        assert_eq!(catalog.len(), 1);
        assert!(catalog[0].official);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_new_latest_version_does_not_inherit_an_older_certification() {
        let root = temp_root();
        let first = build_zip(
            r#"{"id":"version-certified","name":"Certified","description":"one",
            "execution":{"type":"framework_art","framework":"process"},
            "metadata":{"packageSecurity":{"version":"1.0.0","publisher":{"id":"neuro.official","name":"Neuro"}}}}"#,
        );
        let second = build_zip(
            r#"{"id":"version-certified","name":"Certified","description":"two",
            "execution":{"type":"framework_art","framework":"process"},
            "metadata":{"packageSecurity":{"version":"1.1.0","publisher":{"id":"neuro.official","name":"Neuro"}}}}"#,
        );
        store_published_zip(&root, Some("version-certified"), &first).unwrap();
        write_official_certification(
            &root,
            "neuro.official/version-certified",
            "1.0.0",
            &format!("{:x}", Sha256::digest(&first)),
        );
        assert!(build_catalog(&root).unwrap()[0].official);

        store_published_zip(&root, Some("version-certified"), &second).unwrap();
        let catalog = build_catalog(&root).unwrap();
        assert_eq!(catalog[0].latest_version, "1.1.0");
        assert!(!catalog[0].official);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn store_and_read_roundtrip() {
        let root = temp_root();
        let zip = build_zip(
            r#"{"id":"pingo-art","name":"Pingo","description":"d",
            "execution":{"type":"framework_art","framework":"process"},
            "metadata":{"packageSecurity":{"version":"1.0.0"}}}"#,
        );
        let published = store_published_zip(&root, Some("pingo-art"), &zip).unwrap();
        assert_eq!(published.art_id, "pingo-art");
        assert!(published.global_id.starts_with("NA"));
        assert_eq!(published.global_id.len(), 13);
        let read = read_art_zip_version(&root, "pingo-art", "1.0.0")
            .unwrap()
            .unwrap();
        assert_eq!(read, zip);
        let sidecar = read_art_zip_version_sha256(&root, "pingo-art", "1.0.0")
            .unwrap()
            .unwrap();
        assert_eq!(
            String::from_utf8(sidecar).unwrap(),
            art_zip_sha256_sidecar("pingo-art/1.0.0", &zip)
        );
        assert!(read_art_zip_version(&root, "missing", "1.0.0")
            .unwrap()
            .is_none());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn publish_assigns_one_stable_platform_global_id_per_repository() {
        let root = temp_root();
        let first = build_zip(
            r#"{"id":"stable-art","name":"Stable","description":"one",
            "execution":{"type":"framework_art","framework":"process"},
            "metadata":{"packageSecurity":{"version":"1.0.0"},"art":{"globalId":"NA00000000001"}}}"#,
        );
        let second = build_zip(
            r#"{"id":"stable-art","name":"Stable","description":"two",
            "execution":{"type":"framework_art","framework":"process"},
            "metadata":{"packageSecurity":{"version":"1.1.0"}}}"#,
        );
        let published_first = store_published_zip(&root, Some("stable-art"), &first).unwrap();
        let published_second = store_published_zip(&root, Some("stable-art"), &second).unwrap();
        assert_eq!(published_first.global_id, published_second.global_id);
        assert_ne!(published_first.global_id, "NA00000000001");
        assert!(published_first.global_id.starts_with("NA"));
        assert_eq!(published_first.global_id.len(), 13);
        let catalog = build_catalog(&root).unwrap();
        assert_eq!(
            catalog[0].global_id.as_deref(),
            Some(published_first.global_id.as_str())
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn missing_version_digest_sidecar_is_not_synthesized() {
        let root = temp_root();
        let zip = build_zip(
            r#"{"id":"canonical-art","name":"Canonical","description":"d",
            "execution":{"type":"framework_art","framework":"process"},
            "metadata":{"packageSecurity":{"version":"1.0.0"}}}"#,
        );
        let directory = root.join(ARTS_DIR).join("canonical-art");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("1.0.0.zip"), &zip).unwrap();

        assert!(read_art_zip_version_sha256(&root, "canonical-art", "1.0.0")
            .unwrap()
            .is_none());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn publish_rejects_id_mismatch() {
        let root = temp_root();
        let zip = build_zip(
            r#"{"id":"real-id","name":"R","description":"d",
            "execution":{"type":"framework_art","framework":"process"},
            "metadata":{"packageSecurity":{"version":"1.0.0"}}}"#,
        );
        let err = store_published_zip(&root, Some("claimed-id"), &zip).unwrap_err();
        assert!(matches!(err, StoreError::ArtIdMismatch { .. }));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn read_binary_validates_and_reads() {
        let root = temp_root();
        std::fs::create_dir_all(root.join(BINARIES_DIR)).unwrap();
        std::fs::write(root.join(BINARIES_DIR).join("pingo.exe"), b"MZ-fake").unwrap();
        let bytes = read_binary(&root, "pingo.exe").unwrap().unwrap();
        assert_eq!(bytes, b"MZ-fake");
        assert!(read_binary(&root, "missing.exe").unwrap().is_none());
        assert!(read_binary(&root, "../escape").is_err());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn publishing_multiple_versions_preserves_history_and_orders_semver() {
        let root = temp_root();
        for version in ["1.0.0", "1.10.0", "1.2.0"] {
            let zip = build_zip(&format!(
                r#"{{"id":"versioned-art","name":"Versioned","description":"{version}",
                "execution":{{"type":"framework_art","framework":"process"}},
                "metadata":{{"packageSecurity":{{"version":"{version}"}}}}}}"#
            ));
            store_published_zip(&root, Some("versioned-art"), &zip).unwrap();
        }
        let catalog = build_catalog(&root).unwrap();
        assert_eq!(catalog.len(), 1);
        assert_eq!(catalog[0].latest_version, "1.10.0");
        assert_eq!(catalog[0].description, "1.10.0");
        assert_eq!(
            catalog[0]
                .versions
                .iter()
                .map(|version| version.version.as_str())
                .collect::<Vec<_>>(),
            vec!["1.0.0", "1.2.0", "1.10.0"]
        );
        assert!(read_art_zip_version(&root, "versioned-art", "1.2.0")
            .unwrap()
            .is_some());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn catalog_rejects_same_version_with_different_content() {
        let root = temp_root();
        let first = build_zip(
            r#"{"id":"conflict-art","name":"First","description":"one",
            "execution":{"type":"framework_art","framework":"process"},
            "metadata":{"packageSecurity":{"version":"1.0.0"}}}"#,
        );
        let second = build_zip(
            r#"{"id":"conflict-art","name":"Second","description":"two",
            "execution":{"type":"framework_art","framework":"process"},
            "metadata":{"packageSecurity":{"version":"1.0.0"}}}"#,
        );
        store_published_zip(&root, Some("conflict-art"), &first).unwrap();
        assert!(matches!(
            store_published_zip(&root, Some("conflict-art"), &second).unwrap_err(),
            StoreError::VersionConflict { .. }
        ));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn publisher_directory_accepts_the_default_test_user_id() {
        let root = temp_root();
        let key = loom_plugin_security::generate_signing_key("default-user-key");
        let publisher =
            register_publisher_with_id(&root, Some("L0000000000"), &key.key_id, &key.public_key)
                .expect("register default test publisher");
        assert_eq!(publisher.user_id, "L0000000000");
        assert_eq!(
            read_publisher(&root, "L0000000000")
                .expect("read publisher")
                .expect("publisher exists")
                .keys[0]
                .public_key,
            key.public_key
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn publisher_directory_rotates_keys_and_verified_publish_requires_the_active_key() {
        let root = temp_root();
        let first_key = loom_plugin_security::generate_signing_key("key-1");
        let publisher = register_publisher(&root, &first_key.key_id, &first_key.public_key)
            .expect("register publisher");
        assert!(is_platform_publisher_id(&publisher.user_id));

        let first_zip =
            build_signed_zip(&root, "signed-art", "1.0.0", &publisher.user_id, &first_key);
        store_verified_published_zip(&root, Some("signed-art"), &first_zip)
            .expect("publish with active key");

        let second_key = loom_plugin_security::generate_signing_key("key-2");
        let message = publisher_rotation_message(
            &publisher.user_id,
            &first_key.key_id,
            &second_key.key_id,
            &second_key.public_key,
        );
        let rotated = rotate_publisher_key(
            &root,
            &publisher.user_id,
            &PublisherRotationRequest {
                current_key_id: first_key.key_id.clone(),
                new_key_id: second_key.key_id.clone(),
                new_public_key: second_key.public_key.clone(),
                signature: loom_plugin_security::sign_message(&first_key, message.as_bytes())
                    .expect("rotation signature"),
            },
        )
        .expect("rotate publisher key");
        assert_eq!(rotated.keys[0].status, PublisherKeyStatus::Retired);
        assert_eq!(rotated.keys[1].status, PublisherKeyStatus::Active);

        let stale_zip =
            build_signed_zip(&root, "signed-art", "2.0.0", &publisher.user_id, &first_key);
        assert!(matches!(
            store_verified_published_zip(&root, Some("signed-art"), &stale_zip),
            Err(StoreError::PublisherActiveKeyMissing(_))
        ));

        let current_zip = build_signed_zip(
            &root,
            "signed-art",
            "2.0.0",
            &publisher.user_id,
            &second_key,
        );
        store_verified_published_zip(&root, Some("signed-art"), &current_zip)
            .expect("publish with rotated key");
        assert_eq!(build_catalog(&root).unwrap()[0].versions.len(), 2);
        std::fs::remove_dir_all(&root).ok();
    }
}
