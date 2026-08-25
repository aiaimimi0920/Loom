// Loom daemon tests fragment 15; included into the shared crate test module.
#[test]
fn daemon_exposes_hook_canvas_snapshot_contract() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    clear_hook_canvas_runtime_state(None);
    let appdata = unique_temp_dir("hook-canvas-appdata");
    let session_dir = appdata.join("com.yamiyu.hook");
    let images = session_dir.join("images");
    fs::create_dir_all(&images).expect("create session images");
    fs::write(
            session_dir.join("session.json"),
            r#"{"stickers":[{"id":"capture node","type":"sticker","src":"images/capture.png","x":20,"y":30,"w":320,"h":180}],"links":[]}"#,
        )
        .expect("write Hook session");
    fs::write(images.join("capture.png"), test_png_bytes()).expect("write preview");
    let previous = std::env::var("APPDATA").ok();
    std::env::set_var("APPDATA", &appdata);

    let (status, body) = hook_canvas_snapshot().expect("hook canvas snapshot");
    assert_eq!(status, 200);
    let canvas = serde_json::from_str::<serde_json::Value>(&body).expect("snapshot json");
    assert_eq!(canvas["available"], true);
    assert_eq!(canvas["nodes"][0]["id"], "capture node");
    assert_eq!(canvas["nodes"][0]["kind"], "screenshot");
    let preview_url = canvas["nodes"][0]["previewUrl"]
        .as_str()
        .expect("preview url string");
    assert!(
        preview_url.starts_with("/v1/hook-bridge/canvas/nodes/capture%20node/preview?v="),
        "unexpected preview url: {preview_url}"
    );
    assert!(daemon_help_text().contains("GET  /v1/hook-bridge/canvas"));
    assert!(daemon_help_text().contains("GET  /v1/hook-bridge/canvas/nodes/{nodeId}/preview"));
    restore_env("APPDATA", previous);
    clear_hook_canvas_runtime_state(None);
    fs::remove_dir_all(appdata).expect("cleanup");
}

#[test]
fn daemon_hook_canvas_prefers_live_hook_workflow_snapshot_when_available() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    clear_hook_canvas_runtime_state(None);
    let root = unique_temp_dir("hook-canvas-live-workflow");
    let appdata = unique_temp_dir("hook-canvas-live-workflow-appdata");
    let previous_appdata = std::env::var("APPDATA").ok();
    std::env::set_var("APPDATA", &appdata);
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

    let workflow = run_hook_bridge_text(
            &runtime,
            &json!({
                "method": loom_protocol::HOOK_METHOD_WORKFLOW_SYNC,
                "params": {
                    "requestId": "sync-live-workflow",
                    "workflowId": HOOK_LIVE_WORKFLOW_ID,
                    "snapshot": {
                        "name": "Hook Live",
                        "nodes": [
                            {
                                "id": "capture",
                                "type": "sticker",
                                "position": { "x": 20, "y": 30 },
                                "measured": { "width": 80, "height": 80 },
                                "data": {
                                    "src": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII=",
                                    "w": 80,
                                    "h": 80
                                }
                            },
                            {
                                "id": "missing-art-node",
                                "type": "artNode",
                                "position": { "x": 160, "y": 40 },
                                "measured": { "width": 90, "height": 90 },
                                "data": {
                                    "artId": "neuro.official/missing-art",
                                    "previewSrc": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII=",
                                    "params": { "strength": 61 },
                                    "w": 90,
                                    "h": 90
                                }
                            }
                        ],
                        "edges": [
                            {
                                "id": "edge-1",
                                "source": "capture",
                                "target": "missing-art-node",
                                "sourceHandle": "output",
                                "targetHandle": "input"
                            }
                        ]
                    }
                }
            })
            .to_string(),
        );
    assert_eq!(workflow["status"], "succeeded", "response={workflow}");

    let (status, body) = hook_canvas_snapshot().expect("hook canvas snapshot");
    assert_eq!(status, 200);
    let canvas = serde_json::from_str::<serde_json::Value>(&body).expect("snapshot json");
    assert_eq!(canvas["available"], true);
    assert_eq!(canvas["workflowId"], HOOK_LIVE_WORKFLOW_ID);
    assert_eq!(canvas["nodes"].as_array().expect("nodes").len(), 2);
    assert_eq!(canvas["nodes"][1]["id"], "missing-art-node");
    assert_eq!(canvas["nodes"][1]["kind"], "art");
    assert_eq!(canvas["nodes"][1]["status"], "ready");
    assert_eq!(canvas["nodes"][1]["previewAvailable"], true);

    restore_env("APPDATA", previous_appdata);
    clear_hook_canvas_runtime_state(None);
    drop(runtime);
    fs::remove_dir_all(root).expect("cleanup root");
    fs::remove_dir_all(appdata).expect("cleanup appdata");
}

