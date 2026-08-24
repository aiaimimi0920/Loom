//! Resource ceilings and concurrent lifecycle regressions added after extraction.

use super::*;

#[test]
fn file_digest_streams_content_larger_than_its_buffer() {
    let root = std::env::temp_dir().join(staging_name());
    fs::create_dir_all(&root).expect("create digest fixture root");
    let bytes = vec![0x5a; 2 * 64 * 1024 + 17];
    let path = root.join("large-entry.bin");
    fs::write(&path, &bytes).expect("write digest fixture");

    assert_eq!(
        file_digest(&path).expect("stream file digest"),
        format!("{:x}", Sha256::digest(&bytes))
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn active_state_reader_rejects_oversized_files_before_json_parsing() {
    let root = std::env::temp_dir().join(staging_name());
    fs::create_dir_all(&root).expect("create state fixture root");
    let path = root.join("active.json");
    let file = fs::File::create(&path).expect("create oversized active state");
    file.set_len(MAX_ACTIVE_STATE_BYTES as u64 + 1)
        .expect("size oversized active state");

    let error = read_active_state_file(&path).expect_err("oversized state must be rejected");

    assert!(
        matches!(error, McpPackageError::InvalidManifest(message) if message.contains("exceeds"))
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn concurrent_installs_publish_one_complete_active_state() {
    let root = std::sync::Arc::new(std::env::temp_dir().join(staging_name()));
    let bytes = std::sync::Arc::new(stdio_package_bytes());
    let workers = (0..2)
        .map(|_| {
            let root = std::sync::Arc::clone(&root);
            let bytes = std::sync::Arc::clone(&bytes);
            std::thread::spawn(move || install_server_package(&root, &bytes))
        })
        .collect::<Vec<_>>();

    let configs = workers
        .into_iter()
        .map(|worker| {
            worker
                .join()
                .expect("install thread")
                .expect("concurrent install")
        })
        .collect::<Vec<_>>();
    let active = read_active_state(&root, "publisher.test", "fixture-search")
        .expect("read complete active state");

    assert_eq!(
        configs[0].package.as_ref().expect("first state").digest,
        active.digest
    );
    assert_eq!(
        configs[1].package.as_ref().expect("second state").digest,
        active.digest
    );
    assert!(!active.files.is_empty());
    let _ = fs::remove_dir_all(root.as_path());
}
