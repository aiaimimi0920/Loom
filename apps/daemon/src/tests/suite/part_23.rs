// Loom daemon tests fragment 23; included into the shared crate test module.
#[test]
fn daemon_settings_pages_render_real_html() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let previous_apps = std::env::var("LOOM_MANAGED_CONFIG_APPS").ok();
    let root =
        std::env::temp_dir().join(format!("loom-daemon-settings-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::env::set_var("LOOM_MANAGED_CONFIG_APPS", "tea,hook");
    let control_plane_root = unique_temp_dir("settings-pages-control-plane");
    let runtime = test_daemon_runtime_from_config(
        &control_plane_root,
        DaemonConfig::localhost(0).with_configuration_root(&root),
    );

    let index_body = expect_text_route_response(
        route_request(&runtime, &parsed_request("GET", "/settings", &[], None)),
        200,
    );
    let mut index_http = Vec::new();
    write_response(&mut index_http, 200, &index_body).expect("write settings index");
    let index = String::from_utf8(index_http).expect("settings index http");
    assert!(
        index.contains("Content-Type: text/html; charset=utf-8"),
        "index should be served as browser-renderable HTML: {index}"
    );
    assert!(index.contains("Loom Settings"));
    assert!(index.contains("/settings/tea"));

    let tea_body = expect_text_route_response(
        route_request(&runtime, &parsed_request("GET", "/settings/tea", &[], None)),
        200,
    );
    let mut tea_http = Vec::new();
    write_response(&mut tea_http, 200, &tea_body).expect("write tea settings");
    let tea = String::from_utf8(tea_http).expect("tea settings http");
    assert!(
        tea.contains("Content-Type: text/html; charset=utf-8"),
        "app settings should be served as browser-renderable HTML: {tea}"
    );
    assert!(tea.contains("Tea Settings"));
    assert!(tea.contains("expected_revision"));

    drop(runtime);
    let _ = std::fs::remove_dir_all(&control_plane_root);
    let _ = std::fs::remove_dir_all(&root);
    restore_env("LOOM_MANAGED_CONFIG_APPS", previous_apps);
}

#[test]
fn daemon_invokes_brain_plan_and_serves_run_and_events() {
    let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
    let address = daemon.local_addr().expect("local address");
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

    let invoke = http_json_post(
        address.port(),
        "/v1/invoke",
        r#"{
                "requestId": "loom-request-1",
                "caller": "hook",
                "capability": "brain.plan",
                "input": {
                    "goal": "Plan Loom capability API tests",
                    "constraints": ["preserve Tea run contract"]
                }
            }"#,
    );

    assert_eq!(invoke["requestId"], "loom-request-1");
    assert_eq!(invoke["status"], "succeeded");
    assert_eq!(invoke["output"]["run"]["capability"], "brain.plan");
    assert_eq!(
        invoke["output"]["run"]["input"]["goal"],
        "Plan Loom capability API tests"
    );
    assert!(invoke["output"]["summary"]
        .as_str()
        .expect("summary")
        .contains("Plan Loom capability API tests"));
    let run_id = invoke["output"]["runId"].as_str().expect("run id");

    let stored_run = http_json_get(address.port(), &format!("/v1/runs/{run_id}"));
    assert_eq!(stored_run, invoke["output"]["run"]);

    let events = http_json_get(address.port(), &format!("/v1/runs/{run_id}/events"));
    assert_eq!(events["run_id"], run_id);
    assert_eq!(events["events"].as_array().expect("events").len(), 2);
    assert_eq!(events["events"][0]["kind"], "run_started");
    assert_eq!(events["events"][0]["run_id"], run_id);
    assert_eq!(events["events"][1]["kind"], "capability_completed");
    assert_eq!(events["events"][1]["run_id"], run_id);
    assert_eq!(events["events"][1]["planner"]["source"], "local_template");
    assert_eq!(invoke["output"]["planner"]["source"], "local_template");

    shutdown_tx.send(()).expect("shutdown");
    server.join().expect("server thread");
}

