//! Security and resource-boundary regressions added after the module split.

use super::*;

#[test]
fn remote_policy_errors_do_not_expose_url_paths_or_queries() {
    let _guard = ProcessConfigTestGuard::capture();
    configure_local_servers(false);
    let secret = "query-secret-value";
    let config = McpServerConfig::remote(
        "private",
        "Private",
        format!("https://127.0.0.1/private/{secret}?access_token={secret}"),
    );

    let error = match StreamableHttpMcpClient::connect(&config) {
        Ok(_) => panic!("private endpoint must be rejected"),
        Err(error) => error,
    };
    let message = error.to_string();

    assert!(message.contains("https://127.0.0.1"));
    assert!(!message.contains(secret));
    assert!(!message.contains("access_token"));
}

#[test]
fn http_error_body_redacts_configured_authorization_values() {
    let token = "fixture-http-secret";
    let values = vec![format!("Bearer {token}")];
    let sensitive = collect_sensitive_values(values.iter());
    let body = format!("request rejected; Authorization=Bearer {token}; token={token}");

    let diagnostic = bounded_error_body(body.as_bytes(), &sensitive);

    assert!(!diagnostic.contains(token));
    assert!(diagnostic.contains("[REDACTED_SECRET]"));
}

#[test]
fn stdio_error_redacts_configured_environment_values() {
    let _guard = ProcessConfigTestGuard::capture();
    let secret = "fixture-stdio-secret";
    let config = current_test_binary_fixture_config()
        .env("LOOM_MCP_FIXTURE_MODE", "stderr-secret")
        .env("FIXTURE_SECRET", secret);
    let mut client = StdioMcpClient::spawn(&config).expect("spawn secret stderr fixture");

    let error = client
        .initialize()
        .expect_err("secret stderr fixture must exit");
    let message = error.to_string();

    assert!(!message.contains(secret));
    assert!(message.contains("[REDACTED_SECRET]"));
}

#[test]
fn zero_stdio_timeout_is_clamped_to_one_millisecond() {
    let _guard = ProcessConfigTestGuard::capture();
    let config = current_test_binary_fixture_config().env("LOOM_MCP_FIXTURE_MODE", "hang");
    let mut client = StdioMcpClient::spawn_with_timeout(&config, Duration::ZERO)
        .expect("spawn zero-timeout fixture");

    let error = client
        .initialize()
        .expect_err("zero timeout must remain bounded");

    assert!(matches!(error, McpError::Timeout { timeout_ms: 1, .. }));
}

#[test]
fn malformed_stdio_message_flood_is_rejected_before_timeout() {
    let _guard = ProcessConfigTestGuard::capture();
    let config =
        current_test_binary_fixture_config().env("LOOM_MCP_FIXTURE_MODE", "invalid-json-flood");
    let mut client = StdioMcpClient::spawn_with_timeout(&config, Duration::from_secs(5))
        .expect("spawn malformed-message fixture");

    let error = client
        .initialize()
        .expect_err("malformed message flood must be rejected");

    assert!(matches!(error, McpError::Protocol(message) if message.contains("malformed JSON")));
}

#[test]
fn http_message_arrays_have_a_count_ceiling() {
    let body = serde_json::to_vec(&vec![JsonValue::Null; MCP_MAX_HTTP_MESSAGES + 1])
        .expect("serialize oversized message array");

    let error = parse_json_messages(&body).expect_err("message array must be bounded");

    assert!(matches!(error, McpError::Protocol(message) if message.contains("messages")));
}

#[test]
fn transport_clients_validate_tool_names_before_sending() {
    let error = validate_tool_call_payload("invalid tool name", &serde_json::json!({}))
        .expect_err("invalid tool name must be refused");

    assert!(matches!(error, McpError::InvalidConfig(_)));
}
