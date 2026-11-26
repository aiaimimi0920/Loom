// Loom daemon tests fragment 12; included into the shared crate test module.
/// A Surface stream long-poll used to fall through `request_concurrency_class` to `Serialized`,
/// so an idle poll held `serialized_route_lock` for its whole five-second parking period. Every
/// Surface write — events, patches, resource registrations — queued behind it, including
/// the one message that would have ended the poll early.
#[test]
fn a_serialized_route_completes_while_a_surface_stream_long_poll_is_parked() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let root = unique_temp_dir("surface-stream-does-not-serialize");

    let daemon = LoomDaemon::bind(
        DaemonConfig::localhost(0)
            .with_bounded_request_executor(3, 4)
            .with_control_plane_root(&root),
    )
    .expect("bind daemon");
    let port = daemon.local_addr().expect("address").port();
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let server = thread::spawn(move || daemon.serve_until(shutdown_rx));

    let poll = thread::spawn(move || http_get(port, "/v1/surfaces/stream?after=0&timeoutMs=3000"));
    thread::sleep(Duration::from_millis(250));
    let started = Instant::now();
    let serialized = http_get(port, "/v1/workflows");
    let waited = started.elapsed();
    let polled = poll.join().expect("Surface stream client");

    shutdown_tx.send(()).expect("request shutdown");
    server.join().expect("server thread").expect("serve daemon");

    assert!(
        serialized.starts_with("HTTP/1.1 200 OK"),
        "response did not satisfy the expected contract (raw output withheld)"
    );
    assert!(
        polled.starts_with("HTTP/1.1 200 OK"),
        "response did not satisfy the expected contract (raw output withheld)"
    );
    assert!(
        waited < Duration::from_millis(1_500),
        "a serialized route waited {waited:?} behind an idle Surface stream long-poll"
    );
    fs::remove_dir_all(root).expect("cleanup Surface stream concurrency root");
}

/// `serve_until` used to run the reader inline on the accept thread, so one client that sent a
/// partial head and then went quiet owned the daemon's front door until its read timed
/// out — two seconds in which nothing else, `/health` included, could even be accepted.
#[test]
fn a_trickling_request_does_not_block_the_accept_loop() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let root = unique_temp_dir("trickling-request-accept-loop");

    let daemon = LoomDaemon::bind(
        DaemonConfig::localhost(0)
            .with_bounded_request_executor(3, 4)
            .with_control_plane_root(&root),
    )
    .expect("bind daemon");
    let port = daemon.local_addr().expect("address").port();
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let server = thread::spawn(move || daemon.serve_until(shutdown_rx));

    let mut trickler = TcpStream::connect(("127.0.0.1", port)).expect("connect trickler");
    trickler
        .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\n")
        .expect("write a partial request head");
    trickler.flush().expect("flush the partial head");
    thread::sleep(Duration::from_millis(150));

    let started = Instant::now();
    let health = http_get(port, "/health");
    let waited = started.elapsed();

    // Finishing the head lets the read worker return at once, so the join below stays quick.
    trickler.write_all(b"\r\n").expect("finish the head");
    trickler.flush().expect("flush the head terminator");
    drop(trickler);
    shutdown_tx.send(()).expect("request shutdown");
    server.join().expect("server thread").expect("serve daemon");

    assert!(
        health.starts_with("HTTP/1.1 200 OK"),
        "response did not satisfy the expected contract (raw output withheld)"
    );
    assert!(
        waited < Duration::from_millis(1_500),
        "/health waited {waited:?} behind a client that had sent only a partial head"
    );
    fs::remove_dir_all(root).expect("cleanup trickling request root");
}

