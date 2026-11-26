//! Tool dispatch plus shared deadline, cancellation, and terminal relay context.

use super::*;

const MAX_WORKFLOW_NESTING: usize = 32;

#[derive(Default)]
pub(super) struct ExecutionContext<'a> {
    workflow_stack: Vec<String>,
    pub(super) terminal_relay: Option<&'a mut dyn WorkflowTerminalRelay>,
    pub(super) placement_trace: Vec<WorkflowPlacementStep>,
}

impl<'a> ExecutionContext<'a> {
    pub(super) fn with_relay(relay: Option<&'a mut dyn WorkflowTerminalRelay>) -> Self {
        Self {
            workflow_stack: Vec::new(),
            terminal_relay: relay,
            placement_trace: Vec::new(),
        }
    }

    pub(super) fn is_root_workflow(&self) -> bool {
        self.workflow_stack.len() == 1
    }

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
    execution: &mut ExecutionContext<'_>,
) -> WorkflowRuntimeResult<serde_json::Value> {
    check_cancelled(cancellation)?;
    match &tool.execution {
        ToolExecution::Workflow {
            workflow_id,
            workflow_bindings,
        } => {
            let arguments = tool_registry.prepare_arguments(tool, arguments)?;
            execution.enter(workflow_id)?;
            let is_root = execution.is_root_workflow();
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
            result.map(|output| {
                if is_root {
                    attach_placement_trace(output, &execution.placement_trace)
                } else {
                    output
                }
            })
        }
        _ => {
            let result = tool_registry
                .execute(
                    tool,
                    mcp_servers,
                    arguments,
                    remaining_timeout(deadline)?,
                    cancellation,
                )
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
