#[test]
fn active_canvas_cache_shares_documents_and_invalidates_all_runtime_overlays() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    clear_hook_canvas_runtime_state(None);
    let source = unique_temp_dir("active-canvas-cache").join("session.json");
    let root = json!({"stickers": [{"id": "a", "type": "sticker"}], "links": []});
    store_hook_live_workflow_snapshot(&source, HOOK_LIVE_WORKFLOW_ID, &root).unwrap();
    let first = load_active_hook_canvas_document().unwrap();
    assert!(Arc::ptr_eq(
        &first,
        &load_active_hook_canvas_document().unwrap()
    ));
    {
        let mut statuses = hook_canvas_runtime_statuses().lock().unwrap();
        statuses.insert(
            "a".to_owned(),
            HookCanvasRuntimeNodeState {
                status: "processing".to_owned(),
                ..Default::default()
            },
        );
    }
    let processing = load_active_hook_canvas_document().unwrap();
    assert_eq!(processing.snapshot.nodes[0].status, "processing");
    assert!(Arc::ptr_eq(
        &processing,
        &load_active_hook_canvas_document().unwrap()
    ));
    {
        let mut statuses = hook_canvas_runtime_statuses().lock().unwrap();
        let status = statuses.get_mut("a").unwrap();
        status.status = "ready".to_owned();
        status.preview_data_url = Some("data:image/png;base64,aGVsbG8=".to_owned());
        status.preview_cache_token = Some("new-image".to_owned());
    }
    let complete = load_active_hook_canvas_document().unwrap();
    assert_eq!(complete.snapshot.nodes[0].status, "ready");
    assert!(complete.snapshot.nodes[0].preview_available);
    assert_ne!(complete.snapshot.revision, processing.snapshot.revision);
    hook_canvas_runtime_statuses().lock().unwrap().clear();
    let cleared = load_active_hook_canvas_document().unwrap();
    assert!(!cleared.snapshot.nodes[0].preview_available);
    assert!(Arc::ptr_eq(&first, &cleared));
    clear_hook_canvas_runtime_state(None);
}
