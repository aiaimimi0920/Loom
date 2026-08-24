// Loom daemon tests fragment 22; included into the shared crate test module.
#[test]
fn daemon_writes_local_capability_manifest_when_configured() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let temp_dir = unique_temp_dir("manifest");
    let manifest_dir = temp_dir.join("capabilities");
    let control_plane_root = temp_dir.join("control-plane");
    let previous_token = std::env::var("LOOM_DAEMON_TOKEN").ok();
    std::env::remove_var("LOOM_DAEMON_TOKEN");
    let mut config = DaemonConfig::localhost(0)
        .with_manifest_dir(&manifest_dir)
        .with_control_plane_root(&control_plane_root);
    config.auth_token = None;
    let daemon = LoomDaemon::bind(config).expect("bind daemon");
    let address = daemon.local_addr().expect("local address");

    let manifest_path = manifest_dir.join("loom.json");
    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path).expect("read loom manifest"))
            .expect("valid loom manifest json");

    assert_eq!(manifest["schemaVersion"], 1);
    assert_eq!(manifest["appId"], "loom");
    assert_eq!(manifest["displayName"], "Loom");
    assert_eq!(manifest["version"], loom_core::LOOM_VERSION);
    assert!(manifest["pid"].as_u64().expect("pid") > 0);
    assert_eq!(manifest["transport"]["type"], "http");
    assert_eq!(
        manifest["transport"]["baseUrl"],
        format!("http://127.0.0.1:{}", address.port())
    );
    assert_eq!(manifest["transport"]["auth"], "bearer");
    let generated_token = manifest["transport"]["authToken"]
        .as_str()
        .expect("generated manifest auth token");
    assert_eq!(generated_token.len(), 43);
    assert_eq!(
        fs::read_to_string(control_plane_root.join(DAEMON_AUTH_TOKEN_FILE))
            .expect("read generated daemon token"),
        generated_token
    );
    assert!(manifest["capabilities"]
        .as_array()
        .expect("capabilities")
        .contains(&serde_json::Value::String("brain.plan".to_owned())));
    assert!(manifest["capabilities"]
        .as_array()
        .expect("capabilities")
        .contains(&serde_json::Value::String(
            "tea.ticket.decompose.v1".to_owned()
        )));
    assert!(manifest["capabilities"]
        .as_array()
        .expect("capabilities")
        .contains(&serde_json::Value::String(
            "tea.ticket.execute.v1".to_owned()
        )));
    assert!(manifest["capabilities"]
        .as_array()
        .expect("capabilities")
        .contains(&serde_json::Value::String(
            "tea.ticket.review.v1".to_owned()
        )));
    assert!(manifest["startedAt"].as_u64().is_some() || manifest["startedAt"].is_string());
    assert_eq!(
        fs::read_dir(&manifest_dir)
            .expect("list manifest directory")
            .filter_map(std::result::Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp-"))
            .count(),
        0
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&manifest_dir)
                .expect("manifest directory metadata")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&manifest_path)
                .expect("manifest metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    restore_env("LOOM_DAEMON_TOKEN", previous_token);
}

