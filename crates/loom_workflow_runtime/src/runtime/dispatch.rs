//! Tool dispatch plus shared deadline and cancellation checks.

use super::*;

const MAX_WORKFLOW_NESTING: usize = 32;

#[derive(Default)]
pub(super) struct ExecutionContext {
    workflow_stack: Vec<String>,
}

impl ExecutionContext {
    fn enter(&mut self, workflow_id: &str) -> WorkflowRuntimeResult<()> {
        if self.workflow_stack.len() >= MAX_WORKFLOW_NESTING {
            return Err(WorkflowRuntimeError::InvalidWorkflow {
                workflow_id: workflow_id.to_owned(),
                reason: format!("nested workflow depth exceeds {MAX_WORKFLOW_NESTING}"),
            });
        }
        if self
            .workflow_stack
            .iter()
            .any(|active| active == workflow_id)
        {
            return Err(WorkflowRuntimeError::InvalidWorkflow {
                workflow_id: workflow_id.to_owned(),
                reason: "recursive workflow dependency detected".to_owned(),
            });
        }
        self.workflow_stack.push(workflow_id.to_owned());
        Ok(())
    }

    fn leave(&mut self, workflow_id: &str) {
        let completed = self.workflow_stack.pop();
        debug_assert_eq!(completed.as_deref(), Some(workflow_id));
    }
}

pub(super) fn execute_tool_with_workflows_internal(
    tool: &ToolDefinition,
    mcp_servers: &[loom_mcp::McpServerConfig],
    workflow_store: &WorkflowStore,
    tool_registry: &ToolRegistry,
    arguments: JsonValue,
    preview_callback: &mut dyn FnMut(JsonValue),
    deadline: Option<Instant>,
    cancellation: Option<&AtomicBool>,
    execution: &mut ExecutionContext,
) -> WorkflowRuntimeResult<serde_json::Value> {
    check_cancelled(cancellation)?;
    let arguments = prepare_tool_arguments(tool, arguments)?;
    match &tool.execution {
        ToolExecution::Workflow {
            workflow_id,
            workflow_bindings,
        } => {
            execution.enter(workflow_id)?;
            let result = execute_workflow_tool(
                tool,
                workflow_id,
                workflow_bindings.as_ref(),
                mcp_servers,
                workflow_store,
                tool_registry,
                arguments,
                preview_callback,
                deadline,
                cancellation,
                execution,
            );
            execution.leave(workflow_id);
            result
        }
        _ => {
            let result = match (remaining_timeout(deadline)?, cancellation) {
                (Some(timeout), Some(cancellation)) => execute_tool_with_timeout_and_cancellation(
                    tool,
                    mcp_servers,
                    arguments,
                    timeout,
                    cancellation,
                ),
                (Some(timeout), None) => {
                    execute_tool_with_timeout(tool, mcp_servers, arguments, timeout)
                }
                (None, _) => execute_tool(tool, mcp_servers, arguments),
            }
            .map_err(WorkflowRuntimeError::from)?;
            check_cancelled(cancellation)?;
            Ok(result)
        }
    }
}

pub(super) fn check_cancelled(cancellation: Option<&AtomicBool>) -> WorkflowRuntimeResult<()> {
    if cancellation.is_some_and(|token| token.load(Ordering::Acquire)) {
        return Err(WorkflowRuntimeError::Cancelled);
    }
    Ok(())
}

pub(super) fn remaining_timeout(
    deadline: Option<Instant>,
) -> WorkflowRuntimeResult<Option<Duration>> {
    let Some(deadline) = deadline else {
        return Ok(None);
    };
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(WorkflowRuntimeError::Timeout);
    }
    Ok(Some(remaining))
}
