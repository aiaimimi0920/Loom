//! Stdio transport, cancellation, stderr, and Windows launch coverage.

use super::*;

#[test]
pub(super) fn stdio_client_initializes_and_lists_tools_against_fixture_server() {
    let _guard = ProcessConfigTestGuard::capture();
    let config = current_test_binary_fixture_config();
    let mut client = StdioMcpClient::spawn(&config).expect("spawn fixture MCP server");

    let init = client.initialize().expect("initialize MCP fixture");
    let tools = client.list_tools().expect("list fixture MCP tools");

    assert_eq!(init["serverInfo"]["name"], "loom-fixture");
    assert_eq!(init["serverInfo"]["version"], "0.1.0");
    assert_eq!(tools["tools"][0]["name"], "echo");
    assert_eq!(tools["tools"][0]["description"], "Echo arguments");
}

#[test]
pub(super) fn stdio_initialize_falls_back_to_the_next_shared_protocol_revision() {
    let _guard = ProcessConfigTestGuard::capture();
    let config =
        current_test_binary_fixture_config().env("LOOM_MCP_FIXTURE_MODE", "reject-preferred");
    let mut client = StdioMcpClient::spawn(&config).expect("spawn fallback MCP fixture");

    let initialized = client
        .initialize()
        .expect("fall back to legacy MCP revision");

    assert_eq!(
        initialized["protocolVersion"],
        MCP_SUPPORTED_PROTOCOL_VERSIONS[1]
    );
}

#[test]
pub(super) fn stdio_initialize_reports_bounded_no_common_revision_error() {
    let _guard = ProcessConfigTestGuard::capture();
    let config =
        current_test_binary_fixture_config().env("LOOM_MCP_FIXTURE_MODE", "reject-all-protocols");
    let mut client = StdioMcpClient::spawn(&config).expect("spawn incompatible MCP fixture");

    let error = client
        .initialize()
        .expect_err("all rejected revisions must fail");

    let message = error.to_string();
    assert!(message.contains("rejected every supported protocol revision"));
    for version in MCP_SUPPORTED_PROTOCOL_VERSIONS {
        assert!(message.contains(version));
    }
    assert!(message.len() < 1024);
}

#[test]
pub(super) fn stdio_client_calls_fixture_tool_and_returns_structured_content() {
    let _guard = ProcessConfigTestGuard::capture();
    let config = current_test_binary_fixture_config();
    let mut client = StdioMcpClient::spawn(&config).expect("spawn fixture MCP server");

    client.initialize().expect("initialize MCP fixture");
    let result = client
        .call_tool("echo", serde_json::json!({ "text": "hello loom" }))
        .expect("call echo tool");

    assert_eq!(result["content"][0]["type"], "text");
    assert_eq!(result["content"][0]["text"], "hello loom");
}
#[test]
pub(super) fn stdio_client_times_out_and_terminates_hung_server() {
    let _guard = ProcessConfigTestGuard::capture();
    let config = current_test_binary_fixture_config().env("LOOM_MCP_FIXTURE_MODE", "hang");
    let mut client = StdioMcpClient::spawn_with_timeout(&config, Duration::from_millis(150))
        .expect("spawn hung fixture");
    let error = client.initialize().expect_err("hung fixture must time out");
    assert!(matches!(error, McpError::Timeout { .. }));
}

#[test]
pub(super) fn stdio_client_cancels_a_hung_request_before_its_timeout() {
    let _guard = ProcessConfigTestGuard::capture();
    let config = current_test_binary_fixture_config().env("LOOM_MCP_FIXTURE_MODE", "hang");
    let mut client = StdioMcpClient::spawn_with_timeout(&config, Duration::from_secs(5))
        .expect("spawn hung fixture");
    let cancellation = Arc::new(AtomicBool::new(false));
    let trigger = Arc::clone(&cancellation);
    let trigger_thread = thread::spawn(move || {
        thread::sleep(Duration::from_millis(50));
        trigger.store(true, Ordering::Release);
    });

    let started = Instant::now();
    let error = client
        .initialize_cancellable(cancellation.as_ref())
        .expect_err("hung fixture must be cancellable");
    let elapsed = started.elapsed();

    trigger_thread.join().expect("join cancellation trigger");
    assert!(matches!(error, McpError::Cancelled));
    assert!(elapsed < Duration::from_secs(1), "cancel took {elapsed:?}");
}
#[test]
pub(super) fn stdio_client_drains_bounded_stderr_without_deadlocking() {
    let _guard = ProcessConfigTestGuard::capture();
    let config = current_test_binary_fixture_config().env("LOOM_MCP_FIXTURE_MODE", "stderr-flood");
    let mut client = StdioMcpClient::spawn_with_timeout(&config, Duration::from_secs(5))
        .expect("spawn stderr fixture");
    let init = client
        .initialize()
        .expect("stderr flood must not block stdout");
    assert_eq!(init["serverInfo"]["name"], "loom-fixture");
}

#[cfg(windows)]
#[test]
pub(super) fn stdio_client_spawns_extensionless_windows_cmd_fixture() {
    let _guard = ProcessConfigTestGuard::capture();
    let fixture = windows_cmd_fixture_config();
    let mut client = StdioMcpClient::spawn(&fixture).expect("spawn extensionless cmd MCP fixture");

    let init = client.initialize().expect("initialize MCP fixture");
    let tools = client.list_tools().expect("list fixture MCP tools");

    assert_eq!(init["serverInfo"]["name"], "loom-fixture");
    assert_eq!(tools["tools"][0]["name"], "echo");
}

#[cfg(windows)]
#[test]
pub(super) fn resolve_windows_command_in_paths_prefers_cmd_wrapper_for_bare_command() {
    let temp_root = unique_test_temp_dir("resolve-path");
    std::fs::create_dir_all(&temp_root).expect("create path resolution temp dir");

    let command_base = temp_root.join("npx");
    std::fs::write(command_base.with_extension("ps1"), "Write-Host ignored")
        .expect("write ps1 candidate");
    std::fs::write(command_base.with_extension("cmd"), "@echo off\r\n")
        .expect("write cmd candidate");

    let resolved = resolve_windows_command_in_paths(
        Path::new("npx"),
        &[temp_root],
        &[".cmd".to_owned(), ".ps1".to_owned()],
    )
    .expect("resolve command candidate");

    assert_eq!(resolved, command_base.with_extension("cmd"));
}

#[cfg(windows)]
#[test]
pub(super) fn resolve_windows_spawn_command_wraps_powershell_scripts() {
    let config =
        McpServerConfig::new("fixture-ps1", "Fixture PS1", r"C:\loom\fixture.ps1").arg("--flag");

    let spawn_spec =
        resolve_windows_spawn_command(&config).expect("resolve powershell spawn wrapper");

    assert_eq!(spawn_spec.program, "powershell.exe");
    assert_eq!(
        spawn_spec.args,
        vec![
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            r"C:\loom\fixture.ps1",
            "--flag",
        ]
    );
}