#[test]
fn daemon_invokes_gateway_brain_plan_and_forwards_input() {
    let plan_content = serde_json::json!({
        "summary": "Gateway plan",
        "steps": ["inspect", "execute"]
    })
    .to_string();
    let fixture = GatewayBrainPlanFixture::start(
        "200 OK",
        serde_json::json!({
            "model": "resolved-model",
            "choices": [{
                "message": { "content": plan_content }
            }]
        })
        .to_string(),
    );
    let daemon = LoomDaemon::bind(DaemonConfig::localhost(0).with_brain_planner(
        brain_plan::BrainPlannerConfig::Gateway(brain_plan::GatewayPlannerConfig {
            base_url: fixture.base_url(),
            auth_token: Some("test-token".to_owned()),
            model: "planner-model".to_owned(),
            timeout: Duration::from_secs(5),
        }),
    ))
    .expect("bind daemon");
    let address = daemon.local_addr().expect("local address");
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

    let invoke = http_json_post(
        address.port(),
        "/v1/invoke",
        &serde_json::json!({
            "requestId": "loom-request-gateway-success",
            "caller": "hook",
            "capability": "brain.plan",
            "input": {
                "goal": "Plan Gateway-backed smoke",
                "constraints": ["preserve run contract", 42, "  "],
                "context": {"release": "candidate-2"}
            }
        })
        .to_string(),
    );

    assert_eq!(invoke["status"], "succeeded");
    assert_eq!(invoke["output"]["summary"], "Gateway plan");
    assert_eq!(
        invoke["output"]["steps"],
        serde_json::json!(["inspect", "execute"])
    );
    assert_eq!(invoke["output"]["planner"]["source"], "gateway");
    assert_eq!(invoke["output"]["planner"]["model"], "resolved-model");
    assert_eq!(invoke["output"]["run"]["status"], "succeeded");

    let run_id = invoke["output"]["runId"].as_str().expect("run id");
    let stored_run = http_json_get(address.port(), &format!("/v1/runs/{run_id}"));
    assert_eq!(stored_run["status"], "succeeded");
    assert_eq!(stored_run["output"]["planner"]["source"], "gateway");
    let events = http_json_get(address.port(), &format!("/v1/runs/{run_id}/events"));
    assert_eq!(events["events"].as_array().expect("events").len(), 2);
    assert_eq!(events["events"][0]["kind"], "run_started");
    assert_eq!(events["events"][1]["kind"], "capability_completed");
    assert_eq!(events["events"][1]["planner"]["source"], "gateway");

    let gateway_request = fixture.request();
    assert!(gateway_request.starts_with("POST /v1/chat/completions HTTP/1.1"));
    assert!(gateway_request
        .to_ascii_lowercase()
        .contains("authorization: bearer test-token"));
    let gateway_body = gateway_request
        .split_once("\r\n\r\n")
        .expect("Gateway request body")
        .1;
    let gateway_payload: serde_json::Value =
        serde_json::from_str(gateway_body).expect("Gateway request JSON");
    assert_eq!(gateway_payload["model"], "planner-model");
    let user_content = gateway_payload["messages"][1]["content"]
        .as_str()
        .expect("Gateway user content");
    let user_payload: serde_json::Value =
        serde_json::from_str(user_content).expect("Gateway user JSON");
    assert_eq!(user_payload["goal"], "Plan Gateway-backed smoke");
    assert_eq!(
        user_payload["constraints"],
        serde_json::json!(["preserve run contract"])
    );
    assert_eq!(
        user_payload["context"],
        serde_json::json!({"release": "candidate-2"})
    );
    assert_eq!(
        user_payload.as_object().expect("Gateway user object").len(),
        3
    );
    assert!(user_content.contains("Plan Gateway-backed smoke"));
    assert!(!user_content.contains("test-token"));
    assert!(!user_content.contains("LOOM_GATEWAY_BASE_URL"));

    shutdown_tx.send(()).expect("shutdown");
    server.join().expect("server thread");
}

