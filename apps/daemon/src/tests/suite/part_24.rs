// Loom daemon tests fragment 24; included into the shared crate test module.
#[test]
fn daemon_invokes_tea_ticket_decompose_capability() {
    let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
    let address = daemon.local_addr().expect("local address");
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));
    let input: serde_json::Value = serde_json::from_str(&shared_tea_brain_provider_example(
        "decompose-request.example.json",
    ))
    .expect("decompose request fixture json");
    let request = serde_json::json!({
        "requestId": "loom-tea-decompose-1",
        "caller": "tea",
        "capability": "tea.ticket.decompose.v1",
        "input": input
    });

    let invoke = http_json_post(address.port(), "/v1/invoke", &request.to_string());

    assert_eq!(invoke["requestId"], "loom-tea-decompose-1");
    assert_eq!(invoke["status"], "succeeded");
    assert_eq!(
        invoke["output"]["run"]["capability"],
        "tea.ticket.decompose.v1"
    );
    assert_eq!(
        invoke["output"]["proposal"]["analysis"]["intent"],
        "engineering_work_order"
    );
    assert_eq!(
        invoke["output"]["proposal"]["analysis"]["recommended_workflow"],
        "loom.tea_ticket_decompose.v1"
    );
    assert!(
        invoke["output"]["proposal"]["plan"]["steps"]
            .as_array()
            .expect("plan steps")
            .len()
            >= 3
    );
    assert_eq!(invoke["output"]["proposal"]["requires_human_review"], true);
    assert!(invoke["output"]["summary"]
        .as_str()
        .expect("summary")
        .contains("Release smoke matrix includes Tea"));

    shutdown_tx.send(()).expect("shutdown");
    server.join().expect("server thread");
}

#[test]
fn daemon_accepts_root_contract_local_capability_invoke_fixture() {
    let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
    let address = daemon.local_addr().expect("local address");
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));
    let request = shared_local_capability_example("loom-invoke-request.json");

    let response = http_request(address.port(), "POST", "/v1/invoke", Some(&request));

    shutdown_tx.send(()).expect("shutdown");
    server.join().expect("server thread");

    assert!(
        response.starts_with("HTTP/1.1 200 OK"),
        "unexpected response: {response}"
    );
    let invoke = response_json_body(&response);
    assert_eq!(invoke["requestId"], "loom-request-1");
    assert_eq!(invoke["status"], "succeeded");
    assert_eq!(invoke["output"]["run"]["capability"], "brain.plan");
    assert_eq!(invoke["output"]["run"]["input"]["goal"], "release smoke");
    assert_eq!(
        invoke["output"]["run"]["input"]["constraints"][0],
        "Hook Talk Loom"
    );
    assert!(invoke["output"]["summary"]
        .as_str()
        .expect("summary")
        .contains("release smoke"));
}

#[test]
fn daemon_returns_structured_failure_for_unknown_capability() {
    let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
    let address = daemon.local_addr().expect("local address");
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

    let response = http_request(
        address.port(),
        "POST",
        "/v1/invoke",
        Some(
            r#"{"requestId":"loom-request-unknown","caller":"hook","capability":"unknown.tool","input":{}}"#,
        ),
    );

    assert!(response.starts_with("HTTP/1.1 404 Not Found"));
    let body = response_json_body(&response);
    assert_eq!(body["requestId"], "loom-request-unknown");
    assert_eq!(body["status"], "failed");
    assert_eq!(body["error"]["code"], "unknown_capability");
    assert_eq!(body["error"]["capability"], "unknown.tool");
    assert!(body["error"]["message"]
        .as_str()
        .expect("message")
        .contains("unknown.tool"));

    shutdown_tx.send(()).expect("shutdown");
    server.join().expect("server thread");
}

