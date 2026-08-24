//! Streamable HTTP transport, lifecycle, and cancellation coverage.

use super::*;

#[test]
pub(super) fn streamable_http_client_initializes_lists_and_calls_tools() {
    let _guard = ProcessConfigTestGuard::capture();
    // The fixture listens on loopback over plain http and the config carries a bearer token,
    // which is exactly the combination the outbound policy refuses by default; a developer
    // running a local MCP server opts in the same way.
    configure_local_servers(true);
    let fixture = StreamableHttpFixture::start();
    let config = McpServerConfig::remote("remote", "Remote MCP", fixture.url())
        .header("Authorization", "Bearer fixture-token");
    let mut client = McpClient::connect(&config).expect("connect HTTP MCP fixture");

    let initialized = client.initialize().expect("initialize HTTP MCP fixture");
    let tools = client.list_tools().expect("list HTTP MCP tools");
    let result = client
        .call_tool("echo", serde_json::json!({ "text": "hello remote" }))
        .expect("call HTTP MCP tool");

    assert_eq!(initialized["serverInfo"]["name"], "loom-http-fixture");
    assert_eq!(tools["tools"][0]["name"], "echo");
    assert_eq!(result["content"][0]["text"], "hello remote");
    fixture.finish();
}

#[test]
pub(super) fn streamable_http_initialize_uses_the_same_protocol_fallback_table() {
    let _guard = ProcessConfigTestGuard::capture();
    configure_local_servers(true);
    let fixture = ProtocolFallbackHttpFixture::start();
    let config = McpServerConfig::remote("remote-fallback", "Remote Fallback", fixture.url());
    let mut client =
        StreamableHttpMcpClient::connect(&config).expect("connect fallback HTTP fixture");

    let initialized = client
        .initialize()
        .expect("fall back to legacy HTTP MCP revision");
    assert_eq!(
        initialized["protocolVersion"],
        MCP_SUPPORTED_PROTOCOL_VERSIONS[1]
    );
    assert_eq!(client.protocol_version, MCP_SUPPORTED_PROTOCOL_VERSIONS[1]);
    client.close().expect("close fallback HTTP session");
    fixture.finish();
}

#[test]
pub(super) fn streamable_http_close_terminates_or_accepts_an_unsupported_session_close() {
    for mode in [SessionCloseMode::Success, SessionCloseMode::Unsupported] {
        let _guard = ProcessConfigTestGuard::capture();
        configure_local_servers(true);
        let fixture = SessionCloseHttpFixture::start(mode);
        let config = McpServerConfig::remote("remote-close", "Remote Close", fixture.url());
        let mut client = StreamableHttpMcpClient::connect(&config).expect("connect close fixture");

        client.initialize().expect("initialize close fixture");
        assert!(client.session_id.is_some());
        client.close().expect("close or accept unsupported close");
        assert!(client.session_id.is_none());
        fixture.finish();
    }
}

#[test]
pub(super) fn streamable_http_session_close_is_cancellable() {
    let _guard = ProcessConfigTestGuard::capture();
    configure_local_servers(true);
    let fixture = SessionCloseHttpFixture::start(SessionCloseMode::Delayed);
    let config = McpServerConfig::remote("remote-close", "Remote Close", fixture.url());
    let mut client = StreamableHttpMcpClient::connect(&config).expect("connect close fixture");
    client.initialize().expect("initialize close fixture");
    let cancellation = Arc::new(AtomicBool::new(false));
    let trigger = Arc::clone(&cancellation);
    let trigger_thread = thread::spawn(move || {
        thread::sleep(Duration::from_millis(50));
        trigger.store(true, Ordering::Release);
    });

    let started = Instant::now();
    let error = client
        .close_cancellable(cancellation.as_ref())
        .expect_err("delayed close must be cancellable");
    let elapsed = started.elapsed();

    trigger_thread
        .join()
        .expect("join close cancellation trigger");
    assert!(matches!(error, McpError::Cancelled));
    assert!(
        elapsed < Duration::from_secs(1),
        "close cancel took {elapsed:?}"
    );
    fixture.finish();
}

#[test]
pub(super) fn live_streamable_http_server_from_official_registry() {
    let _guard = ProcessConfigTestGuard::capture();
    let Some(url) = std::env::var("LOOM_MCP_LIVE_TEST_URL")
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
    else {
        return;
    };
    let config = McpServerConfig::remote("live", "Live MCP", url);
    let mut client = McpClient::connect(&config).expect("connect live HTTP MCP");
    let initialized = client.initialize().expect("initialize live HTTP MCP");
    let tools = client.list_tools().expect("list live HTTP MCP tools");
    assert!(initialized.get("serverInfo").is_some());
    assert!(tools.get("tools").and_then(JsonValue::as_array).is_some());
}
#[test]
pub(super) fn streamable_http_cancels_while_waiting_for_response_headers() {
    assert_delayed_http_request_is_cancellable(DelayedHttpMode::Headers);
}

#[test]
pub(super) fn streamable_http_cancels_while_waiting_for_response_body() {
    assert_delayed_http_request_is_cancellable(DelayedHttpMode::Body);
}

pub(super) fn assert_delayed_http_request_is_cancellable(mode: DelayedHttpMode) {
    let _guard = ProcessConfigTestGuard::capture();
    configure_local_servers(true);
    let fixture = DelayedHttpFixture::start(mode);
    let config = McpServerConfig::remote("delayed", "Delayed MCP", fixture.url());
    let mut client = StreamableHttpMcpClient::connect_with_timeout(&config, Duration::from_secs(5))
        .expect("connect delayed fixture");
    let cancellation = Arc::new(AtomicBool::new(false));
    let trigger = Arc::clone(&cancellation);
    let trigger_thread = thread::spawn(move || {
        thread::sleep(Duration::from_millis(50));
        trigger.store(true, Ordering::Release);
    });

    let started = Instant::now();
    let error = client
        .initialize_cancellable(cancellation.as_ref())
        .expect_err("delayed request must be cancellable");
    let elapsed = started.elapsed();

    trigger_thread.join().expect("join cancellation trigger");
    assert!(matches!(error, McpError::Cancelled));
    assert!(elapsed < Duration::from_secs(1), "cancel took {elapsed:?}");
    fixture.finish();
}