#[test]
fn local_capability_manifest_atomic_write_replaces_existing_token() {
    let temp_dir = unique_temp_dir("manifest-atomic-replace");
    let manifest_dir = temp_dir.join("capabilities");
    let address = "127.0.0.1:38191".parse().expect("address");
    write_local_capability_manifest(&manifest_dir, address, Some("token-one"))
        .expect("write first manifest");
    write_local_capability_manifest(&manifest_dir, address, Some("token-two"))
        .expect("replace manifest");

    let manifest: serde_json::Value = serde_json::from_slice(
        &fs::read(manifest_dir.join("loom.json")).expect("read replaced manifest"),
    )
    .expect("parse replaced manifest");
    assert_eq!(manifest["transport"]["authToken"], "token-two");
    assert_eq!(
        fs::read_dir(&manifest_dir)
            .expect("list manifest directory")
            .filter_map(std::result::Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp-"))
            .count(),
        0
    );
}

#[test]
fn loopback_daemon_survives_an_unwritable_discovery_manifest() {
    handle_capability_manifest_error(
        "127.0.0.1:38191".parse().expect("loopback address"),
        anyhow::anyhow!("simulated access denied"),
    )
    .expect("loopback manifest failure is non-fatal");

    let error = handle_capability_manifest_error(
        "0.0.0.0:38191".parse().expect("non-loopback address"),
        anyhow::anyhow!("simulated access denied"),
    )
    .expect_err("non-loopback manifest failure remains fatal");
    assert!(error.to_string().contains("capability manifest"));
}

#[test]
fn daemon_writes_bearer_local_capability_manifest_and_requires_auth_when_configured() {
    let temp_dir = unique_temp_dir("bearer-manifest");
    let manifest_dir = temp_dir.join("capabilities");
    let daemon = LoomDaemon::bind(
        DaemonConfig::localhost(0)
            .with_bearer_token("local-token")
            .with_manifest_dir(&manifest_dir),
    )
    .expect("bind tokenized daemon");
    let address = daemon.local_addr().expect("local address");

    let manifest_path = manifest_dir.join("loom.json");
    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path).expect("read loom manifest"))
            .expect("valid loom manifest json");
    assert_eq!(manifest["transport"]["type"], "http");
    assert_eq!(
        manifest["transport"]["baseUrl"],
        format!("http://127.0.0.1:{}", address.port())
    );
    assert_eq!(manifest["transport"]["auth"], "bearer");
    assert_eq!(manifest["transport"]["authToken"], "local-token");
    assert!(manifest["capabilities"]
        .as_array()
        .expect("capabilities")
        .contains(&serde_json::Value::String("brain.plan".to_owned())));
    assert!(manifest["capabilities"]
        .as_array()
        .expect("capabilities")
        .contains(&serde_json::Value::String(
            "tea.ticket.decompose.v1".to_owned()
        )));

    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

    let public_health = http_get(address.port(), "/health");
    assert!(
        public_health.starts_with("HTTP/1.1 200 OK"),
        "public_health={public_health}"
    );
    let public_health_body = response_json_body(&public_health);
    assert!(public_health_body.get("pid").is_none());
    assert!(public_health_body.get("executablePath").is_none());

    let unauthorized_status = http_get_without_auth(address.port(), "/status");
    assert!(
        unauthorized_status.starts_with("HTTP/1.1 401 Unauthorized"),
        "unauthorized_status={unauthorized_status}"
    );
    let authorized_status =
        http_request_with_bearer(address.port(), "GET", "/status", None, "local-token");
    assert!(
        authorized_status.starts_with("HTTP/1.1 200 OK"),
        "authorized_status={authorized_status}"
    );
    let authorized_status_body = response_json_body(&authorized_status);
    assert_eq!(authorized_status_body["pid"], std::process::id());
    assert!(authorized_status_body["executablePath"]
        .as_str()
        .is_some_and(|path| !path.is_empty()));

    let unauthorized_capabilities = http_get_without_auth(address.port(), "/v1/capabilities");
    assert!(
        unauthorized_capabilities.starts_with("HTTP/1.1 401 Unauthorized"),
        "unauthorized_capabilities={unauthorized_capabilities}"
    );

    let invoke_body = r#"{
            "requestId":"loom-bearer-manifest-1",
            "caller":"hook",
            "capability":"brain.plan",
            "input":{"goal":"token protected manifest"}
        }"#;
    let unauthorized_invoke =
        http_request_without_auth(address.port(), "POST", "/v1/invoke", Some(invoke_body));
    assert!(
        unauthorized_invoke.starts_with("HTTP/1.1 401 Unauthorized"),
        "unauthorized_invoke={unauthorized_invoke}"
    );

    let authorized_invoke = http_request_with_bearer(
        address.port(),
        "POST",
        "/v1/invoke",
        Some(invoke_body),
        "local-token",
    );
    assert!(
        authorized_invoke.starts_with("HTTP/1.1 200 OK"),
        "authorized_invoke={authorized_invoke}"
    );
    let authorized_body = response_json_body(&authorized_invoke);
    assert_eq!(authorized_body["requestId"], "loom-bearer-manifest-1");
    assert_eq!(authorized_body["status"], "succeeded");
    assert!(authorized_body["output"]["runId"].as_str().is_some());

    shutdown_tx.send(()).expect("shutdown");
    server.join().expect("server thread");
}

#[test]
fn root_contract_loom_manifest_fixture_matches_current_invokable_capabilities() {
    let fixture: serde_json::Value =
        serde_json::from_str(&shared_local_capability_example("loom-manifest.json"))
            .expect("root contract Loom manifest fixture json");
    let capabilities = fixture["capabilities"]
        .as_array()
        .expect("fixture capabilities")
        .iter()
        .map(|capability| capability.as_str().expect("capability string").to_owned())
        .collect::<Vec<_>>();

    assert_eq!(fixture["schemaVersion"], 1);
    assert_eq!(fixture["appId"], "loom");
    assert_eq!(fixture["displayName"], "Loom");
    assert_eq!(fixture["transport"]["type"], "http");
    assert_eq!(fixture["transport"]["auth"], "none");
    assert_eq!(
        capabilities,
        vec![
            "brain.plan".to_owned(),
            "tea.ticket.decompose.v1".to_owned(),
            "tea.ticket.execute.v1".to_owned(),
            "tea.ticket.review.v1".to_owned(),
        ]
    );
}