#[test]
fn hook_canvas_patch_migrates_legacy_session_and_persists_revision_across_reload() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    clear_hook_canvas_runtime_state(None);
    let root = unique_temp_dir("hook-session-revision-migration");
    let session_path = root.join("session.json");
    fs::write(
        &session_path,
        r#"{"stickers":[{"id":"node-1","params":{"strength":1}}],"links":[]}"#,
    )
    .expect("write legacy Hook session");
    store_hook_live_workflow_snapshot(
        &session_path,
        HOOK_LIVE_WORKFLOW_ID,
        &json!({
            "nodes": [{"id":"node-1","data":{"params":{"strength":1}}}],
            "edges": []
        }),
    )
    .expect("store legacy live snapshot");
    let patch = HookCanvasPersistPatch {
        param_updates: vec![("strength".to_owned(), json!(9))],
    };

    let revision = persist_hook_canvas_live_node_patch("node-1", &patch)
        .expect("persist Loom patch against legacy revision");
    clear_hook_canvas_runtime_state(None);
    let reloaded: Value =
        serde_json::from_slice(&fs::read(&session_path).expect("reload persisted Hook session"))
            .expect("parse persisted Hook session");

    assert_eq!(revision, 1);
    assert_eq!(reloaded["documentSchemaVersion"], 1);
    assert_eq!(reloaded["documentRevision"], 1);
    assert_eq!(reloaded["stickers"][0]["params"]["strength"], 9);
    fs::remove_dir_all(root).expect("cleanup Hook session migration test");
}

#[test]
fn hook_canvas_patch_rejects_concurrent_hook_revision_and_preserves_hook_edit() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    clear_hook_canvas_runtime_state(None);
    let root = unique_temp_dir("hook-session-concurrent-edit");
    let session_path = root.join("session.json");
    fs::write(
        &session_path,
        serde_json::to_vec(&json!({
            "documentSchemaVersion": 1,
            "documentRevision": 1,
            "stickers": [{"id":"node-1","params":{"strength":1}}],
            "links": []
        }))
        .unwrap(),
    )
    .expect("write initial Hook session");
    store_hook_live_workflow_snapshot(
        &session_path,
        HOOK_LIVE_WORKFLOW_ID,
        &json!({
            "documentSchemaVersion": 1,
            "documentRevision": 1,
            "nodes": [{"id":"node-1","data":{"params":{"strength":1}}}],
            "edges": []
        }),
    )
    .expect("store revision-one live snapshot");
    let hook_lease = HookSessionFileLease::acquire(&session_path).expect("Hook writer lease");
    let loom_patch = thread::spawn(|| {
        persist_hook_canvas_live_node_patch(
            "node-1",
            &HookCanvasPersistPatch {
                param_updates: vec![("strength".to_owned(), json!(9))],
            },
        )
    });
    thread::sleep(Duration::from_millis(75));
    let hook_edit = json!({
        "documentSchemaVersion": 1,
        "documentRevision": 2,
        "stickers": [{"id":"node-1","params":{"strength":4}}],
        "links": []
    });
    write_hook_canvas_root(&session_path, &hook_edit).expect("commit concurrent Hook edit");
    drop(hook_lease);

    let error = loom_patch
        .join()
        .expect("join Loom patch writer")
        .expect_err("stale Loom patch must fail");
    let preserved: Value = serde_json::from_slice(&fs::read(&session_path).unwrap()).unwrap();

    assert!(
        matches!(
            &error,
            HookCanvasPersistError::RevisionConflict {
                expected: 1,
                current: 2
            }
        ),
        "unexpected concurrent patch result: {error:?}"
    );
    assert_eq!(preserved["documentRevision"], 2);
    assert_eq!(preserved["stickers"][0]["params"]["strength"], 4);
    clear_hook_canvas_runtime_state(None);
    fs::remove_dir_all(root).expect("cleanup concurrent Hook edit test");
}

