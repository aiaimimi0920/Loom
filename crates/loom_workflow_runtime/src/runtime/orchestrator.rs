//! Deterministic dependency scheduling, preview publication, and final selection.

use super::*;

pub(super) fn execute_workflow_tool(
    root_tool: &ToolDefinition,
    workflow_id: &str,
    workflow_bindings: Option<&WorkflowExecutionBindings>,
    mcp_servers: &[loom_mcp::McpServerConfig],
    workflow_store: &WorkflowStore,
    tool_registry: &ToolRegistry,
    arguments: JsonValue,
    preview_callback: &mut dyn FnMut(JsonValue),
    deadline: Option<Instant>,
    cancellation: Option<&AtomicBool>,
    execution: &mut ExecutionContext,
) -> WorkflowRuntimeResult<JsonValue> {
    check_cancelled(cancellation)?;
    let workflow = load_stored_workflow(workflow_store, workflow_id)?;
    validate_workflow_bindings(workflow_id, &workflow, workflow_bindings)?;
    let preview_policy = workflow_bindings
        .and_then(|bindings| bindings.preview_output.as_ref())
        .map(|preview_output| {
            validate_preview_policy(
                workflow_id,
                &workflow,
                workflow_bindings.expect("preview output requires bindings"),
                preview_output,
                root_tool,
                tool_registry,
            )
        })
        .transpose()?;

    if workflow.nodes.is_empty() {
        return Ok(text_content_response(&format!(
            "workflow `{workflow_id}` completed with no nodes"
        )));
    }

    let root_input = extract_root_input(&arguments);
    let mut pending = workflow
        .nodes
        .iter()
        .cloned()
        .map(|node| (node.id.clone(), node))
        .collect::<BTreeMap<_, _>>();
    let order = workflow
        .nodes
        .iter()
        .map(|node| node.id.clone())
        .collect::<Vec<_>>();
    let mut results = HashMap::<String, JsonValue>::new();
    let mut stored_result_bytes = 0;
    let mut preview_emitted = false;

    while !pending.is_empty() {
        check_cancelled(cancellation)?;
        let mut ready = order
            .iter()
            .filter(|node_id| pending.contains_key(*node_id))
            .filter(|node_id| {
                pending[*node_id]
                    .needs
                    .iter()
                    .all(|dependency| results.contains_key(dependency))
            })
            .cloned()
            .collect::<Vec<_>>();

        if let Some(policy) = &preview_policy {
            ready.sort_by_key(|node_id| !policy.priority_nodes.contains(node_id));
        }

        if ready.is_empty() {
            return Err(WorkflowRuntimeError::UnresolvedDependencies {
                workflow_id: workflow_id.to_owned(),
            });
        }

        for node_id in ready {
            check_cancelled(cancellation)?;
            remaining_timeout(deadline)?;
            let node = pending
                .remove(&node_id)
                .expect("ready node is present in pending map");
            let result = execute_workflow_node(
                workflow_id,
                root_tool,
                &node,
                workflow_bindings,
                &root_input,
                &arguments,
                &results,
                mcp_servers,
                workflow_store,
                tool_registry,
                deadline,
                cancellation,
                execution,
            )?;
            check_cancelled(cancellation)?;
            stored_result_bytes =
                reserve_workflow_result(workflow_id, &node_id, &result, stored_result_bytes)?;
            results.insert(node_id, result);
            if !preview_emitted {
                if let Some(policy) = &preview_policy {
                    if policy
                        .required_nodes
                        .iter()
                        .all(|required| results.contains_key(required))
                    {
                        let preview = select_bound_workflow_output(&policy.output, &results)
                            .ok_or_else(|| WorkflowRuntimeError::InvalidPreviewPolicy {
                                workflow_id: workflow_id.to_owned(),
                                reason: format!(
                                    "node `{}` did not produce output `{}`",
                                    policy.output.node_id, policy.output.output
                                ),
                            })?;
                        preview_callback(preview);
                        preview_emitted = true;
                    }
                }
            }
        }
    }

    Ok(select_workflow_output(
        workflow_id,
        &workflow.nodes,
        workflow_bindings.and_then(|bindings| bindings.primary_output.as_ref()),
        &results,
    ))
}
