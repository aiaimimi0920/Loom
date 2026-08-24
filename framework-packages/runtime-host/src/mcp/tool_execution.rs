// Sequential tool execution, outcome retention, and schema lookup.
fn execute_tools(
    server: &McpServerConfig,
    calls: &[ResolvedCall],
    argument_aliases: &BTreeMap<String, BTreeMap<String, String>>,
) -> Result<McpBatchExecution, String> {
    let mut session = acquire_mcp_session(server)?;
    let mut reusable = true;
    // Calls remain sequential (concurrency = 1). That is an explicit bounded policy: many MCP
    // servers are stateful, while the manifest already caps a batch at eight calls. Failures are
    // captured per call so one rejection never discards earlier successes or prevents later calls.
    let outcomes = collect_call_outcomes(calls, |call| {
        let schema = find_tool_input_schema(&session.tools, &call.tool_name)
            .ok_or_else(|| format!("MCP server does not expose tool `{}`", call.tool_name))?;
        let normalized_arguments = normalize_arguments(&call.arguments, schema, argument_aliases)?;
        match session
            .client
            .call_tool(&call.tool_name, normalized_arguments)
        {
            Ok(result) => match validate_mcp_response_value(&result, "MCP tools/call response") {
                Ok(()) => Ok(result),
                Err(error) => {
                    reusable = false;
                    Err(error)
                }
            },
            Err(error) => {
                reusable = false;
                Err(format!(
                    "MCP tools/call `{}` failed: {error}",
                    call.tool_name
                ))
            }
        }
    });
    let close_error = if reusable {
        return_cached_mcp_session(session);
        None
    } else {
        session
            .client
            .close()
            .err()
            .map(|error| format!("MCP session close failed: {error}"))
    };
    Ok(McpBatchExecution {
        outcomes,
        close_error,
    })
}

fn collect_call_outcomes(
    calls: &[ResolvedCall],
    mut execute: impl FnMut(&ResolvedCall) -> Result<Value, String>,
) -> Vec<McpCallOutcome> {
    calls
        .iter()
        .map(|call| match execute(call) {
            Ok(value) => McpCallOutcome::Success(value),
            Err(error) => McpCallOutcome::Failure(error),
        })
        .collect()
}

fn find_tool_input_schema<'a>(tools: &'a Value, tool_name: &str) -> Option<&'a Value> {
    tools
        .get("tools")?
        .as_array()?
        .iter()
        .find(|tool| tool.get("name").and_then(Value::as_str) == Some(tool_name))
        .and_then(|tool| tool.get("inputSchema"))
}
