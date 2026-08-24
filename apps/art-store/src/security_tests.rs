// Regression tests for archive budgets, path containment and cross-process persistence.
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::archive::{MAX_ARCHIVE_ENTRIES, MAX_MANIFEST_BYTES};
#[cfg(unix)]
use crate::{read_binary, BINARIES_DIR};
use crate::{
    read_manifest_bytes, store_published_zip, StoreError, ARTS_DIR, MAX_PUBLISHED_ZIP_BYTES,
};

struct TestRoot(PathBuf);

impl TestRoot {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "loom-art-store-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("create test root");
        Self(root)
    }
}

impl std::ops::Deref for TestRoot {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn package_zip(id: &str, version: &str) -> Vec<u8> {
    let manifest = serde_json::json!({
        "id": id,
        "name": id,
        "description": version,
        "metadata": {
            "dependencies": { "framework": "process" },
            "packageSecurity": {
                "version": version,
                "publisher": { "id": "publisher.test" }
            }
        }
    });
    zip_with_files(
        &[("manifest.json", serde_json::to_vec(&manifest).unwrap())],
        false,
    )
}

fn zip_with_files(files: &[(&str, Vec<u8>)], compressed: bool) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut bytes));
        let method = if compressed {
            zip::CompressionMethod::Deflated
        } else {
            zip::CompressionMethod::Stored
        };
        let options: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(method);
        for (name, content) in files {
            writer.start_file(*name, options).unwrap();
            writer.write_all(content).unwrap();
        }
        writer.finish().unwrap();
    }
    bytes
}

#[test]
fn archive_budgets_reject_oversized_or_suspicious_packages() {
    assert!(matches!(
        read_manifest_bytes(&vec![0; MAX_PUBLISHED_ZIP_BYTES as usize + 1]),
        Err(StoreError::PackageTooLarge(_))
    ));

    let oversized_manifest = zip_with_files(
        &[("manifest.json", vec![b'x'; MAX_MANIFEST_BYTES as usize + 1])],
        false,
    );
    assert!(matches!(
        read_manifest_bytes(&oversized_manifest),
        Err(StoreError::ArchiveEntryTooLarge { .. })
    ));

    let compressed = zip_with_files(
        &[
            ("manifest.json", b"{}".to_vec()),
            ("payload.bin", vec![0; 2 * 1024 * 1024]),
        ],
        true,
    );
    assert!(matches!(
        read_manifest_bytes(&compressed),
        Err(StoreError::ArchiveCompressionRatio(_))
    ));

    let mut bytes = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut bytes));
        let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        for index in 0..=MAX_ARCHIVE_ENTRIES {
            writer.start_file(format!("{index}.txt"), options).unwrap();
        }
        writer.finish().unwrap();
    }
    assert!(matches!(
        read_manifest_bytes(&bytes),
        Err(StoreError::ArchiveEntryCount)
    ));
}

#[test]
fn archive_metadata_rejects_unsafe_names_and_symbolic_links() {
    for name in ["../manifest.json", "folder\\manifest.json"] {
        let bytes = zip_with_files(&[(name, b"{}".to_vec())], false);
        assert!(matches!(
            read_manifest_bytes(&bytes),
            Err(StoreError::InvalidResourceName(_))
        ));
    }

    let mut bytes = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut bytes));
        let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        writer.start_file("manifest.json", options).unwrap();
        writer.write_all(b"{}").unwrap();
        writer
            .add_symlink("payload-link", "payload.bin", options)
            .unwrap();
        writer.finish().unwrap();
    }
    assert!(matches!(
        read_manifest_bytes(&bytes),
        Err(StoreError::ArchiveSymbolicLink(name)) if name == "payload-link"
    ));
}

#[test]
fn concurrent_publications_keep_unique_ids_and_valid_index_json() {
    let root = TestRoot::new("concurrent-publish");
    let first_root = root.0.clone();
    let second_root = root.0.clone();
    let first = package_zip("parallel-a", "1.0.0");
    let second = package_zip("parallel-b", "1.0.0");
    let first = std::thread::spawn(move || {
        store_published_zip(&first_root, Some("parallel-a"), &first).unwrap()
    });
    let second = std::thread::spawn(move || {
        store_published_zip(&second_root, Some("parallel-b"), &second).unwrap()
    });
    let first = first.join().unwrap();
    let second = second.join().unwrap();
    assert_ne!(first.global_id, second.global_id);
    let index = std::fs::read(root.join(crate::GLOBAL_ART_IDS_FILE)).unwrap();
    let index: serde_json::Value = serde_json::from_slice(&index).unwrap();
    assert_eq!(index["assignments"].as_object().unwrap().len(), 2);
    assert!(root.join(ARTS_DIR).join("parallel-a/1.0.0.zip").is_file());
    assert!(root.join(ARTS_DIR).join("parallel-b/1.0.0.zip").is_file());
}

#[test]
fn concurrent_identical_publications_are_idempotent() {
    let root = TestRoot::new("concurrent-idempotent-publish");
    let first_root = root.0.clone();
    let second_root = root.0.clone();
    let package = package_zip("parallel-same", "1.0.0");
    let first_package = package.clone();
    let second_package = package.clone();
    let first = std::thread::spawn(move || {
        store_published_zip(&first_root, Some("parallel-same"), &first_package).unwrap()
    });
    let second = std::thread::spawn(move || {
        store_published_zip(&second_root, Some("parallel-same"), &second_package).unwrap()
    });
    let first = first.join().unwrap();
    let second = second.join().unwrap();
    assert_eq!(first.global_id, second.global_id);
    assert_eq!(
        std::fs::read(root.join(ARTS_DIR).join("parallel-same/1.0.0.zip")).unwrap(),
        package
    );
    let index = std::fs::read(root.join(crate::GLOBAL_ART_IDS_FILE)).unwrap();
    let index: serde_json::Value = serde_json::from_slice(&index).unwrap();
    assert_eq!(index["assignments"].as_object().unwrap().len(), 1);
}

#[cfg(unix)]
#[test]
fn binary_reads_reject_symlink_escape() {
    use std::os::unix::fs::symlink;

    let root = TestRoot::new("symlink-read");
    let outside = TestRoot::new("symlink-target");
    std::fs::create_dir_all(root.join(BINARIES_DIR)).unwrap();
    std::fs::write(outside.join("secret.bin"), b"outside").unwrap();
    symlink(
        outside.join("secret.bin"),
        root.join(BINARIES_DIR).join("linked.bin"),
    )
    .unwrap();
    assert!(matches!(
        read_binary(&root, "linked.bin"),
        Err(StoreError::UnsafeStoredPath)
    ));
}
