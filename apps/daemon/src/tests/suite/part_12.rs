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

    assert!(serialized.starts_with("HTTP/1.1 200 OK"), "{serialized}");
    assert!(polled.starts_with("HTTP/1.1 200 OK"), "{polled}");
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

    assert!(health.starts_with("HTTP/1.1 200 OK"), "{health}");
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
        ("GET", "/v1/hook-bridge/canvas", None),
        ("GET", "/v1/hook-bridge/canvas/nodes/capture/preview", None),
        ("GET", "/v1/surfaces/stream", None),
        ("GET", "/v1/surfaces/stream?since=7", None),
        ("GET", "/v1/runs/run-1", None),
        ("GET", "/v1/runs/run-1/events", None),
        ("POST", "/v1/invoke", Some("brain.plan")),
        ("POST", "/v1/invoke", Some("tea.ticket.decompose.v1")),
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
        ("POST", "/v1/tools/tool-1/execute", None),
        ("POST", "/v1/invoke", Some("future.capability")),
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
fn daemon_reports_configured_gateway_brain_planner_without_auth_token() {
    let config = DaemonConfig::localhost(0).with_brain_planner(
        brain_plan::BrainPlannerConfig::Gateway(brain_plan::GatewayPlannerConfig {
            base_url: "http://127.0.0.1:4200".to_owned(),
            auth_token: Some("do-not-expose".to_owned()),
            model: "test-model".to_owned(),
            timeout: Duration::from_secs(12),
        }),
    );
    let root = unique_temp_dir("status-gateway-brain-planner");
    let runtime = test_daemon_runtime_from_config(&root, config);
    let status = expect_json_text_route_response(
        route_request(&runtime, &parsed_request("GET", "/status", &[], None)),
        200,
    );

    assert_eq!(status["brain_planner"]["mode"], "gateway");
    assert_eq!(status["brain_planner"]["configured"], true);
    assert_eq!(status["brain_planner"]["model"], "test-model");
    assert_eq!(status["brain_planner"]["timeout_seconds"], 12);
    assert!(status["brain_planner"].get("auth_token").is_none());
    assert!(!status.to_string().contains("do-not-expose"));
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn daemon_rejects_invalid_gateway_timeout_configuration() {
    let mut values = std::collections::HashMap::new();
    values.insert("LOOM_GATEWAY_MODEL".to_owned(), "test-model".to_owned());
    values.insert("LOOM_GATEWAY_TIMEOUT_SECS".to_owned(), "301".to_owned());

    let error = brain_plan::BrainPlannerConfig::from_lookup(|name| values.get(name).cloned())
        .expect_err("invalid timeout must be rejected");

    assert_eq!(
        error,
        brain_plan::BrainPlannerConfigError::TimeoutOutOfRange(301)
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
fn status_reports_hook_settings_summary() {
    let hook_settings = loom_hooks::HookSettings::enabled(vec![
        loom_hooks::HookRule::new(loom_hooks::HookEventKind::RunStarted)
            .with_target(loom_hooks::HookTarget::memory("runs")),
        loom_hooks::HookRule::new(loom_hooks::HookEventKind::RunStopped)
            .with_target(loom_hooks::HookTarget::memory("finished")),
    ]);
    let root = unique_temp_dir("status-hook-settings-summary");
    let runtime = test_daemon_runtime_from_config(
        &root,
        DaemonConfig::localhost(0).with_hook_settings(hook_settings),
    );
    let status = expect_json_text_route_response(
        route_request(&runtime, &parsed_request("GET", "/status", &[], None)),
        200,
    );

    assert_eq!(status["hooks"]["enabled"], true);
    assert_eq!(status["hooks"]["ruleCount"], 2);
    assert_eq!(status["hooks"]["targetCount"], 2);
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn daemon_serves_tea_run_contract() {
    let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
    let address = daemon.local_addr().expect("local address");
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

    let ticket_id = "11111111-1111-4111-8111-111111111111";
    let run_body = http_post(
        address.port(),
        "/v1/runs",
        &format!(
            r#"{{
                    "ticket": {{
                        "id": "{ticket_id}",
                        "title": "Tea integration smoke",
                        "description": "Run a Tea-created work order through Loom."
                    }}
                }}"#
        ),
    );
    assert!(run_body.contains("\"ticket_id\":\"11111111-1111-4111-8111-111111111111\""));
    assert!(run_body.contains("\"status\":\"succeeded\""));
    assert!(run_body.contains("\"loom_session_id\""));
    assert!(run_body.contains("loom daemon run completed"));

    let run: serde_json::Value = serde_json::from_str(&run_body).expect("run json");
    let run_id = run["id"].as_str().expect("run id");

    let stopped = http_post(
        address.port(),
        &format!("/v1/runs/{run_id}/stop"),
        &format!(r#"{{"run":{run_body}}}"#),
    );
    assert!(stopped.contains("\"status\":\"stopped\""));

    let retrying = http_post(
        address.port(),
        &format!("/v1/runs/{run_id}/retry"),
        &format!(r#"{{"run":{run_body}}}"#),
    );
    assert!(retrying.contains("\"status\":\"retrying\""));

    shutdown_tx.send(()).expect("shutdown");
    server.join().expect("server thread");
}

#[test]
fn daemon_lists_brain_plan_capability() {
    let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
    let address = daemon.local_addr().expect("local address");
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

    let response = http_json_get(address.port(), "/v1/capabilities");

    assert_eq!(response["capabilities"][0]["id"], "brain.plan");
    assert_eq!(response["capabilities"][0]["mode"], "run");
    assert!(response["capabilities"][0]["description"]
        .as_str()
        .expect("description")
        .contains("plan"));
    let capability_ids = response["capabilities"]
        .as_array()
        .expect("capabilities")
        .iter()
        .map(|capability| capability["id"].as_str().expect("capability id").to_owned())
        .collect::<Vec<_>>();
    assert!(capability_ids.contains(&"brain.plan".to_owned()));
    assert!(capability_ids.contains(&"tea.ticket.decompose.v1".to_owned()));
    assert!(capability_ids.contains(&"tea.ticket.execute.v1".to_owned()));
    assert!(capability_ids.contains(&"tea.ticket.review.v1".to_owned()));

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