#[test]
fn daemon_records_failed_gateway_brain_plan_with_run_evidence() {
    let fixture = GatewayBrainPlanFixture::start(
        "503 Service Unavailable",
        serde_json::json!({
            "error": {
                "code": "gateway_unavailable",
                "message": format!("fixture Gateway is unavailable {}", "x".repeat(700))
            }
        })
        .to_string(),
    );
    let daemon = LoomDaemon::bind(DaemonConfig::localhost(0).with_brain_planner(
        brain_plan::BrainPlannerConfig::Gateway(brain_plan::GatewayPlannerConfig {
            base_url: fixture.base_url(),
            auth_token: Some("failure-secret".to_owned()),
            model: "failure-model".to_owned(),
            timeout: Duration::from_secs(5),
        }),
    ))
    .expect("bind daemon");
    let address = daemon.local_addr().expect("local address");
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

    let response = http_request(
        address.port(),
        "POST",
        "/v1/invoke",
        Some(
            r#"{
                    "requestId":"loom-request-gateway-failure",
                    "caller":"hook",
                    "capability":"brain.plan",
                    "input":{
                        "goal":"record Gateway failure",
                        "context":{"raw":"must not become diagnostic"}
                    }
                }"#,
        ),
    );

    assert!(
        response.starts_with("HTTP/1.1 502 Bad Gateway"),
        "response={response}"
    );
    let body = response_json_body(&response);
    assert_eq!(body["status"], "failed");
    assert_eq!(body["error"]["code"], "gateway_planner_failed");
    let run_id = body["error"]["runId"].as_str().expect("failed run id");
    assert!(!response.contains("failure-secret"));

    let stored_run = http_json_get(address.port(), &format!("/v1/runs/{run_id}"));
    assert_eq!(stored_run["status"], "failed");
    assert_eq!(stored_run["error"]["code"], "gateway_planner_failed");
    let diagnostic = stored_run["error"]["diagnostic"]
        .as_str()
        .expect("failed run diagnostic");
    assert!(diagnostic.len() <= 512, "diagnostic={diagnostic}");
    assert!(!diagnostic.contains("must not become diagnostic"));
    assert!(!stored_run.to_string().contains("failure-secret"));
    let events = http_json_get(address.port(), &format!("/v1/runs/{run_id}/events"));
    assert_eq!(events["events"].as_array().expect("events").len(), 2);
    assert_eq!(events["events"][0]["kind"], "run_started");
    assert_eq!(events["events"][1]["kind"], "capability_failed");
    assert_eq!(events["events"][1]["planner"]["source"], "gateway");
    assert_eq!(
        events["events"][1]["error"]["code"],
        "gateway_planner_failed"
    );

    shutdown_tx.send(()).expect("shutdown");
    server.join().expect("server thread");
}

