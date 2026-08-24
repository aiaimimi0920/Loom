// Session fixture process, reuse, and eviction contracts.
#[test]
fn runtime_mcp_session_pool_fixture_server() {
    if std::env::var("LOOM_RUNTIME_MCP_POOL_FIXTURE")
        .ok()
        .as_deref()
        != Some("1")
    {
        return;
    }
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let mut call_count = 0_u64;
    for line in stdin.lock().lines() {
        let request: Value = serde_json::from_str(&line.expect("fixture request line"))
            .expect("fixture request JSON");
        let method = request["method"].as_str().unwrap_or_default();
        if method == "notifications/initialized" {
            continue;
        }
        let result = match method {
            "initialize" => json!({
                "protocolVersion": request["params"]["protocolVersion"],
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "runtime-pool-fixture", "version": "0.1.0" }
            }),
            "tools/list" => json!({
                "tools": [{
                    "name": "count",
                    "inputSchema": { "type": "object", "properties": {} }
                }]
            }),
            "tools/call" => {
                call_count += 1;
                json!({ "content": [{ "type": "text", "text": call_count.to_string() }] })
            }
            _ => panic!("unexpected fixture method {method}"),
        };
        writeln!(
            stdout,
            "{}",
            json!({ "jsonrpc": "2.0", "id": request["id"], "result": result })
        )
        .expect("write fixture response");
        stdout.flush().expect("flush fixture response");
    }
    std::process::exit(0);
}

#[test]
fn repeated_runtime_executions_reuse_initialized_mcp_session() {
    clear_mcp_session_pool();
    let executable = std::env::current_exe().expect("current runtime-host test executable");
    let server = McpServerConfig::new(
        "runtime-pool-fixture",
        "Runtime Pool Fixture",
        executable.display().to_string(),
    )
    .arg("mcp::tests::runtime_mcp_session_pool_fixture_server")
    .arg("--exact")
    .arg("--nocapture")
    .env("LOOM_RUNTIME_MCP_POOL_FIXTURE", "1");
    let calls = vec![ResolvedCall {
        id: "count".to_owned(),
        tool_name: "count".to_owned(),
        arguments: json!({}),
    }];

    let first =
        execute_tools(&server, &calls, &BTreeMap::new()).expect("first pooled MCP execution");
    let second =
        execute_tools(&server, &calls, &BTreeMap::new()).expect("second pooled MCP execution");

    let count = |batch: &McpBatchExecution| match &batch.outcomes[0] {
        McpCallOutcome::Success(value) => value["content"][0]["text"]
            .as_str()
            .expect("fixture count text")
            .to_owned(),
        McpCallOutcome::Failure(error) => panic!("fixture call failed: {error}"),
    };
    assert_eq!(count(&first), "1");
    assert_eq!(count(&second), "2");
    clear_mcp_session_pool();
}

#[test]
fn changing_server_config_evicts_the_old_session_before_connecting() {
    clear_mcp_session_pool();
    let executable = std::env::current_exe().expect("current runtime-host test executable");
    let server = |instance: &str| {
        McpServerConfig::new(
            format!("runtime-pool-fixture-{instance}"),
            "Runtime Pool Fixture",
            executable.display().to_string(),
        )
        .arg("mcp::tests::runtime_mcp_session_pool_fixture_server")
        .arg("--exact")
        .arg("--nocapture")
        .env("LOOM_RUNTIME_MCP_POOL_FIXTURE", "1")
        .env("LOOM_RUNTIME_MCP_POOL_INSTANCE", instance)
    };
    let server_a = server("a");
    let server_b = server("b");
    let calls = vec![ResolvedCall {
        id: "count".to_owned(),
        tool_name: "count".to_owned(),
        arguments: json!({}),
    }];
    let count = |batch: &McpBatchExecution| match &batch.outcomes[0] {
        McpCallOutcome::Success(value) => value["content"][0]["text"]
            .as_str()
            .expect("fixture count text")
            .to_owned(),
        McpCallOutcome::Failure(error) => panic!("fixture call failed: {error}"),
    };

    let first_a =
        execute_tools(&server_a, &calls, &BTreeMap::new()).expect("first server A execution");
    let first_b =
        execute_tools(&server_b, &calls, &BTreeMap::new()).expect("first server B execution");
    let second_a =
        execute_tools(&server_a, &calls, &BTreeMap::new()).expect("replacement server A execution");

    assert_eq!(count(&first_a), "1");
    assert_eq!(count(&first_b), "1");
    assert_eq!(count(&second_a), "1");
    clear_mcp_session_pool();
}