#[test]
fn daemon_reports_default_tea_configuration_claim() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let previous_apps = std::env::var("LOOM_MANAGED_CONFIG_APPS").ok();
    let previous_base = std::env::var("LOOM_SETTINGS_BASE_URL").ok();
    std::env::remove_var("LOOM_MANAGED_CONFIG_APPS");
    std::env::remove_var("LOOM_SETTINGS_BASE_URL");
    let root = unique_temp_dir("claims-tea-default");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
    let response = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request("GET", "/v1/configuration/claims?app=tea", &[], None),
        ),
        200,
    );

    assert_eq!(response["app"], "tea");
    assert_eq!(response["managed"], false);
    assert_eq!(response["panel_url"], serde_json::Value::Null);

    restore_env("LOOM_MANAGED_CONFIG_APPS", previous_apps);
    restore_env("LOOM_SETTINGS_BASE_URL", previous_base);
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn daemon_reports_managed_tea_configuration_claim_from_env() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let previous_apps = std::env::var("LOOM_MANAGED_CONFIG_APPS").ok();
    let previous_base = std::env::var("LOOM_SETTINGS_BASE_URL").ok();
    std::env::set_var("LOOM_MANAGED_CONFIG_APPS", "hook,tea,talk");
    std::env::set_var("LOOM_SETTINGS_BASE_URL", "http://127.0.0.1:8765/settings");

    let root = unique_temp_dir("claims-tea-managed");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
    let response = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request("GET", "/v1/configuration/claims?app=tea", &[], None),
        ),
        200,
    );

    assert_eq!(response["app"], "tea");
    assert_eq!(response["managed"], true);
    assert_eq!(response["panel_url"], "http://127.0.0.1:8765/settings/tea");

    restore_env("LOOM_MANAGED_CONFIG_APPS", previous_apps);
    restore_env("LOOM_SETTINGS_BASE_URL", previous_base);
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn daemon_claim_response_includes_owner_source_and_schema_version() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let previous_apps = std::env::var("LOOM_MANAGED_CONFIG_APPS").ok();
    let previous_base = std::env::var("LOOM_SETTINGS_BASE_URL").ok();
    std::env::set_var("LOOM_MANAGED_CONFIG_APPS", "tea");
    std::env::set_var("LOOM_SETTINGS_BASE_URL", "http://127.0.0.1:8765/settings");
    let root = unique_temp_dir("claims-tea-owner-source");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
    let response = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request("GET", "/v1/configuration/claims?app=tea", &[], None),
        ),
        200,
    );

    assert_eq!(response["app"], "tea");
    assert_eq!(response["managed"], true);
    assert_eq!(response["owner"], "loom");
    assert_eq!(response["source"], "loom-managed");
    assert_eq!(response["schema_version"], 1);
    assert_eq!(response["panel_url"], "http://127.0.0.1:8765/settings/tea");

    restore_env("LOOM_MANAGED_CONFIG_APPS", previous_apps);
    restore_env("LOOM_SETTINGS_BASE_URL", previous_base);
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn daemon_configuration_api_reads_writes_and_rejects_stale_revisions() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let previous_apps = std::env::var("LOOM_MANAGED_CONFIG_APPS").ok();
    let root = std::env::temp_dir().join(format!("loom-daemon-config-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::env::set_var("LOOM_MANAGED_CONFIG_APPS", "tea");
    let control_plane_root = unique_temp_dir("configuration-api-control-plane");
    let runtime = test_daemon_runtime_from_config(
        &control_plane_root,
        DaemonConfig::localhost(0).with_configuration_root(&root),
    );

    let first = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request("GET", "/v1/configuration/apps/tea", &[], None),
        ),
        200,
    );
    assert_eq!(first["source"], "loom-managed");
    assert_eq!(first["created"], true);
    assert_eq!(first["document"]["revision"], 1);

    let write = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request(
                "PUT",
                "/v1/configuration/apps/tea",
                &[],
                Some(
                    r#"{
              "expected_revision": 1,
              "config": {
                "notifications_enabled": false,
                "human_ticket_default_approval_policy": "manual_only",
                "hook_ticket_default_approval_policy": "plan_only"
              }
            }"#,
                ),
            ),
        ),
        200,
    );
    assert_eq!(write["ok"], true);
    assert_eq!(write["document"]["revision"], 2);

    let stale = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request(
                "PUT",
                "/v1/configuration/apps/tea",
                &[],
                Some(r#"{"expected_revision":1,"config":{"notifications_enabled":true}}"#),
            ),
        ),
        409,
    );
    assert_eq!(stale["error"]["code"], "revision_conflict");

    drop(runtime);
    fs::remove_dir_all(control_plane_root).expect("cleanup control plane");
    let _ = std::fs::remove_dir_all(&root);
    restore_env("LOOM_MANAGED_CONFIG_APPS", previous_apps);
}