#[test]
fn request_concurrency_classification_is_conservative() {
    let request = |method: &str, path: &str, capability: Option<&str>| ParsedHttpRequest {
        method: method.to_owned(),
        path: path.to_owned(),
        headers: Vec::new(),
        body: capability
            .map(|capability| serde_json::json!({ "capability": capability }).to_string())
            .unwrap_or_default(),
    };

    let concurrent = [
        ("GET", "/health", None),
        ("GET", "/status", None),
        ("GET", "/v1/capabilities", None),
        ("GET", "/v1/ai/status", None),
        ("GET", "/v1/ai/models", None),
        ("GET", "/v1/hook-bridge/canvas", None),
        ("GET", "/v1/hook-bridge/canvas/nodes/capture/preview", None),
        ("GET", "/v1/surfaces/stream", None),
        ("GET", "/v1/surfaces/stream?since=7", None),
        ("GET", "/v1/live/sessions/session-1/events", None),
        ("GET", "/v1/runs/run-1", None),
        ("GET", "/v1/runs/run-1/events", None),
        ("POST", "/v1/ai/chat/completions", None),
        ("POST", "/v1/asset-library/download", None),
        ("POST", "/v1/asset-library/download-control", None),
        ("POST", "/v1/tools/tool-1/execute", None),
        ("POST", "/v1/invoke", Some("ai.chat.v1")),
        ("POST", "/v1/invoke", Some("loom.runtime.probe.v1")),
        ("POST", "/v1/invoke", Some("ocr.recognize.v1")),
    ];
    for (method, path, capability) in concurrent {
        assert_eq!(
            request_concurrency_class(&request(method, path, capability)),
            RequestConcurrencyClass::Concurrent,
            "expected concurrent classification for {method} {path} {capability:?}"
        );
    }

    let serialized = [
        ("GET", "/v1/workflows", None),
        ("PUT", "/v1/workflows/workflow-1", None),
        ("POST", "/v1/workflows/workflow-1/execute", None),
    ];
    for (method, path, capability) in serialized {
        assert_eq!(
            request_concurrency_class(&request(method, path, capability)),
            RequestConcurrencyClass::Serialized,
            "expected serialized classification for {method} {path} {capability:?}"
        );
    }

    let invalid_invoke = ParsedHttpRequest {
        method: "POST".to_owned(),
        path: "/v1/invoke".to_owned(),
        headers: Vec::new(),
        body: "not-json".to_owned(),
    };
    assert_eq!(
        request_concurrency_class(&invalid_invoke),
        RequestConcurrencyClass::Serialized
    );
}

