//! Diagnostics, endpoint selection, cache, snapshot, and JSON transport contracts.

use super::*;

#[test]
fn application_diagnostics_expose_build_repositories_and_six_character_commits() {
    for (app, repository) in [
        ("loom", "https://github.com/aiaimimi0920/Loom"),
        ("hook", "https://github.com/aiaimimi0920/Hook"),
    ] {
        let diagnostics = application_diagnostics(app).expect("application diagnostics");
        assert_eq!(diagnostics.repository_url.as_deref(), Some(repository));
        let commit = diagnostics.commit_short.expect("embedded commit");
        assert_eq!(commit.len(), 6);
        assert!(commit.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert!(is_allowed_repository_url(repository));
    }
    assert!(!is_allowed_repository_url("https://example.com/untrusted"));
}

#[test]
fn embedded_build_commits_are_normalized_to_six_hex_characters() {
    assert_eq!(normalize_build_commit("abcdef1"), Some("abcdef".to_owned()));
    assert_eq!(normalize_build_commit("abcdef"), Some("abcdef".to_owned()));
    assert_eq!(normalize_build_commit("unknown"), None);
}

#[test]
fn mcp_source_urls_allow_safe_https_and_reject_unsafe_targets() {
    assert!(is_safe_external_https_url(
        "https://github.com/modelcontextprotocol/servers"
    ));
    assert!(is_safe_external_https_url(
        "https://registry.modelcontextprotocol.io/v0.1/servers"
    ));
    assert!(!is_safe_external_https_url("http://example.com/mcp"));
    assert!(!is_safe_external_https_url(
        "https://user:secret@example.com/mcp"
    ));
    assert!(!is_safe_external_https_url("javascript:alert(1)"));
    assert!(!is_safe_external_https_url("not-a-url"));
}

#[test]
fn mcp_registry_gets_a_timeout_longer_than_the_daemon_outbound_fetch() {
    assert_eq!(
        daemon_get_timeout("/v1/mcp/registry?limit=100&cursor=opaque"),
        LOOM_MCP_REGISTRY_REQUEST_TIMEOUT
    );
    assert_eq!(
        daemon_get_timeout("/health"),
        LOOM_DAEMON_DEFAULT_REQUEST_TIMEOUT
    );
    assert!(LOOM_MCP_REGISTRY_REQUEST_TIMEOUT > Duration::from_secs(40));
}

#[test]
fn daemon_commands_prefer_the_active_owned_daemon_over_a_stale_frontend_url() {
    assert_eq!(
        resolve_command_base_url_with_active(
            DEFAULT_LOOM_DAEMON_URL.to_owned(),
            Some("http://127.0.0.1:49321/".to_owned()),
        ),
        "http://127.0.0.1:49321"
    );
    assert_eq!(
        resolve_command_base_url_with_active("http://127.0.0.1:18765/".to_owned(), None,),
        "http://127.0.0.1:18765"
    );
}

#[test]
fn loom_general_settings_restore_and_apply_close_to_tray() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let root = unique_temp_dir("general-settings");
    let settings_dir = root.join("settings");
    fs::create_dir_all(&settings_dir).expect("settings dir");
    fs::write(
        settings_dir.join("settings.json"),
        br#"{"general":{"minimizeToTray":false}}"#,
    )
    .expect("settings file");
    let previous_root = std::env::var("LOOM_CONTROL_PLANE_ROOT").ok();
    std::env::set_var("LOOM_CONTROL_PLANE_ROOT", &root);

    assert_eq!(
        read_loom_persisted_general_settings(),
        Some(LoomGeneralRuntimeSettings {
            minimize_to_tray: false,
        })
    );
    apply_loom_general_settings(LoomGeneralRuntimeSettings {
        minimize_to_tray: false,
    });
    assert!(!LOOM_CLOSE_TO_TRAY.load(Ordering::Relaxed));

    restore_env("LOOM_CONTROL_PLANE_ROOT", previous_root);
    LOOM_CLOSE_TO_TRAY.store(true, Ordering::Relaxed);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn hook_cache_snapshot_and_clear_only_manage_hook_temporary_files() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let root = unique_temp_dir("hook-cache");
    let clipboard = root.join("clipboard");
    let app_data = root.join("app-data");
    let image_search = app_data.join("image-search-cache");
    fs::create_dir_all(&clipboard).expect("create clipboard cache");
    fs::create_dir_all(&image_search).expect("create image search cache");
    fs::write(clipboard.join("capture.png"), vec![1_u8; 12]).expect("write clipboard cache");
    fs::write(image_search.join("remote_test.png"), vec![2_u8; 20])
        .expect("write image search cache");
    fs::write(
        app_data.join("session.json"),
        br#"{"recycleBin":[{},{}],"referenceLibrary":[{}]}"#,
    )
    .expect("write session");
    fs::write(
        app_data.join("app-settings.json"),
        br#"{"cache":{"recycleBinMaxEntries":50,"recycleBinRetentionDays":30,"tempCacheMaxBytes":0,"tempCacheRetentionDays":0}}"#,
    )
    .expect("write app settings");
    let previous_clipboard = std::env::var("HOOK_CLIPBOARD_CACHE_DIR").ok();
    let previous_app_data = std::env::var("HOOK_APPDATA_DIR").ok();
    std::env::set_var("HOOK_CLIPBOARD_CACHE_DIR", &clipboard);
    std::env::set_var("HOOK_APPDATA_DIR", &app_data);

    let before = hook_cache_snapshot().expect("cache snapshot");
    assert_eq!(
        read_hook_persisted_cache_settings(),
        Some(HookCachePreferences {
            recycle_bin_max_entries: 50,
            recycle_bin_retention_days: 30,
            temp_cache_max_bytes: 0,
            temp_cache_retention_days: 0,
        })
    );
    assert_eq!(before.temporary.bytes, 12);
    assert_eq!(before.recycle_bin_entries, 2);
    assert_eq!(before.reference_entries, 1);
    let cleared = clear_hook_cache_blocking("temporary").expect("clear temporary cache");
    assert_eq!(cleared.freed_bytes, 12);
    assert_eq!(cleared.snapshot.temporary.bytes, 0);
    assert!(image_search.join("remote_test.png").is_file());

    restore_env("HOOK_CLIPBOARD_CACHE_DIR", previous_clipboard);
    restore_env("HOOK_APPDATA_DIR", previous_app_data);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn persisted_cache_settings_ignore_oversized_files() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let root = unique_temp_dir("oversized-settings");
    let previous_hook = std::env::var("HOOK_APPDATA_DIR").ok();
    let previous_loom = std::env::var("LOOM_CONTROL_PLANE_ROOT").ok();
    std::env::set_var("HOOK_APPDATA_DIR", &root);
    std::env::set_var("LOOM_CONTROL_PLANE_ROOT", &root);
    fs::write(
        root.join("app-settings.json"),
        vec![b' '; MAX_SETTINGS_FILE_BYTES as usize + 1],
    )
    .expect("write oversized Hook settings");
    let loom_settings = root.join("settings").join("settings.json");
    fs::create_dir_all(loom_settings.parent().expect("settings parent"))
        .expect("create settings dir");
    fs::write(
        &loom_settings,
        vec![b' '; MAX_SETTINGS_FILE_BYTES as usize + 1],
    )
    .expect("write oversized Loom settings");

    assert_eq!(read_hook_persisted_cache_settings(), None);
    assert_eq!(read_loom_persisted_cache_settings(), None);
    assert_eq!(read_loom_persisted_general_settings(), None);

    restore_env("HOOK_APPDATA_DIR", previous_hook);
    restore_env("LOOM_CONTROL_PLANE_ROOT", previous_loom);
    fs::remove_dir_all(root).expect("cleanup oversized settings");
}

