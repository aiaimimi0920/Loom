// Loom daemon tests fragment 19; included into the shared crate test module.
#[test]
fn daemon_hook_bridge_ocr_image_unavailable_by_default() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let previous_fixture = std::env::var("LOOM_OCR_FIXTURE_TEXT").ok();
    let previous_model_dir = std::env::var("LOOM_OCR_MODEL_DIR").ok();
    std::env::remove_var("LOOM_OCR_FIXTURE_TEXT");
    let root = unique_temp_dir("ocr-unavailable");
    let empty_model_dir = root.join("empty-ocr-models");
    fs::create_dir_all(&empty_model_dir).expect("create empty model dir");
    std::env::set_var("LOOM_OCR_MODEL_DIR", &empty_model_dir);
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
    let response = run_hook_bridge_text(
        &runtime,
        &serde_json::json!({
            "method": loom_protocol::HOOK_METHOD_OCR_EXECUTE,
            "params": {
                "requestId": "unavailable-ocr",
                "imageBase64": test_png_base64()
            }
        })
        .to_string(),
    );

    assert_eq!(response["status"], "failed");
    assert_eq!(response["error"]["message"], "OCR enhancement unavailable");

    drop(runtime);
    restore_env("LOOM_OCR_MODEL_DIR", previous_model_dir);
    restore_env("LOOM_OCR_FIXTURE_TEXT", previous_fixture);
    fs::remove_dir_all(root).expect("cleanup ocr unavailable root");
}

#[test]
fn formal_shared_memory_port_materializes_as_an_image_input() {
    let store = Arc::new(Mutex::new(SharedImageStore::new()));
    let image = store
        .lock()
        .expect("shared image store")
        .create_rgba8(1, 1, vec![10, 20, 30, 255])
        .expect("create shared image");
    let inputs = BTreeMap::from([(
        "input".to_owned(),
        HookArtPortValue::SharedMemory {
            handle: image.handle,
            size: 4,
            width: 1,
            height: 1,
            format: "rgba8".to_owned(),
        },
    )]);

    let materialized =
        materialize_hook_art_inputs(&inputs, &store).expect("materialize shared-memory input");

    let data_url = materialized["input"].as_str().expect("image data URL");
    assert!(data_url.starts_with("data:image/png;base64,"));
    assert_eq!(
        loom_image_io::decode_image_base64_to_rgba8(data_url)
            .expect("decode materialized image")
            .data,
        vec![10, 20, 30, 255]
    );
}

#[test]
fn formal_inline_resource_requires_bare_valid_base64() {
    let store = Arc::new(Mutex::new(SharedImageStore::new()));
    let valid = BTreeMap::from([(
        "input".to_owned(),
        HookArtPortValue::InlineResource {
            mime: "image/png".to_owned(),
            data_base64: BASE64.encode(test_png_bytes()),
            width: Some(1),
            height: Some(1),
        },
    )]);
    let materialized =
        materialize_hook_art_inputs(&valid, &store).expect("materialize inline resource");
    assert!(materialized["input"]
        .as_str()
        .is_some_and(|value| value.starts_with("data:image/png;base64,")));

    let data_url = BTreeMap::from([(
        "input".to_owned(),
        HookArtPortValue::InlineResource {
            mime: "image/png".to_owned(),
            data_base64: test_png_base64(),
            width: None,
            height: None,
        },
    )]);
    assert!(materialize_hook_art_inputs(&data_url, &store)
        .expect_err("data URL must be rejected")
        .contains("bare base64"));

    let invalid = BTreeMap::from([(
        "input".to_owned(),
        HookArtPortValue::InlineResource {
            mime: "image/png".to_owned(),
            data_base64: "not-base64".to_owned(),
            width: None,
            height: None,
        },
    )]);
    assert!(materialize_hook_art_inputs(&invalid, &store)
        .expect_err("invalid base64 must be rejected")
        .contains("not valid base64"));
}

#[test]
fn formal_output_extracts_image_content_blocks() {
    let data_url = test_png_base64();
    assert_eq!(
        extract_art_image_data_url(&json!({
            "content": [{
                "type": "image",
                "mimeType": "image/png",
                "data": data_url,
            }]
        })),
        Some(data_url.clone())
    );
    let bare = data_url.split_once(',').expect("test data URL").1;
    assert_eq!(
        extract_art_image_data_url(&json!({
            "content": [{
                "type": "image",
                "mimeType": "image/png",
                "data": bare,
            }]
        })),
        Some(data_url)
    );
}

#[test]
fn formal_image_output_falls_back_to_inline_when_shared_memory_is_unavailable() {
    let output = hook_art_image_value_with(&test_png_base64(), || {
        Err("shared memory unavailable".to_owned())
    });
    let HookArtPortValue::InlineResource {
        mime,
        data_base64,
        width,
        height,
    } = output
    else {
        panic!("failed shared memory must produce an inline resource")
    };
    assert_eq!(mime, "image/png");
    assert!(!data_base64.starts_with("data:"));
    assert_eq!(width, Some(1));
    assert_eq!(height, Some(1));
}