#[test]
fn daemon_reports_in_memory_run_store_by_default() {
    let root = unique_temp_dir("status-run-store-memory");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
    let status = expect_json_text_route_response(
        route_request(&runtime, &parsed_request("GET", "/status", &[], None)),
        200,
    );

    assert_eq!(status["run_store"]["mode"], "memory");
    assert_eq!(status["run_store"]["persistent"], false);
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn daemon_reports_explicit_sqlite_run_store() {
    let root = unique_temp_dir("sqlite-status");
    let path = root.join("runs").join("loom-runs.sqlite3");
    let runtime = test_daemon_runtime_from_config(
        &root,
        DaemonConfig::localhost(0).with_sqlite_run_store(&path),
    );
    let status = expect_json_text_route_response(
        route_request(&runtime, &parsed_request("GET", "/status", &[], None)),
        200,
    );

    assert_eq!(status["run_store"]["mode"], "sqlite");
    assert_eq!(status["run_store"]["persistent"], true);
    assert!(status["run_store"].get("path").is_none());
    assert!(!status
        .to_string()
        .contains(&path.to_string_lossy().to_string()));
    drop(runtime);
    assert!(path.exists());
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn daemon_reports_configured_ai_gateway_without_auth_token() {
    let config = DaemonConfig::localhost(0).with_ai_gateway(AiGatewayConfig::Gateway(
        AiGatewayEndpointConfig {
            base_url: "http://192.168.1.50:4200".to_owned(),
            auth_token: Some("do-not-expose".to_owned()),
            default_model: Some("test-model".to_owned()),
            timeout: Duration::from_secs(12),
        },
    ));
    let root = unique_temp_dir("status-ai-gateway");
    let runtime = test_daemon_runtime_from_config(&root, config);
    let status = expect_json_text_route_response(
        route_request(&runtime, &parsed_request("GET", "/status", &[], None)),
        200,
    );

    assert_eq!(status["ai_gateway"]["mode"], "gateway");
    assert_eq!(status["ai_gateway"]["configured"], true);
    assert_eq!(status["ai_gateway"]["origin_kind"], "lan");
    assert_eq!(status["ai_gateway"]["model"], "test-model");
    assert_eq!(status["ai_gateway"]["timeout_seconds"], 12);
    assert_eq!(status["ai_gateway"]["has_token"], true);
    assert!(status["ai_gateway"].get("auth_token").is_none());
    assert!(!status.to_string().contains("do-not-expose"));
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn daemon_rejects_invalid_gateway_timeout_configuration() {
    let mut values = std::collections::HashMap::new();
    values.insert("LOOM_GATEWAY_MODEL".to_owned(), "test-model".to_owned());
    values.insert("LOOM_GATEWAY_TIMEOUT_SECS".to_owned(), "301".to_owned());

    let error = ai_gateway::AiGatewayConfig::from_lookup(|name| values.get(name).cloned())
        .expect_err("invalid timeout must be rejected");

    assert_eq!(
        error,
        ai_gateway::AiGatewayConfigError::TimeoutOutOfRange(301)
    );
}

#[test]
fn daemon_ignores_empty_probe_before_serving_real_request() {
    let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
    let address = daemon.local_addr().expect("local address");
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

    TcpStream::connect(("127.0.0.1", address.port())).expect("empty probe");
    let health = http_get(address.port(), "/health");

    assert!(health.contains("200 OK"));
    assert!(health.contains("\"status\":\"ok\""));

    shutdown_tx.send(()).expect("shutdown");
    server.join().expect("server thread");
}

#[test]
fn live_session_event_long_polls_do_not_starve_request_workers() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let root = unique_temp_dir("live-long-poll-executor-isolation");
    let daemon = LoomDaemon::bind(
        DaemonConfig::localhost(0)
            .with_bounded_request_executor(1, 2)
            .with_control_plane_root(&root),
    )
    .expect("bind daemon");
    let port = daemon.local_addr().expect("address").port();
    let live_sessions = Arc::clone(&daemon.runtime.live_sessions);
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let server = thread::spawn(move || daemon.serve_until(shutdown_rx));

    let (device_id, device_token) = wall_http::pair(port, "Live poll source");
    let session_id = "live:long-poll".to_owned();
    let mut start = live_start_envelope(&session_id);
    if let LiveControlMessage::SessionStart(message) = &mut start.message {
        message.session.source_device_id = device_id.clone();
        message.requested_by_device_id = device_id.clone();
    }
    live_sessions
        .create(&device_id, start)
        .expect("create live session");
    let after = live_sessions
        .events_after(&session_id, 0)
        .unwrap()
        .1
        .last()
        .expect("initial session event")
        .sequence;

    let s1 = session_id.clone();
    let t1 = device_token.clone();
    let poll_one = thread::spawn(move || {
        wall_http::device(
            port,
            &t1,
            "GET",
            &format!("/v1/live/sessions/{s1}/events?after={after}&timeoutMs=3000"),
            None,
        )
        .0
    });
    let s2 = session_id;
    let t2 = device_token;
    let poll_two = thread::spawn(move || {
        wall_http::device(
            port,
            &t2,
            "GET",
            &format!("/v1/live/sessions/{s2}/events?after={after}&timeoutMs=3000"),
            None,
        )
        .0
    });
    let deadline = Instant::now() + Duration::from_secs(2);
    while live_sessions.waiting_event_polls.load(Ordering::SeqCst) < 2 {
        assert!(Instant::now() < deadline, "both long-polls must enter wait");
        thread::yield_now();
    }

    let started = Instant::now();
    let workflows = http_get(port, "/v1/workflows");
    let waited = started.elapsed();
    assert_eq!(
        live_sessions.waiting_event_polls.load(Ordering::SeqCst),
        2,
        "ordinary request must finish while both long-polls are still waiting"
    );

    let r1 = poll_one.join().expect("poll one");
    let r2 = poll_two.join().expect("poll two");
    shutdown_tx.send(()).expect("shutdown");
    server.join().expect("server").expect("serve");
    let _ = fs::remove_dir_all(root);

    assert!(workflows.starts_with("HTTP/1.1 200 OK"));
    assert_eq!(r1, 200);
    assert_eq!(r2, 200);
    assert!(
        waited < Duration::from_millis(600),
        "ordinary request waited {waited:?} behind parked Live event long-polls"
    );
}

#[test]
fn ai_gateway_config_hot_reloads_and_persists_token_privately() {
    let root = unique_temp_dir("ai-gateway-hot-reload");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

    let updated = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request(
                "PUT",
                "/v1/ai/config",
                &[],
                Some(
                    r#"{
                        "enabled": true,
                        "baseUrl": "http://192.168.1.88:4200",
                        "defaultModel": "qwen-lan",
                        "timeoutSeconds": 25,
                        "authToken": "lan-gateway-secret-token"
                    }"#,
                ),
            ),
        ),
        200,
    );
    assert_eq!(updated["configured"], true);
    assert_eq!(updated["origin_kind"], "lan");
    assert_eq!(updated["model"], "qwen-lan");
    assert_eq!(updated["has_token"], true);
    assert!(!updated.to_string().contains("lan-gateway-secret-token"));

    let persisted_settings =
        fs::read_to_string(root.join("settings").join("ai-gateway.json")).expect("settings json");
    assert!(!persisted_settings.contains("lan-gateway-secret-token"));

    drop(runtime);
    let reloaded = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
    let status = expect_json_text_route_response(
        route_request(
            &reloaded,
            &parsed_request("GET", "/v1/ai/status", &[], None),
        ),
        200,
    );
    assert_eq!(status["configured"], true);
    assert_eq!(status["origin_kind"], "lan");
    assert_eq!(status["has_token"], true);
    drop(reloaded);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn daemon_lists_core_capabilities() {
    let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
    let address = daemon.local_addr().expect("local address");
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

    let response = http_json_get(address.port(), "/v1/capabilities");

    assert_eq!(response["capabilities"][0]["id"], "ai.chat.v1");
    assert_eq!(response["capabilities"][0]["mode"], "run");
    let capability_ids = response["capabilities"]
        .as_array()
        .expect("capabilities")
        .iter()
        .map(|capability| capability["id"].as_str().expect("capability id").to_owned())
        .collect::<Vec<_>>();
    assert!(capability_ids.contains(&"ai.chat.v1".to_owned()));
    assert!(capability_ids.contains(&"loom.runtime.probe.v1".to_owned()));

    shutdown_tx.send(()).expect("shutdown");
    server.join().expect("server thread");
}