#[test]
fn destructive_cache_validation_rejects_relative_and_protected_roots() {
    assert!(validate_destructive_cache_root(Path::new("relative-cache")).is_err());
    assert!(
        validate_destructive_cache_root(&std::env::temp_dir().join("child").join("..")).is_err()
    );
    let protected = std::env::temp_dir();
    let error = clear_directory_contents(&protected).expect_err("temp root must be protected");
    assert!(error.contains("拒绝清理"), "{error}");

    let root = unique_temp_dir("cache-file-root");
    let file = root.join("not-a-directory");
    fs::write(&file, b"preserve").expect("write non-directory cache root");
    assert!(clear_directory_contents(&file).is_err());
    assert_eq!(fs::read(&file).expect("read preserved file"), b"preserve");
    fs::remove_dir_all(root).expect("cleanup cache file root");
}

#[test]
fn loom_cache_snapshot_and_clear_preserve_installed_art_and_workflow_data() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let root = unique_temp_dir("loom-cache");
    let control_plane = root.join("control-plane");
    let art_version = control_plane
        .join("arts")
        .join("sample-art")
        .join("versions")
        .join("0.1.0");
    let art_cache = art_version.join(".loom-cache");
    let workflow = control_plane.join("workflows").join("saved.yaml");
    let framework_temp = root.join("framework-temporary");
    fs::create_dir_all(&art_cache).expect("create Art cache");
    fs::create_dir_all(workflow.parent().expect("workflow parent"))
        .expect("create workflow directory");
    fs::create_dir_all(&framework_temp).expect("create framework temporary directory");
    fs::write(art_version.join("manifest.json"), b"installed-art").expect("write Art manifest");
    fs::write(art_cache.join("runtime.bin"), vec![1_u8; 12]).expect("write Art cache");
    fs::write(&workflow, b"workflow").expect("write workflow");
    fs::write(framework_temp.join("request.bin"), vec![2_u8; 20])
        .expect("write framework temporary file");
    let settings_path = control_plane.join("settings").join("settings.json");
    fs::create_dir_all(settings_path.parent().expect("settings parent"))
        .expect("create settings directory");
    fs::write(
        &settings_path,
        br#"{"loom_cache":{"artCacheMaxBytes":0,"artCacheRetentionDays":0,"frameworkTempRetentionDays":0}}"#,
    )
    .expect("write Loom cache settings");

    let previous_control_plane = std::env::var("LOOM_CONTROL_PLANE_ROOT").ok();
    let previous_framework_temp = std::env::var("LOOM_FRAMEWORK_TEMP_DIR").ok();
    std::env::set_var("LOOM_CONTROL_PLANE_ROOT", &control_plane);
    std::env::set_var("LOOM_FRAMEWORK_TEMP_DIR", &framework_temp);

    assert_eq!(
        read_loom_persisted_cache_settings(),
        Some(LoomCachePreferences {
            art_cache_max_bytes: 0,
            art_cache_retention_days: 0,
            framework_temp_retention_days: 0,
        })
    );
    let before = loom_cache_snapshot().expect("read Loom cache snapshot");
    assert_eq!(before.art_runtime.bytes, 12);
    assert_eq!(before.framework_temporary.bytes, 20);

    let cleared_art = clear_loom_cache_blocking("artRuntime").expect("clear Art cache");
    assert_eq!(cleared_art.freed_bytes, 12);
    assert!(art_version.join("manifest.json").is_file());
    assert!(workflow.is_file());
    assert!(framework_temp.join("request.bin").is_file());

    let cleared_temp =
        clear_loom_cache_blocking("frameworkTemporary").expect("clear framework temporary files");
    assert_eq!(cleared_temp.freed_bytes, 20);
    assert!(art_version.join("manifest.json").is_file());
    assert!(workflow.is_file());

    fs::write(art_cache.join("old.bin"), vec![3_u8; 4]).expect("write first cache file");
    fs::write(art_cache.join("new.bin"), vec![4_u8; 4]).expect("write second cache file");
    prune_cache_roots(std::slice::from_ref(&art_cache), 5, 0).expect("enforce Art cache limit");
    assert!(directory_usage(&art_cache).expect("read pruned cache").0 <= 5);
    assert!(art_version.join("manifest.json").is_file());

    restore_env("LOOM_CONTROL_PLANE_ROOT", previous_control_plane);
    restore_env("LOOM_FRAMEWORK_TEMP_DIR", previous_framework_temp);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn offline_snapshot_preserves_settings_links() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let root = unique_temp_dir("offline-settings-links");
    let previous_root = std::env::var("LOOM_CONTROL_PLANE_ROOT").ok();
    let previous_token = std::env::var("LOOM_DAEMON_TOKEN").ok();
    std::env::set_var("LOOM_CONTROL_PLANE_ROOT", &root);
    std::env::remove_var("LOOM_DAEMON_TOKEN");
    let snapshot = read_loom_snapshot_blocking(Some("http://127.0.0.1:9".to_string()));

    assert_eq!(snapshot.base_url, "http://127.0.0.1:9");
    assert_eq!(snapshot.connection_state, "offline");
    assert_eq!(snapshot.settings.root, "http://127.0.0.1:9/settings");
    assert_eq!(snapshot.settings.tea, "http://127.0.0.1:9/settings/tea");
    assert!(snapshot.mcp_servers.is_empty());
    assert!(snapshot.tools.is_empty());
    assert!(snapshot.workflows.is_empty());
    assert!(snapshot.hook_bridge.is_none());
    assert!(snapshot.error.is_some());
    restore_env("LOOM_CONTROL_PLANE_ROOT", previous_root);
    restore_env("LOOM_DAEMON_TOKEN", previous_token);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn optional_daemon_module_failure_keeps_snapshot_online() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind snapshot fixture");
    let address = listener
        .local_addr()
        .expect("read snapshot fixture address");
    listener
        .set_nonblocking(true)
        .expect("set snapshot fixture nonblocking");
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let server = thread::spawn(move || loop {
        if shutdown_rx.try_recv().is_ok() {
            break;
        }
        let (mut stream, _) = match listener.accept() {
            Ok(connection) => connection,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(5));
                continue;
            }
            Err(error) => panic!("accept snapshot request: {error}"),
        };
        let mut request = Vec::new();
        let mut buffer = [0_u8; 512];
        loop {
            let bytes = stream.read(&mut buffer).expect("read snapshot request");
            if bytes == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..bytes]);
            if request.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
        let request = String::from_utf8(request).expect("snapshot request is utf8");
        let path = request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .expect("snapshot request path");
        let (status, body) = match path {
            "/health" => (200, r#"{"status":"ok"}"#),
            "/status" => (200, r#"{"status":"ready"}"#),
            "/v1/capabilities" => (200, r#"{"capabilities":[]}"#),
            "/v1/mcp/servers" => (200, r#"{"servers":[]}"#),
            "/v1/tools" => (500, r#"{"error":{"code":"tool_registry_error"}}"#),
            "/v1/art-authoring/python/arts" => (200, r#"{"arts":[]}"#),
            "/v1/workflows" => (200, r#"{"workflows":[]}"#),
            "/v1/hook-bridge/status" => (200, r#"{"running":false}"#),
            other => panic!("unexpected snapshot path: {other}"),
        };
        let response = format!(
                "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        stream
            .write_all(response.as_bytes())
            .expect("write snapshot response");
    });

    let snapshot =
        read_loom_snapshot_blocking(Some(format!("http://127.0.0.1:{}", address.port())));
    shutdown_tx.send(()).expect("stop snapshot fixture");
    server.join().expect("join snapshot fixture");

    assert_eq!(snapshot.connection_state, "online");
    assert_eq!(snapshot.health, Some(serde_json::json!({"status": "ok"})));
    assert_eq!(
        snapshot.status,
        Some(serde_json::json!({"status": "ready"}))
    );
    assert!(snapshot.tools.is_empty());
    assert!(snapshot
        .error
        .as_deref()
        .is_some_and(|error| error.contains("/v1/tools")));
}

#[test]
fn daemon_http_error_preserves_structured_message() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind error fixture");
    let address = listener.local_addr().expect("read error fixture address");
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept error request");
        let mut request = [0_u8; 512];
        let _ = stream.read(&mut request).expect("read error request");
        let body = r#"{"error":{"code":"framework_install_failed","message":"framework `process` has no available package source"}}"#;
        let response = format!(
            "HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream
            .write_all(response.as_bytes())
            .expect("write error response");
    });

    let error = http_post_json(
        &format!("http://127.0.0.1:{}", address.port()),
        "/v1/frameworks/process/install",
        &serde_json::json!({}),
    )
    .expect_err("framework install must fail");
    server.join().expect("join error fixture");

    assert!(error.contains("HTTP/1.1 500 Internal Server Error"));
    assert!(error.contains("no available package source"));
}

#[test]
fn daemon_http_client_accepts_successful_non_200_status() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind success fixture");
    let address = listener.local_addr().expect("read success fixture address");
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept success request");
        let mut request = [0_u8; 512];
        let _ = stream.read(&mut request).expect("read success request");
        let body = r#"{"created":true}"#;
        let response = format!(
            "HTTP/1.0 201 Created\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream
            .write_all(response.as_bytes())
            .expect("write success response");
    });

    let response = http_get_json(
        &format!("http://127.0.0.1:{}", address.port()),
        "/v1/frameworks",
    )
    .expect("201 response should be accepted");
    server.join().expect("join success fixture");

    assert_eq!(response, serde_json::json!({"created": true}));
}