#[test]
fn daemon_returns_structured_errors_for_invalid_invoke_goal_and_missing_route() {
    let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
    let address = daemon.local_addr().expect("local address");
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

    let invalid_json = http_request(address.port(), "POST", "/v1/invoke", Some("{"));
    assert!(invalid_json.starts_with("HTTP/1.1 400 Bad Request"));
    let invalid_json_body = response_json_body(&invalid_json);
    assert_eq!(invalid_json_body["status"], "failed");
    assert_eq!(invalid_json_body["error"]["code"], "invalid_request");

    let missing_goal = http_request(
        address.port(),
        "POST",
        "/v1/invoke",
        Some(
            r#"{"requestId":"loom-request-missing-goal","caller":"hook","capability":"brain.plan","input":{}}"#,
        ),
    );
    assert!(missing_goal.starts_with("HTTP/1.1 400 Bad Request"));
    let missing_goal_body = response_json_body(&missing_goal);
    assert_eq!(missing_goal_body["requestId"], "loom-request-missing-goal");
    assert_eq!(missing_goal_body["status"], "failed");
    assert_eq!(missing_goal_body["error"]["code"], "invalid_input");
    assert!(missing_goal_body["error"]["message"]
        .as_str()
        .expect("message")
        .contains("goal"));

    let not_found = http_request(address.port(), "GET", "/v1/does-not-exist", None);
    assert!(not_found.starts_with("HTTP/1.1 404 Not Found"));
    let not_found_body = response_json_body(&not_found);
    assert_eq!(not_found_body["error"]["code"], "not_found");

    shutdown_tx.send(()).expect("shutdown");
    server.join().expect("server thread");
}

#[test]
fn daemon_validates_stop_and_retry_path_run_ids() {
    let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
    let address = daemon.local_addr().expect("local address");
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

    let invoke = http_json_post(
        address.port(),
        "/v1/invoke",
        r#"{"requestId":"loom-request-run-action","caller":"hook","capability":"brain.plan","input":{"goal":"validate run id"}}"#,
    );
    let run_id = invoke["output"]["runId"].as_str().expect("run id");
    let run = invoke["output"]["run"].clone();
    let mismatched_body = format!(r#"{{"run":{run}}}"#);

    let stop_response = http_request(
        address.port(),
        "POST",
        "/v1/runs/not-the-run/stop",
        Some(&mismatched_body),
    );
    assert!(stop_response.starts_with("HTTP/1.1 400 Bad Request"));
    let stop_error = response_json_body(&stop_response);
    assert_eq!(stop_error["error"]["code"], "run_id_mismatch");
    assert_eq!(stop_error["error"]["path_run_id"], "not-the-run");
    assert_eq!(stop_error["error"]["body_run_id"], run_id);

    let retry_response = http_request(
        address.port(),
        "POST",
        "/v1/runs/not-the-run/retry",
        Some(&mismatched_body),
    );
    assert!(retry_response.starts_with("HTTP/1.1 400 Bad Request"));
    let retry_error = response_json_body(&retry_response);
    assert_eq!(retry_error["error"]["code"], "run_id_mismatch");
    assert_eq!(retry_error["error"]["path_run_id"], "not-the-run");
    assert_eq!(retry_error["error"]["body_run_id"], run_id);

    let unknown_run_body = r#"{"run":{"id":"missing-run","status":"succeeded"}}"#;
    let missing_run_response = http_request(
        address.port(),
        "POST",
        "/v1/runs/missing-run/stop",
        Some(unknown_run_body),
    );
    assert!(missing_run_response.starts_with("HTTP/1.1 404 Not Found"));
    let missing_run_error = response_json_body(&missing_run_response);
    assert_eq!(missing_run_error["error"]["code"], "run_not_found");

    let forged = serde_json::json!({
        "run": {
            "id": run_id,
            "status": "succeeded",
            "input": { "goal": "forged" },
            "output": { "summary": "forged" }
        }
    });
    let stopped = http_json_post(
        address.port(),
        &format!("/v1/runs/{run_id}/stop"),
        &forged.to_string(),
    );
    assert_eq!(stopped["status"], "stopped");
    assert_eq!(stopped["input"]["goal"], "validate run id");
    assert_ne!(stopped["output"]["summary"], "forged");

    let retry_body = format!(r#"{{"run":{stopped}}}"#);
    let retrying = http_json_post(
        address.port(),
        &format!("/v1/runs/{run_id}/retry"),
        &retry_body,
    );
    assert_eq!(retrying["status"], "retrying");

    shutdown_tx.send(()).expect("shutdown");
    server.join().expect("server thread");
}

#[test]
fn daemon_config_supports_non_loopback_bind_host_for_containers() {
    let config = DaemonConfig::bind_host("0.0.0.0", 8765);

    assert_eq!(config.host, "0.0.0.0");
    assert_eq!(config.port, 8765);
}

#[test]
fn daemon_request_gateway_rejects_rebinding_cross_origin_and_simple_posts() {
    let root = unique_temp_dir("request-gateway");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

    let invalid_host = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request("GET", "/status", &[("Host", "attacker.example")], None),
        ),
        400,
    );
    assert_eq!(invalid_host["error"]["code"], "invalid_host");

    let invalid_origin = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request(
                "GET",
                "/status",
                &[("Origin", "https://attacker.example")],
                None,
            ),
        ),
        403,
    );
    assert_eq!(invalid_origin["error"]["code"], "origin_denied");

    let cross_site = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request("GET", "/status", &[("Sec-Fetch-Site", "cross-site")], None),
        ),
        403,
    );
    assert_eq!(cross_site["error"]["code"], "browser_context_denied");

    let simple_post = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request(
                "POST",
                "/v1/invoke",
                &[("Content-Type", "text/plain")],
                Some("{}"),
            ),
        ),
        415,
    );
    assert_eq!(simple_post["error"]["code"], "json_content_type_required");
    fs::remove_dir_all(root).expect("cleanup request gateway root");
}