#[test]
fn hook_art_result_preserves_all_declared_output_ports_and_transport() {
    let mut tool = ToolDefinition::new(
        "multi-output",
        "Multi output",
        "test",
        ToolExecution::FrameworkArt {
            framework: "process".to_owned(),
        },
    );
    tool.outputs = vec![
        json!({ "name": "output_image", "type": "image" }),
        json!({ "name": "score", "type": "number" }),
    ];
    let image = test_png_base64();
    let result = json!({ "output_image": image, "score": 0.9 });
    let store = Arc::new(Mutex::new(SharedImageStore::new()));

    let websocket = hook_art_result_outputs(&tool, &result, &store, false);
    assert!(matches!(
        websocket.get("output_image"),
        Some(HookArtPortValue::InlineResource { .. })
    ));
    assert_eq!(
        websocket.get("score"),
        Some(&HookArtPortValue::Value { value: json!(0.9) })
    );

    let native = hook_art_result_outputs(&tool, &result, &store, true);
    assert!(matches!(
        native.get("output_image"),
        Some(HookArtPortValue::SharedMemory { .. })
    ));
    assert_eq!(native.len(), 2);
}

#[test]
fn hook_art_resource_release_closes_shared_memory_and_is_idempotent() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    clear_hook_canvas_runtime_state(None);
    let store = Arc::new(Mutex::new(SharedImageStore::new()));
    let execution = hook_art_request("request:1", "node:1", 1);
    assert!(matches!(
        reserve_hook_art_request(&execution, &store),
        HookArtReservation::Execute(_)
    ));
    let image = store
        .lock()
        .expect("shared image store")
        .create_rgba8(1, 1, vec![10, 20, 30, 255])
        .expect("create shared image");
    assert!(register_hook_art_resource_handles(
        &execution,
        &BTreeSet::from([image.handle.clone()]),
        true,
    ));
    finish_hook_art_request(
        &execution.request_id,
        &execution.node_id,
        execution.generation,
        execution.device_id.as_deref(),
        hook_art_request_fingerprint(&execution),
        HookRequestStatus::Succeeded,
        "{}".to_owned(),
        &store,
    );
    let request = HookArtResourcesReleaseRequest {
        protocol_version: loom_protocol::HOOK_PROTOCOL_VERSION.to_owned(),
        request_id: "release:request:1".to_owned(),
        execution_request_id: execution.request_id.clone(),
        node_id: "node:1".to_owned(),
        generation: 1,
        device_id: Some("device:local".to_owned()),
        handles: vec![image.handle.clone()],
    };
    let released: Value = serde_json::from_str(&release_hook_art_resources(&request, &store))
        .expect("release response");
    assert_eq!(released["status"], "succeeded");
    assert_eq!(released["data"]["released"][0], image.handle);
    assert!(store.lock().expect("shared image store").list().is_empty());

    let repeated: Value = serde_json::from_str(&release_hook_art_resources(&request, &store))
        .expect("idempotent release response");
    assert_eq!(repeated["status"], "succeeded");
    assert_eq!(repeated["data"]["missing"][0], image.handle);
    let HookArtReservation::Reject(replay) = reserve_hook_art_request(&execution, &store) else {
        panic!("released shared-memory results must not replay stale handles");
    };
    let replay: HookResponse = serde_json::from_str(&replay).expect("replay rejection");
    assert_eq!(
        replay.error.expect("replay error").code,
        "request_resources_released"
    );
    clear_hook_canvas_runtime_state(Some(&store));
}

