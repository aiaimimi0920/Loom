#[test]
fn unchanged_canvas_reads_share_one_normalized_document() {
    let root = test_root("cache-shared");
    let session = write_session(
        &root,
        r#"{"stickers":[{"id":"a","src":"images/a.png"}],"links":[]}"#,
    );
    let mut cache = HookCanvasDocumentCache::default();
    let first = cache.read(&session).unwrap();
    for _ in 0..100 {
        assert!(std::sync::Arc::ptr_eq(
            &first,
            &cache.read(&session).unwrap()
        ));
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cache_tracks_new_missing_and_in_place_updated_preview_candidates() {
    let root = test_root("cache-images");
    let session = write_session(
        &root,
        r#"{"stickers":[{"id":"a","src":"images/a.png"}],"links":[]}"#,
    );
    let image = session.parent().unwrap().join("images/a.png");
    let mut cache = HookCanvasDocumentCache::default();
    let missing = cache.read(&session).unwrap();
    assert!(!missing.snapshot.nodes[0].preview_available);
    fs::write(&image, b"first").unwrap();
    let first = cache.read(&session).unwrap();
    assert!(first.snapshot.nodes[0].preview_available);
    fs::write(&image, b"replacement").unwrap();
    let changed = cache.read(&session).unwrap();
    assert_ne!(first.snapshot.revision, changed.snapshot.revision);
    fs::remove_file(&image).unwrap();
    assert!(!cache.read(&session).unwrap().snapshot.nodes[0].preview_available);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cache_checks_session_bytes_even_when_size_and_modified_time_are_preserved() {
    let root = test_root("cache-byte-identity");
    let session = write_session(&root, r#"{"stickers":[{"id":"a"}],"links":[]}"#);
    let time = fs::metadata(&session).unwrap().modified().unwrap();
    let mut cache = HookCanvasDocumentCache::default();
    let first = cache.read(&session).unwrap();
    fs::write(&session, r#"{"stickers":[{"id":"b"}],"links":[]}"#).unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(&session)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(time))
        .unwrap();
    let second = cache.read(&session).unwrap();
    assert_eq!(second.snapshot.nodes[0].id, "b");
    assert!(!std::sync::Arc::ptr_eq(&first, &second));
    fs::write(&session, b"invalid").unwrap();
    assert!(cache.read(&session).is_err());
    fs::remove_file(&session).unwrap();
    assert!(!cache.read(&session).unwrap().snapshot.available);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn changing_source_directory_cannot_reuse_another_canvas_document() {
    let first_root = test_root("cache-first-root");
    let second_root = test_root("cache-second-root");
    let json = r#"{"stickers":[{"id":"a","src":"images/a.png"}],"links":[]}"#;
    let first_path = write_session(&first_root, json);
    let second_path = write_session(&second_root, json);
    fs::write(first_path.parent().unwrap().join("images/a.png"), b"first").unwrap();
    let mut cache = HookCanvasDocumentCache::default();
    let first = cache.read(&first_path).unwrap();
    let second = cache.read(&second_path).unwrap();
    assert!(first.snapshot.nodes[0].preview_available);
    assert!(!second.snapshot.nodes[0].preview_available);
    assert!(!std::sync::Arc::ptr_eq(
        &first,
        &cache.read(&first_path).unwrap()
    ));
    fs::remove_dir_all(first_root).unwrap();
    fs::remove_dir_all(second_root).unwrap();
}

#[cfg(unix)]
#[test]
fn a_cached_preview_symlink_cannot_retarget_outside_allowed_roots() {
    let root = test_root("cache-symlink");
    let session = write_session(
        &root,
        r#"{"stickers":[{"id":"a","src":"images/link.png"}],"links":[]}"#,
    );
    let images = session.parent().unwrap().join("images");
    fs::write(images.join("safe.png"), b"safe").unwrap();
    fs::write(root.join("outside.png"), b"outside").unwrap();
    std::os::unix::fs::symlink(images.join("safe.png"), images.join("link.png")).unwrap();
    let mut cache = HookCanvasDocumentCache::default();
    assert!(cache.read(&session).unwrap().snapshot.nodes[0].preview_available);
    fs::remove_file(images.join("link.png")).unwrap();
    std::os::unix::fs::symlink(root.join("outside.png"), images.join("link.png")).unwrap();
    assert!(!cache.read(&session).unwrap().snapshot.nodes[0].preview_available);
    fs::remove_dir_all(root).unwrap();
}
