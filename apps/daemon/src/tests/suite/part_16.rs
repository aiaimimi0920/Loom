// Loom daemon tests fragment 16; included into the shared crate test module.
#[test]
fn daemon_prefers_data_url_hook_canvas_preview_over_file_backed_src() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let appdata = unique_temp_dir("hook-canvas-preview-data-url");
    let session_dir = appdata.join("com.yamiyu.hook");
    let images = session_dir.join("images");
    fs::create_dir_all(&images).expect("create session images");

    let preferred_png = test_png_bytes();
    let preferred_data_url = test_png_base64();

    let fallback_png = {
        let image = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(1, 1, vec![200, 100, 50, 255])
            .expect("fallback png image");
        let mut bytes = Vec::new();
        DynamicImage::ImageRgba8(image)
            .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
            .expect("encode fallback png");
        bytes
    };
    fs::write(images.join("original.png"), &fallback_png).expect("write fallback image");
    fs::write(
        session_dir.join("session.json"),
        format!(
            r#"{{
                  "stickers": [
                    {{
                      "id":"capture",
                      "type":"sticker",
                      "src":"images/original.png",
                      "previewSrc":"{}"
                    }}
                  ],
                  "links": []
                }}"#,
            preferred_data_url
        ),
    )
    .expect("write session");

    let previous = std::env::var("APPDATA").ok();
    std::env::set_var("APPDATA", &appdata);

    let response = hook_canvas_preview_response("capture").expect("data-url preview response");
    let body = expect_binary_route_response(response, 200, "image/png");
    assert_eq!(
        body, preferred_png,
        "daemon should serve previewSrc data URL instead of falling back to src",
    );

    restore_env("APPDATA", previous);
    fs::remove_dir_all(appdata).expect("cleanup");
}

#[test]
fn daemon_validates_hook_canvas_preview_type_and_size() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let appdata = unique_temp_dir("hook-canvas-preview-validation");
    let session_dir = appdata.join("com.yamiyu.hook");
    let images = session_dir.join("images");
    fs::create_dir_all(&images).expect("create session images");
    fs::write(images.join("unsupported.bin"), b"not-an-image").expect("write unsupported");
    let oversized_path = images.join("oversized.png");
    fs::File::create(&oversized_path)
        .expect("create oversized")
        .set_len(hook_canvas::MAX_PREVIEW_BYTES + 1)
        .expect("size oversized");
    fs::write(images.join("pixel.jpg"), [0xff, 0xd8, 0xff, 0xe0]).expect("write jpeg");
    fs::write(images.join("pixel.webp"), b"RIFF\x04\x00\x00\x00WEBP").expect("write webp");
    fs::write(
        session_dir.join("session.json"),
        r#"{
              "stickers": [
                {"id":"unsupported","type":"sticker","src":"images/unsupported.bin"},
                {"id":"oversized","type":"sticker","src":"images/oversized.png"},
                {"id":"jpeg","type":"sticker","src":"images/pixel.jpg"},
                {"id":"webp","type":"sticker","src":"images/pixel.webp"}
              ],
              "links": []
            }"#,
    )
    .expect("write Hook session");
    let previous = std::env::var("APPDATA").ok();
    std::env::set_var("APPDATA", &appdata);

    let unsupported =
        hook_canvas_preview_response("unsupported").expect("unsupported preview response");
    expect_text_route_response(unsupported, 415);

    let oversized = hook_canvas_preview_response("oversized").expect("oversized preview response");
    expect_text_route_response(oversized, 413);

    for (node_id, expected_type) in [("jpeg", "image/jpeg"), ("webp", "image/webp")] {
        let response = hook_canvas_preview_response(node_id)
            .unwrap_or_else(|error| panic!("preview response for {node_id}: {error:#}"));
        expect_binary_route_response(response, 200, expected_type);
    }

    restore_env("APPDATA", previous);
    fs::remove_dir_all(appdata).expect("cleanup");
}

#[test]
fn daemon_preserves_auth_and_structured_errors_for_hook_canvas_routes() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let appdata = unique_temp_dir("hook-canvas-auth-appdata");
    let control_plane_root = unique_temp_dir("hook-canvas-auth-control-plane");
    let session_dir = appdata.join("com.yamiyu.hook");
    let images = session_dir.join("images");
    fs::create_dir_all(&images).expect("create session images");
    fs::write(images.join("capture.png"), test_png_bytes()).expect("write preview");
    fs::write(
        session_dir.join("session.json"),
        r#"{"stickers":[{"id":"capture","type":"sticker","src":"images/capture.png"}],"links":[]}"#,
    )
    .expect("write Hook session");
    let previous = std::env::var("APPDATA").ok();
    std::env::set_var("APPDATA", &appdata);
    let runtime = test_daemon_runtime(&control_plane_root, Some("canvas-secret"));

    let unauthorized = route_request(
        &runtime,
        &parsed_request(
            "GET",
            "/v1/hook-bridge/canvas/nodes/capture/preview",
            &[],
            None,
        ),
    );
    let unauthorized_body = expect_text_route_response(unauthorized, 401);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&unauthorized_body).expect("unauthorized json")
            ["error"]["code"],
        "unauthorized"
    );

    let authorized = route_request(
        &runtime,
        &parsed_request(
            "GET",
            "/v1/hook-bridge/canvas/nodes/capture/preview",
            &[("Authorization", "Bearer canvas-secret")],
            None,
        ),
    );
    expect_binary_route_response(authorized, 200, "image/png");

    fs::write(session_dir.join("session.json"), "{not-json").expect("corrupt session");
    let malformed_runtime = test_daemon_runtime(&control_plane_root, None);
    let response = route_request(
        &malformed_runtime,
        &parsed_request("GET", "/v1/hook-bridge/canvas", &[], None),
    );
    let body = expect_text_route_response(response, 500);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&body).expect("malformed session json")["error"]
            ["code"],
        "hook_canvas_error"
    );

    restore_env("APPDATA", previous);
    fs::remove_dir_all(appdata).expect("cleanup");
    fs::remove_dir_all(control_plane_root).expect("cleanup control plane");
}