#[test]
fn hook_canvas_patch_rejects_future_session_schema_without_overwrite() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    clear_hook_canvas_runtime_state(None);
    let root = unique_temp_dir("hook-session-future-schema");
    let session_path = root.join("session.json");
    let future = serde_json::to_vec(&json!({
        "documentSchemaVersion": 2,
        "documentRevision": 5,
        "stickers": [{"id":"node-1","params":{"strength":4}}],
        "links": []
    }))
    .unwrap();
    fs::write(&session_path, &future).expect("write future Hook session");
    store_hook_live_workflow_snapshot(
        &session_path,
        HOOK_LIVE_WORKFLOW_ID,
        &json!({
            "documentSchemaVersion": 1,
            "documentRevision": 5,
            "nodes": [{"id":"node-1","data":{"params":{"strength":4}}}],
            "edges": []
        }),
    )
    .expect("store supported live snapshot");

    let error = persist_hook_canvas_live_node_patch(
        "node-1",
        &HookCanvasPersistPatch {
            param_updates: vec![("strength".to_owned(), json!(9))],
        },
    )
    .expect_err("future Hook schema must fail closed");

    assert!(matches!(
        error,
        HookCanvasPersistError::UnsupportedDocument(_)
    ));
    assert_eq!(fs::read(&session_path).unwrap(), future);
    clear_hook_canvas_runtime_state(None);
    fs::remove_dir_all(root).expect("cleanup future Hook schema test");
}