#[test]
fn daemon_reads_brain_plan_run_after_restart() {
    let root = unique_temp_dir("run-restart");
    let path = root.join("runs.sqlite3");
    let (port, shutdown_tx, server) =
        start_daemon_with_store(&path, BrainPlannerConfig::LocalTemplate);

    let invoke = http_json_post(
        port,
        "/v1/invoke",
        r#"{"requestId":"persist-1","caller":"hook","capability":"brain.plan","input":{"goal":"survive restart"}}"#,
    );
    let run_id = invoke["output"]["runId"]
        .as_str()
        .expect("run id")
        .to_owned();
    shutdown_tx.send(()).expect("shutdown first daemon");
    server.join().expect("first daemon").expect("first serve");

    let (port, shutdown_tx, server) =
        start_daemon_with_store(&path, BrainPlannerConfig::LocalTemplate);
    let run = http_json_get(port, &format!("/v1/runs/{run_id}"));
    let events = http_json_get(port, &format!("/v1/runs/{run_id}/events"));
    assert_eq!(run["status"], "succeeded");
    assert_eq!(run["input"]["goal"], "survive restart");
    assert_eq!(events["events"][0]["kind"], "run_started");
    assert_eq!(events["events"][1]["kind"], "capability_completed");
    shutdown_tx.send(()).expect("shutdown second daemon");
    server.join().expect("second daemon").expect("second serve");
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn daemon_reads_failed_gateway_run_after_restart() {
    let root = unique_temp_dir("gateway-run-restart");
    let path = root.join("runs.sqlite3");
    let planner = BrainPlannerConfig::Gateway(brain_plan::GatewayPlannerConfig {
        base_url: "http://127.0.0.1:9".to_owned(),
        auth_token: Some("restart-secret".to_owned()),
        model: "restart-model".to_owned(),
        timeout: Duration::from_secs(1),
    });
    let (port, shutdown_tx, server) = start_daemon_with_store(&path, planner);

    let response = http_request(
        port,
        "POST",
        "/v1/invoke",
        Some(
            r#"{"requestId":"persist-gateway-failure","caller":"hook","capability":"brain.plan","input":{"goal":"persist gateway failure"}}"#,
        ),
    );
    assert!(response.starts_with("HTTP/1.1 502 Bad Gateway"));
    let body = response_json_body(&response);
    let run_id = body["error"]["runId"].as_str().expect("failed run id");
    assert!(!response.contains("restart-secret"));
    shutdown_tx.send(()).expect("shutdown first daemon");
    server.join().expect("first daemon").expect("first serve");

    let (port, shutdown_tx, server) =
        start_daemon_with_store(&path, BrainPlannerConfig::LocalTemplate);
    let run = http_json_get(port, &format!("/v1/runs/{run_id}"));
    let events = http_json_get(port, &format!("/v1/runs/{run_id}/events"));
    assert_eq!(run["status"], "failed");
    assert_eq!(run["error"]["code"], "gateway_planner_failed");
    assert!(!run.to_string().contains("restart-secret"));
    assert_eq!(events["events"][1]["kind"], "capability_failed");
    shutdown_tx.send(()).expect("shutdown second daemon");
    server.join().expect("second daemon").expect("second serve");
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn daemon_recovers_preexisting_running_run_after_bind() {
    let root = unique_temp_dir("running-run-recovery");
    let path = root.join("runs.sqlite3");
    {
        let mut store = SqliteRunEvidenceStore::open(&path).expect("open seed store");
        store
            .insert_run(
                json!({
                    "id": "preseeded-running",
                    "capability": "brain.plan",
                    "loom_session_id": "session-preseeded",
                    "status": "running",
                    "input": { "goal": "recover me" }
                }),
                vec![RunEventDraft::new(
                    "run_started",
                    json!({ "capability": "brain.plan", "status": "running" }),
                )
                .expect("start event")],
            )
            .expect("seed running run");
    }

    let (port, shutdown_tx, server) =
        start_daemon_with_store(&path, BrainPlannerConfig::LocalTemplate);
    let run = http_json_get(port, "/v1/runs/preseeded-running");
    let events = http_json_get(port, "/v1/runs/preseeded-running/events");
    assert_eq!(run["status"], "failed");
    assert_eq!(run["error"]["code"], "daemon_restarted");
    assert_eq!(run["input"]["goal"], "recover me");
    assert_eq!(events["events"].as_array().expect("events").len(), 2);
    assert_eq!(events["events"][1]["kind"], "run_interrupted");
    assert_eq!(events["events"][1]["error"]["code"], "daemon_restarted");
    shutdown_tx.send(()).expect("shutdown daemon");
    server.join().expect("daemon").expect("serve");
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn run_store_failure_returns_safe_http_error_without_stopping_daemon() {
    let mut daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
    Arc::get_mut(&mut daemon.runtime)
        .expect("exclusive daemon runtime")
        .run_store = Arc::new(Mutex::new(Box::new(FailingRunEvidenceStore)));
    let address = daemon.local_addr().expect("local address");
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let server = thread::spawn(move || daemon.serve_until(shutdown_rx));

    let response = http_request(
        address.port(),
        "POST",
        "/v1/invoke",
        Some(
            r#"{"requestId":"run-store-failure","caller":"hook","capability":"brain.plan","input":{"goal":"fail store"}}"#,
        ),
    );
    assert!(response.starts_with("HTTP/1.1 500 Internal Server Error"));
    let body = response_json_body(&response);
    assert_eq!(body["error"]["code"], "run_store_failed");
    assert!(!body.to_string().contains("fixture failure"));

    let health = http_request(address.port(), "GET", "/health", None);
    assert!(health.starts_with("HTTP/1.1 200 OK"));
    shutdown_tx.send(()).expect("shutdown");
    server.join().expect("server").expect("serve");
}