#[test]
fn settings_store_rejects_incomplete_settings_schema() {
    let root = unique_temp_dir("loom-settings-strict-schema");
    fs::create_dir_all(&root).expect("create settings root");
    let path = root.join("settings.json");
    let mut value =
        serde_json::to_value(LoomSettings::default()).expect("serialize default settings");
    let settings = value.as_object_mut().expect("settings object");
    settings["general"]["theme"] = json!("light");
    settings.remove("loom_cache");
    settings.remove("mcp");
    settings.remove("art_store");
    assert!(serde_json::from_value::<LoomSettings>(value.clone()).is_err());
    fs::write(
        &path,
        serde_json::to_vec_pretty(&value).expect("settings json"),
    )
    .expect("write settings");

    let store = LoomSettingsStore::new(path);

    assert_eq!(store.settings.general.theme, "dark");
    assert_eq!(store.settings.loom_cache, LoomCacheSettings::default());
    assert_eq!(store.settings.mcp, McpSettings::default());
    assert_eq!(store.settings.art_store, ArtStoreSettings::default());
    fs::remove_dir_all(root).expect("cleanup settings root");
}

#[test]
fn settings_persist_mcp_limits_and_global_art_update_policy() {
    struct RuntimeSettingsReset;
    impl Drop for RuntimeSettingsReset {
        fn drop(&mut self) {
            apply_runtime_settings(&LoomSettings::default());
        }
    }

    let _runtime_settings_reset = RuntimeSettingsReset;
    let root = unique_temp_dir("loom-runtime-settings");
    let settings_store = Arc::new(Mutex::new(LoomSettingsStore::new(
        root.join("settings.json"),
    )));
    let hook_bridge = Arc::new(Mutex::new(HookBridgeRuntime::new(root.join("workflows"))));
    let mut settings = LoomSettings::default();
    settings.mcp.request_timeout_seconds = 120;
    settings.mcp.memory_limit_bytes = 1024 * 1024 * 1024;
    settings.network.loom.mode = "disabled".to_owned();
    settings.system.loom_log_level = "error".to_owned();
    settings.art_store.auto_update = false;
    let body = serde_json::to_string(&settings).expect("settings body");

    let (status, _) =
        put_settings(&body, &settings_store, &hook_bridge).expect("save runtime settings");

    assert_eq!(status, 200);
    let saved_settings = settings_store.lock().expect("settings store").settings.clone();
    assert_eq!(saved_settings.mcp.request_timeout_seconds, 120);
    assert_eq!(saved_settings.mcp.memory_limit_bytes, 1024 * 1024 * 1024);
    assert_eq!(saved_settings.network.loom.mode, "disabled");
    assert_eq!(
        parse_runtime_log_level(&saved_settings.system.loom_log_level),
        RuntimeLogLevel::Error
    );
    let tool_registry = ToolRegistry::new(root.join("tools"));
    let framework_registry = FrameworkRegistry::new(&root);
    let workflow_store = WorkflowStore::new(root.join("workflows"));
    let (update_status, update_body) = auto_update_arts(
        &tool_registry,
        &framework_registry,
        &workflow_store,
        &root,
        &hook_bridge,
        &settings_store,
    )
    .expect("skip globally disabled Art updates");
    assert_eq!(update_status, 200);
    assert_eq!(
        serde_json::from_str::<Value>(&update_body).expect("update response")["disabled"],
        true
    );
    fs::remove_dir_all(root).expect("cleanup settings root");
}

#[test]
fn art_store_rejects_caller_selected_remote_store() {
    let (status, body) =
        fetch_art_store_catalog("/v1/arts/store/catalog?store=https%3A%2F%2Fthird-party.example")
            .expect("custom store rejection response");

    assert_eq!(status, 400);
    assert_eq!(
        serde_json::from_str::<Value>(&body).expect("error response")["error"]["code"],
        "custom_art_store_not_supported"
    );
    assert!(custom_art_store_requested(Some(
        "https://third-party.example"
    )));
    assert!(!custom_art_store_requested(None));
}