#[test]
fn daemon_can_save_a_hook_canvas_component_directly_as_a_workflow() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    clear_hook_canvas_runtime_state(None);
    let previous_appdata = std::env::var("APPDATA").ok();
    let root = unique_temp_dir("hook-canvas-save-workflow");
    let appdata = unique_temp_dir("hook-canvas-save-workflow-appdata");
    let session_dir = appdata.join("com.yamiyu.hook");
    let images = session_dir.join("images");
    fs::create_dir_all(&images).expect("create session images");
    fs::write(
            session_dir.join("session.json"),
            r#"{
              "stickers": [
                {"id":"a","type":"sticker","src":"images/a.png","x":0,"y":0,"w":80,"h":80},
                {"id":"b","type":"art","artId":"neuro.official/resize","src":"images/b.png","x":200,"y":0,"w":80,"h":80},
                {"id":"c","type":"art","artId":"neuro.official/resize","src":"images/c.png","x":400,"y":0,"w":80,"h":80},
                {"id":"lonely","type":"sticker","src":"images/lonely.png","x":0,"y":200,"w":80,"h":80}
              ],
              "links": [
                {"id":"e1","fromUnitId":"a","toUnitId":"b"},
                {"id":"e2","fromUnitId":"b","toUnitId":"c"}
              ]
            }"#,
        )
        .expect("write Hook session");
    for name in ["a.png", "b.png", "c.png", "lonely.png"] {
        fs::write(images.join(name), test_png_bytes()).expect("write preview");
    }
    std::env::set_var("APPDATA", &appdata);

    let workflow_store = WorkflowStore::new(root.join("workflows"));
    let canvas_workflow_root = root.join("canvas-workflows");
    let (status, body) = save_hook_canvas_workflow(
        "hook-export",
        r#"{"selectedNodeId":"a","workflowName":"Hook Export"}"#,
        &workflow_store,
        &canvas_workflow_root,
    )
    .expect("save Hook canvas workflow");
    assert_eq!(status, 200);
    let saved = serde_json::from_str::<serde_json::Value>(&body).expect("saved workflow json");
    assert_eq!(saved["workflow"]["id"], "hook-export");
    assert_eq!(saved["workflow"]["name"], "Hook Export");

    let loaded = workflow_store
        .load_workflow("hook-export")
        .expect("load saved workflow");
    let data = loaded;
    assert!(data.contains("name: 'Hook Export'"));
    assert!(data.contains("- id: a"));
    assert!(data.contains("- id: resize"));
    assert!(data.contains("- id: resize-2"));
    assert!(data.contains("needs: [a]"));
    assert!(data.contains("needs: [resize]"));
    assert!(!data.contains("lonely"));

    // Renaming updates the display name in meta.json without changing the id.
    let canvas_root = root.join("canvas-workflows");
    let (rename_status, rename_body) =
        rename_canvas_workflow("hook-export", r#"{"name":"Renamed Flow"}"#, &canvas_root)
            .expect("rename canvas workflow");
    assert_eq!(rename_status, 200);
    let renamed = serde_json::from_str::<serde_json::Value>(&rename_body).expect("rename json");
    assert_eq!(renamed["id"], "hook-export");
    assert_eq!(renamed["name"], "Renamed Flow");
    let (_, list_body) = list_canvas_workflows(&canvas_root).expect("list canvas workflows");
    let list_json = serde_json::from_str::<serde_json::Value>(&list_body).expect("list json");
    assert!(list_json["workflows"]
        .as_array()
        .expect("workflows array")
        .iter()
        .any(|w| w["id"] == "hook-export" && w["name"] == "Renamed Flow"));

    // Deleting removes the frozen snapshot directory.
    let (delete_status, _) =
        delete_canvas_workflow("hook-export", &canvas_root).expect("delete canvas workflow");
    assert_eq!(delete_status, 200);
    let (_, after_body) = list_canvas_workflows(&canvas_root).expect("list after delete");
    let after_json = serde_json::from_str::<serde_json::Value>(&after_body).expect("after json");
    assert!(after_json["workflows"]
        .as_array()
        .expect("workflows array")
        .iter()
        .all(|w| w["id"] != "hook-export"));

    restore_env("APPDATA", previous_appdata);
    clear_hook_canvas_runtime_state(None);
    fs::remove_dir_all(root).expect("cleanup root");
    fs::remove_dir_all(appdata).expect("cleanup appdata");
}

#[test]
fn daemon_serves_only_registered_hook_canvas_preview_images() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let appdata = unique_temp_dir("hook-canvas-preview-appdata");
    let session_dir = appdata.join("com.yamiyu.hook");
    let images = session_dir.join("images");
    fs::create_dir_all(&images).expect("create session images");
    let png = test_png_bytes();
    fs::write(images.join("capture.png"), &png).expect("write registered preview");
    fs::write(appdata.join("outside.png"), &png).expect("write outside preview");
    fs::write(
            session_dir.join("session.json"),
            r#"{
              "stickers": [
                {"id":"capture node","type":"sticker","src":"images/capture.png","x":20,"y":30,"w":320,"h":180},
                {"id":"escape","type":"sticker","src":"../outside.png","x":400,"y":30,"w":320,"h":180}
              ],
              "links": []
            }"#,
        )
        .expect("write Hook session");
    let previous = std::env::var("APPDATA").ok();
    std::env::set_var("APPDATA", &appdata);

    let registered = hook_canvas_preview_response("capture node").expect("registered preview");
    let registered_body = expect_binary_route_response(registered, 200, "image/png");
    assert_eq!(registered_body, png);

    for node_id in ["unknown", "escape"] {
        let response = hook_canvas_preview_response(node_id).expect("preview not found response");
        let body = expect_text_route_response(response, 404);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&body).expect("preview error json")["error"]
                ["code"],
            "preview_not_found"
        );
    }

    restore_env("APPDATA", previous);
    fs::remove_dir_all(appdata).expect("cleanup");
}