#[test]
fn hook_art_resource_release_rejects_cross_execution_identity() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    clear_hook_canvas_runtime_state(None);
    let store = Arc::new(Mutex::new(SharedImageStore::new()));
    let owner = hook_art_request("request:owner", "node:owner", 7);
    assert!(matches!(
        reserve_hook_art_request(&owner, &store),
        HookArtReservation::Execute(_)
    ));
    let owner_image = store
        .lock()
        .expect("shared image store")
        .create_rgba8(1, 1, vec![1, 2, 3, 255])
        .expect("create owner image");
    assert!(register_hook_art_resource_handles(
        &owner,
        &BTreeSet::from([owner_image.handle.clone()]),
        true,
    ));

    let base_release = HookArtResourcesReleaseRequest {
        protocol_version: loom_protocol::HOOK_PROTOCOL_VERSION.to_owned(),
        request_id: "release:owner".to_owned(),
        execution_request_id: owner.request_id.clone(),
        node_id: owner.node_id.clone(),
        generation: owner.generation,
        device_id: owner.device_id.clone(),
        handles: vec![owner_image.handle.clone()],
    };
    for (request, expected_code) in [
        (
            HookArtResourcesReleaseRequest {
                execution_request_id: "request:missing".to_owned(),
                ..base_release.clone()
            },
            "execution_request_not_found",
        ),
        (
            HookArtResourcesReleaseRequest {
                device_id: Some("device:other".to_owned()),
                ..base_release.clone()
            },
            "resource_release_device_mismatch",
        ),
        (
            HookArtResourcesReleaseRequest {
                node_id: "node:other".to_owned(),
                ..base_release.clone()
            },
            "resource_release_identity_mismatch",
        ),
        (
            HookArtResourcesReleaseRequest {
                generation: owner.generation + 1,
                ..base_release.clone()
            },
            "resource_release_identity_mismatch",
        ),
    ] {
        let response: HookResponse =
            serde_json::from_str(&release_hook_art_resources(&request, &store))
                .expect("release rejection");
        assert_eq!(response.status, HookRequestStatus::Failed);
        assert_eq!(response.error.expect("release error").code, expected_code);
        assert!(store
            .lock()
            .expect("shared image store")
            .list()
            .iter()
            .any(|image| image.handle == owner_image.handle));
    }

    let other = hook_art_request("request:other", "node:other", 1);
    assert!(matches!(
        reserve_hook_art_request(&other, &store),
        HookArtReservation::Execute(_)
    ));
    let other_image = store
        .lock()
        .expect("shared image store")
        .create_rgba8(1, 1, vec![4, 5, 6, 255])
        .expect("create other image");
    assert!(register_hook_art_resource_handles(
        &other,
        &BTreeSet::from([other_image.handle.clone()]),
        true,
    ));
    let cross_handle = HookArtResourcesReleaseRequest {
        handles: vec![other_image.handle.clone()],
        ..base_release.clone()
    };
    let response: HookResponse =
        serde_json::from_str(&release_hook_art_resources(&cross_handle, &store))
            .expect("cross-handle rejection");
    assert_eq!(
        response.error.expect("cross-handle error").code,
        "resource_release_ownership_mismatch"
    );
    assert_eq!(store.lock().expect("shared image store").list().len(), 2);

    let released: HookResponse =
        serde_json::from_str(&release_hook_art_resources(&base_release, &store))
            .expect("owned release");
    assert_eq!(released.status, HookRequestStatus::Succeeded);
    assert_eq!(store.lock().expect("shared image store").list().len(), 1);
    clear_hook_canvas_runtime_state(Some(&store));
    assert!(store.lock().expect("shared image store").list().is_empty());
}

#[test]
fn released_preview_resources_do_not_invalidate_live_final_replay() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    clear_hook_canvas_runtime_state(None);
    let store = Arc::new(Mutex::new(SharedImageStore::new()));
    let execution = hook_art_request("request:preview-final", "node:preview-final", 1);
    assert!(matches!(
        reserve_hook_art_request(&execution, &store),
        HookArtReservation::Execute(_)
    ));
    let preview = store
        .lock()
        .expect("shared image store")
        .create_rgba8(1, 1, vec![1, 2, 3, 255])
        .expect("create preview image");
    assert!(register_hook_art_resource_handles(
        &execution,
        &BTreeSet::from([preview.handle.clone()]),
        false,
    ));
    let preview_release = HookArtResourcesReleaseRequest {
        protocol_version: loom_protocol::HOOK_PROTOCOL_VERSION.to_owned(),
        request_id: "release:preview".to_owned(),
        execution_request_id: execution.request_id.clone(),
        node_id: execution.node_id.clone(),
        generation: execution.generation,
        device_id: execution.device_id.clone(),
        handles: vec![preview.handle],
    };
    let preview_release: HookResponse =
        serde_json::from_str(&release_hook_art_resources(&preview_release, &store))
            .expect("preview release response");
    assert_eq!(preview_release.status, HookRequestStatus::Succeeded);

    let final_image = store
        .lock()
        .expect("shared image store")
        .create_rgba8(1, 1, vec![4, 5, 6, 255])
        .expect("create final image");
    assert!(register_hook_art_resource_handles(
        &execution,
        &BTreeSet::from([final_image.handle]),
        true,
    ));
    finish_hook_art_request(
        &execution.request_id,
        &execution.node_id,
        execution.generation,
        execution.device_id.as_deref(),
        hook_art_request_fingerprint(&execution),
        HookRequestStatus::Succeeded,
        "{\"status\":\"succeeded\"}".to_owned(),
        &store,
    );
    assert!(matches!(
        reserve_hook_art_request(&execution, &store),
        HookArtReservation::Replay(_)
    ));
    clear_hook_canvas_runtime_state(Some(&store));
    assert!(store.lock().expect("shared image store").list().is_empty());
}
