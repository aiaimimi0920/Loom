//! MCP execution behavior coverage.

use super::*;

#[test]
pub(super) fn execute_mcp_tool_calls_configured_server() {
    let tool = ToolDefinition::new(
        "fixture-echo",
        "Fixture Echo",
        "Echo through fixture MCP",
        ToolExecution::Mcp {
            server_id: "fixture".to_owned(),
            tool_name: "echo".to_owned(),
        },
    );
    let server = current_test_binary_fixture_config();

    let result = execute_tool(
        &tool,
        &[server],
        serde_json::json!({ "text": "hello registry" }),
    )
    .expect("execute MCP-backed tool");

    assert_eq!(result["content"][0]["type"], "text");
    assert_eq!(result["content"][0]["text"], "hello registry");
}

#[test]
pub(super) fn repeated_mcp_calls_reuse_the_initialized_session() {
    let tool = ToolDefinition::new(
        "fixture-counter",
        "Fixture Counter",
        "Count calls in one MCP server process",
        ToolExecution::Mcp {
            server_id: "fixture".to_owned(),
            tool_name: "counter".to_owned(),
        },
    );
    let server = current_test_binary_fixture_config();

    let first = execute_tool(&tool, std::slice::from_ref(&server), serde_json::json!({}))
        .expect("first pooled MCP call");
    let second =
        execute_tool(&tool, &[server], serde_json::json!({})).expect("second pooled MCP call");

    assert_eq!(first["content"][0]["text"], "1");
    assert_eq!(second["content"][0]["text"], "2");
    clear_cached_mcp_sessions_for_current_thread();
}

#[test]
pub(super) fn a_cancelled_mcp_run_stops_before_the_server_is_started() {
    let tool = ToolDefinition::new(
        "fixture-echo",
        "Fixture Echo",
        "Echo through fixture MCP",
        ToolExecution::Mcp {
            server_id: "fixture".to_owned(),
            tool_name: "echo".to_owned(),
        },
    );
    // A command that cannot be spawned: reaching the connect step at all would fail with an MCP
    // error rather than a cancellation, so the assertion below proves the run stopped before it.
    let server = loom_mcp::McpServerConfig::new(
        "fixture",
        "Fixture MCP",
        "loom-nonexistent-mcp-server-binary",
    );
    let cancellation = AtomicBool::new(true);

    let error = execute_tool_with_timeout_and_cancellation(
        &tool,
        &[server],
        serde_json::json!({ "text": "hello registry" }),
        Duration::from_secs(5),
        &cancellation,
    )
    .expect_err("a cancelled run does not execute");

    assert!(
        matches!(error, ToolRegistryError::ExecutionCancelled { ref id } if id == "fixture-echo"),
        "unexpected error: {error}"
    );
}

#[test]
pub(super) fn an_in_flight_mcp_round_trip_is_cancelled() {
    let tool = ToolDefinition::new(
        "fixture-echo-cancel",
        "Fixture Echo Cancel",
        "Cancel a hung MCP round trip",
        ToolExecution::Mcp {
            server_id: "fixture".to_owned(),
            tool_name: "echo".to_owned(),
        },
    );
    let server =
        current_test_binary_fixture_config().env("LOOM_TOOL_REGISTRY_MCP_FIXTURE_MODE", "hang");
    let cancellation = Arc::new(AtomicBool::new(false));
    let trigger = Arc::clone(&cancellation);
    let trigger_thread = thread::spawn(move || {
        thread::sleep(Duration::from_millis(50));
        trigger.store(true, Ordering::Release);
    });

    let started = Instant::now();
    let error = execute_tool_with_timeout_and_cancellation(
        &tool,
        &[server],
        serde_json::json!({ "text": "never returned" }),
        Duration::from_secs(5),
        cancellation.as_ref(),
    )
    .expect_err("the hung MCP round trip must be cancelled");
    let elapsed = started.elapsed();

    trigger_thread
        .join()
        .expect("join MCP cancellation trigger");
    assert!(matches!(
        error,
        ToolRegistryError::ExecutionCancelled { ref id } if id == "fixture-echo-cancel"
    ));
    assert!(elapsed < Duration::from_secs(1), "cancel took {elapsed:?}");
}

#[test]
pub(super) fn an_uncancelled_mcp_run_still_reaches_the_server() {
    let tool = ToolDefinition::new(
        "fixture-echo",
        "Fixture Echo",
        "Echo through fixture MCP",
        ToolExecution::Mcp {
            server_id: "fixture".to_owned(),
            tool_name: "echo".to_owned(),
        },
    );
    let server = current_test_binary_fixture_config();
    let cancellation = AtomicBool::new(false);

    let result = execute_tool_with_timeout_and_cancellation(
        &tool,
        &[server],
        serde_json::json!({ "text": "hello registry" }),
        Duration::from_secs(30),
        &cancellation,
    )
    .expect("an uncancelled run executes normally");

    assert_eq!(result["content"][0]["text"], "hello registry");
}