#[test]
fn art_store_latest_requests_resolve_to_a_cataloged_exact_version() {
    let catalog = RemoteArtStoreCatalog {
        arts: vec![RemoteArtStoreEntry {
            id: "canonical-art".to_owned(),
            latest_version: "1.2.0".to_owned(),
            versions: vec![
                RemoteArtStoreVersion {
                    version: "1.0.0".to_owned(),
                    sha256: "a".repeat(64),
                },
                RemoteArtStoreVersion {
                    version: "1.2.0".to_owned(),
                    sha256: "b".repeat(64),
                },
            ],
            ..RemoteArtStoreEntry::default()
        }],
    };

    assert_eq!(
        resolve_art_store_package_version(Some(&catalog), "canonical-art", None).unwrap(),
        "1.2.0"
    );
    assert_eq!(
        resolve_art_store_package_version(None, "canonical-art", Some("1.0.0")).unwrap(),
        "1.0.0"
    );
    assert!(resolve_art_store_package_version(Some(&catalog), "missing", None).is_err());
    assert!(resolve_art_store_package_version(None, "canonical-art", Some("latest")).is_err());
}

#[test]
fn hook_cache_settings_and_library_commands_use_the_hook_bridge_channel() {
    let root = unique_temp_dir("hook-cache-control");
    let settings_store = Arc::new(Mutex::new(LoomSettingsStore::new(
        root.join("settings.json"),
    )));
    let hook_bridge = Arc::new(Mutex::new(HookBridgeRuntime::new(root.join("workflows"))));
    let hub = hook_bridge
        .lock()
        .expect("hook bridge")
        .broadcast_hub
        .clone();
    let (rx, _subscription) =
        register_hook_bridge_subscription(&hub, vec![HOOK_EVENT_CACHE_CONTROL.to_owned()]);

    let body = serde_json::to_string(&LoomSettings::default()).expect("settings body");
    let (status, _) =
        put_settings(&body, &settings_store, &hook_bridge).expect("save cache settings");
    assert_eq!(status, 200);
    let settings_event: Value = serde_json::from_str(
        &rx.recv_timeout(Duration::from_secs(1))
            .expect("settings event"),
    )
    .expect("settings json");
    assert_eq!(settings_event["method"], HOOK_EVENT_CACHE_CONTROL);
    assert_eq!(settings_event["params"]["action"], "settings");
    assert_eq!(
        settings_event["params"]["settings"]["recycleBinMaxEntries"],
        15
    );

    let (status, _) =
        broadcast_hook_cache_control(r#"{"action":"clearReferenceLibrary"}"#, &hook_bridge)
            .expect("broadcast reference clear");
    assert_eq!(status, 200);
    let clear_event: Value = serde_json::from_str(
        &rx.recv_timeout(Duration::from_secs(1))
            .expect("clear event"),
    )
    .expect("clear json");
    assert_eq!(clear_event["params"]["action"], "clearReferenceLibrary");

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn hook_receives_full_settings_after_settings_or_shortcut_updates() {
    let root = unique_temp_dir("hook-settings-updated");
    let settings_store = Arc::new(Mutex::new(LoomSettingsStore::new(
        root.join("settings.json"),
    )));
    let hook_bridge = Arc::new(Mutex::new(HookBridgeRuntime::new(root.join("workflows"))));
    let hub = hook_bridge
        .lock()
        .expect("hook bridge")
        .broadcast_hub
        .clone();
    let (rx, _subscription) =
        register_hook_bridge_subscription(&hub, vec![HOOK_EVENT_SETTINGS_UPDATED.to_owned()]);

    let mut settings = LoomSettings::default();
    settings.general.theme = "dark".to_owned();
    let body = serde_json::to_string(&settings).expect("settings body");
    put_settings(&body, &settings_store, &hook_bridge).expect("save settings");
    let settings_event: Value = serde_json::from_str(
        &rx.recv_timeout(Duration::from_secs(1))
            .expect("full settings event"),
    )
    .expect("settings json");
    assert_eq!(settings_event["method"], HOOK_EVENT_SETTINGS_UPDATED);
    assert_eq!(
        settings_event["params"]["settings"]["general"]["theme"],
        "dark"
    );

    let shortcut = LoomShortcutConfig {
        id: "capture".to_owned(),
        label: "Screenshot".to_owned(),
        keys: "Alt+9 / F8".to_owned(),
        enabled: true,
    };
    put_shortcut(
        "capture",
        &serde_json::to_string(&shortcut).expect("shortcut body"),
        &settings_store,
        &hook_bridge,
    )
    .expect("save shortcut");
    let shortcut_event: Value = serde_json::from_str(
        &rx.recv_timeout(Duration::from_secs(1))
            .expect("shortcut settings event"),
    )
    .expect("shortcut settings json");
    assert_eq!(
        shortcut_event["params"]["settings"]["shortcuts"]["capture"]["keys"],
        "Alt+9 / F8"
    );
    assert_eq!(
        shortcut_event["params"]["settings"]["general"]["theme"],
        "dark"
    );

    fs::remove_dir_all(root).expect("cleanup");
}
