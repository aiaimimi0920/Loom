//! Public execution entry points and caller-owned timeout/cancellation setup.

use super::*;

pub fn execute_tool_with_workflows(
    tool: &ToolDefinition,
    mcp_servers: &[loom_mcp::McpServerConfig],
    workflow_store: &WorkflowStore,
    tool_registry: &ToolRegistry,
    arguments: JsonValue,
) -> WorkflowRuntimeResult<serde_json::Value> {
    let mut execution = ExecutionContext::default();
    execute_tool_with_workflows_internal(
        tool,
        mcp_servers,
        workflow_store,
        tool_registry,
        arguments,
        &mut |_| {},
        None,
        None,
        &mut execution,
    )
}

pub fn execute_tool_with_workflows_timeout(
    tool: &ToolDefinition,
    mcp_servers: &[loom_mcp::McpServerConfig],
    workflow_store: &WorkflowStore,
    tool_registry: &ToolRegistry,
    arguments: JsonValue,
    timeout: Duration,
) -> WorkflowRuntimeResult<serde_json::Value> {
    let mut execution = ExecutionContext::default();
    execute_tool_with_workflows_internal(
        tool,
        mcp_servers,
        workflow_store,
        tool_registry,
        arguments,
        &mut |_| {},
        Some(Instant::now() + timeout.max(Duration::from_millis(1))),
        None,
        &mut execution,
    )
}

/// Run a tool under a caller-owned timeout and cancellation flag, with no preview stream.
///
/// The preview variant of this call already existed, and a caller that wants cancellation without
/// previews had to pass a callback that discards everything it is handed. The surface action runner is
/// such a caller: an action has nowhere to show intermediate output, but it does need to be cancellable.
pub fn execute_tool_with_workflows_timeout_and_cancellation(
    tool: &ToolDefinition,
    mcp_servers: &[loom_mcp::McpServerConfig],
    workflow_store: &WorkflowStore,
    tool_registry: &ToolRegistry,
    arguments: JsonValue,
    timeout: Duration,
    cancellation: &AtomicBool,
) -> WorkflowRuntimeResult<serde_json::Value> {
    let mut execution = ExecutionContext::default();
    execute_tool_with_workflows_internal(
        tool,
        mcp_servers,
        workflow_store,
        tool_registry,
        arguments,
        &mut |_| {},
        Some(Instant::now() + timeout.max(Duration::from_millis(1))),
        Some(cancellation),
        &mut execution,
    )
}

pub fn execute_tool_with_workflows_and_preview<F>(
    tool: &ToolDefinition,
    mcp_servers: &[loom_mcp::McpServerConfig],
    workflow_store: &WorkflowStore,
    tool_registry: &ToolRegistry,
    arguments: JsonValue,
    mut preview_callback: F,
) -> WorkflowRuntimeResult<serde_json::Value>
where
    F: FnMut(JsonValue),
{
    let mut execution = ExecutionContext::default();
    execute_tool_with_workflows_internal(
        tool,
        mcp_servers,
        workflow_store,
        tool_registry,
        arguments,
        &mut preview_callback,
        None,
        None,
        &mut execution,
    )
}

pub fn execute_tool_with_workflows_and_preview_timeout_and_cancellation<F>(
    tool: &ToolDefinition,
    mcp_servers: &[loom_mcp::McpServerConfig],
    workflow_store: &WorkflowStore,
    tool_registry: &ToolRegistry,
    arguments: JsonValue,
    timeout: Duration,
    cancellation: &AtomicBool,
    mut preview_callback: F,
) -> WorkflowRuntimeResult<serde_json::Value>
where
    F: FnMut(JsonValue),
{
    let mut execution = ExecutionContext::default();
    execute_tool_with_workflows_internal(
        tool,
        mcp_servers,
        workflow_store,
        tool_registry,
        arguments,
        &mut preview_callback,
        Some(Instant::now() + timeout.max(Duration::from_millis(1))),
        Some(cancellation),
        &mut execution,
    )
}
