//! Tool execution dispatch and timeout handling.

use super::*;

pub fn execute_tool(
    tool: &ToolDefinition,
    mcp_servers: &[loom_mcp::McpServerConfig],
    arguments: serde_json::Value,
) -> ToolRegistryResult<serde_json::Value> {
    execute_tool_with_optional_timeout(tool, mcp_servers, arguments, None, None)
}

pub fn execute_tool_with_timeout(
    tool: &ToolDefinition,
    mcp_servers: &[loom_mcp::McpServerConfig],
    arguments: serde_json::Value,
    timeout: Duration,
) -> ToolRegistryResult<serde_json::Value> {
    execute_tool_with_optional_timeout(
        tool,
        mcp_servers,
        arguments,
        Some(timeout.max(Duration::from_millis(1))),
        None,
    )
}

pub fn execute_tool_with_timeout_and_cancellation(
    tool: &ToolDefinition,
    mcp_servers: &[loom_mcp::McpServerConfig],
    arguments: serde_json::Value,
    timeout: Duration,
    cancellation: &AtomicBool,
) -> ToolRegistryResult<serde_json::Value> {
    execute_tool_with_optional_timeout(
        tool,
        mcp_servers,
        arguments,
        Some(timeout.max(Duration::from_millis(1))),
        Some(cancellation),
    )
}

pub(super) fn execute_tool_with_optional_timeout(
    tool: &ToolDefinition,
    mcp_servers: &[loom_mcp::McpServerConfig],
    arguments: serde_json::Value,
    timeout: Option<Duration>,
    cancellation: Option<&AtomicBool>,
) -> ToolRegistryResult<serde_json::Value> {
    tool.validate()?;
    if !tool.enabled {
        return Err(ToolRegistryError::ExecutionRejected {
            id: tool.id.clone(),
        });
    }

    let arguments = prepare_tool_arguments(tool, arguments)?;
    match &tool.execution {
        ToolExecution::Mcp {
            server_id,
            tool_name,
        } => {
            let server = mcp_servers
                .iter()
                .find(|server| server.id == *server_id && server.enabled)
                .ok_or_else(|| ToolRegistryError::MissingMcpServer {
                    tool_id: tool.id.clone(),
                    server_id: server_id.clone(),
                })?;

            let stop_if_cancelled = || -> ToolRegistryResult<()> {
                if cancellation.is_some_and(|token| token.load(Ordering::Acquire)) {
                    return Err(ToolRegistryError::ExecutionCancelled {
                        id: tool.id.clone(),
                    });
                }
                Ok(())
            };

            stop_if_cancelled()?;
            let mut session = acquire_mcp_session(tool, server, timeout, cancellation)?;
            let operation = (|| -> ToolRegistryResult<serde_json::Value> {
                stop_if_cancelled()?;
                let normalized_arguments = normalize_mcp_call_arguments(
                    &arguments,
                    session
                        .tools
                        .as_ref()
                        .and_then(|tools| find_mcp_tool_input_schema(tools, tool_name)),
                );
                stop_if_cancelled()?;
                let call = match cancellation {
                    Some(cancellation) => session.client.call_tool_cancellable(
                        tool_name,
                        normalized_arguments.clone(),
                        cancellation,
                    ),
                    None => session
                        .client
                        .call_tool(tool_name, normalized_arguments.clone()),
                };
                let result = match call {
                    Ok(value) => normalize_mcp_result(tool, &normalized_arguments, value),
                    Err(loom_mcp::McpError::Cancelled) => {
                        session.reusable = false;
                        return Err(ToolRegistryError::ExecutionCancelled {
                            id: tool.id.clone(),
                        });
                    }
                    Err(error) => {
                        session.reusable = matches!(error, loom_mcp::McpError::JsonRpc(_));
                        return Err(mcp_call_error(error, session.listing_failure.as_deref()));
                    }
                };
                stop_if_cancelled()?;
                Ok(result)
            })();
            let cancelled = cancellation.is_some_and(|token| token.load(Ordering::Acquire));
            if operation.is_ok() && session.reusable && !cancelled {
                return_cached_mcp_session(session);
            } else if cancelled {
                session.client.cancel();
            } else {
                let _ = session.client.close();
            }
            operation
        }
        ToolExecution::CloudApi {
            endpoint,
            method,
            content_type,
            headers,
            body,
        } => execute_cloud_api_tool(
            tool,
            endpoint,
            method,
            content_type.as_deref(),
            headers.as_deref(),
            body.as_deref(),
            arguments,
            cloud_api_timeout(tool, timeout),
            cancellation,
        ),
        ToolExecution::FrameworkArt { framework } => match (timeout, cancellation) {
            (Some(timeout), Some(cancellation)) => {
                framework_process::execute_framework_art_with_timeout_and_cancellation(
                    tool,
                    framework,
                    arguments,
                    timeout,
                    cancellation,
                )
            }
            (Some(timeout), None) => framework_process::execute_framework_art_with_timeout(
                tool, framework, arguments, timeout,
            ),
            (None, _) => framework_process::execute_framework_art(tool, framework, arguments),
        },
        _ => Err(ToolRegistryError::UnsupportedExecution {
            id: tool.id.clone(),
            execution_type: execution_type_name(&tool.execution),
        }),
    }
}

pub fn prepare_tool_arguments(
    tool: &ToolDefinition,
    arguments: serde_json::Value,
) -> ToolRegistryResult<serde_json::Value> {
    let arguments = art_settings::merge_tool_arguments(tool, arguments);
    art_settings::resolve_tool_value_bindings(tool, arguments).map_err(|error| {
        ToolRegistryError::ParameterBinding {
            id: tool.qualified_id(),
            reason: error.to_string(),
        }
    })
}