#[test]
fn settings_navigation_exchanges_query_token_for_strict_http_only_cookie() {
    let root = unique_temp_dir("settings-cookie");
    let runtime = test_daemon_runtime(&root, Some("settings-secret"));

    let exchange = route_request(
        &runtime,
        &parsed_request("GET", "/settings?token=settings-secret", &[], None),
    );
    let headers = match exchange {
        RouteResponse::TextWithHeaders {
            status,
            headers,
            body,
        } => {
            assert_eq!(status, 303);
            assert!(body.is_empty());
            headers
        }
        _ => panic!("expected settings token exchange response"),
    };
    assert!(headers
        .iter()
        .any(|(name, value)| name == "Location" && value == "/settings"));
    let cookie = headers
        .iter()
        .find(|(name, _)| name == "Set-Cookie")
        .map(|(_, value)| value.as_str())
        .expect("settings auth cookie");
    assert!(cookie.contains("loom_admin=settings-secret"));
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("SameSite=Strict"));
    assert!(cookie.contains("Path=/"));

    let page = expect_text_route_response(
        route_request(
            &runtime,
            &parsed_request(
                "GET",
                "/settings",
                &[("Cookie", "loom_admin=settings-secret")],
                None,
            ),
        ),
        200,
    );
    assert!(page.trim_start().starts_with("<!doctype html"));

    let denied = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request(
                "GET",
                "/settings",
                &[
                    ("Cookie", "loom_admin=settings-secret"),
                    ("Origin", "http://127.0.0.1:9999"),
                ],
                None,
            ),
        ),
        403,
    );
    assert_eq!(denied["error"]["code"], "origin_denied");
    fs::remove_dir_all(root).expect("cleanup settings cookie root");
}

#[test]
fn daemon_rejects_discovery_manifest_for_non_loopback_bind_host() {
    let temp_dir = unique_temp_dir("non-loopback-manifest");
    let manifest_dir = temp_dir.join("capabilities");

    let bind_error = match LoomDaemon::bind(
        DaemonConfig::bind_host("0.0.0.0", 0)
            .with_bearer_token("local-token")
            .with_tls_termination(true)
            .with_manifest_dir(&manifest_dir),
    ) {
        Ok(_) => panic!("non-loopback manifest bind should fail"),
        Err(error) => error,
    };

    assert!(bind_error
        .to_string()
        .contains("loom discovery manifest requires a loopback bind host"));
    assert!(
        !manifest_dir.join("loom.json").exists(),
        "non-loopback manifest bind should not write loom.json"
    );
}