#[test]
fn daemon_reads_and_writes_mcp_servers() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let root = unique_temp_dir("control-plane-mcp");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

    let empty = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request("GET", "/v1/mcp/servers", &[], None),
        ),
        200,
    );
    assert_eq!(empty["servers"], serde_json::json!([]));

    let saved = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request(
                "PUT",
                "/v1/mcp/servers/brave",
                &[],
                Some(
                    r#"{
              "id": "brave",
              "name": "Brave Search",
              "command": "npx",
              "args": ["-y", "@brave/brave-search-mcp-server"],
              "env": { "BRAVE_API_KEY": "test-key" },
              "transport": "stdio",
              "enabled": true
            }"#,
                ),
            ),
        ),
        200,
    );
    assert_eq!(saved["server"]["id"], "brave");
    assert_eq!(saved["server"]["name"], "Brave Search");
    assert_eq!(saved["server"]["args"][1], "@brave/brave-search-mcp-server");

    let listed = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request("GET", "/v1/mcp/servers", &[], None),
        ),
        200,
    );
    assert_eq!(listed["servers"].as_array().expect("servers").len(), 1);
    assert_eq!(listed["servers"][0]["id"], "brave");

    let deleted = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request("DELETE", "/v1/mcp/servers/brave", &[], None),
        ),
        200,
    );
    assert_eq!(deleted["deleted"], true);
    let listed_after_delete = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request("GET", "/v1/mcp/servers", &[], None),
        ),
        200,
    );
    assert_eq!(
        listed_after_delete["servers"]
            .as_array()
            .expect("servers")
            .len(),
        0
    );

    drop(runtime);
    let _ = fs::remove_dir_all(root);
}
