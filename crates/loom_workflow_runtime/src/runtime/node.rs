//! Per-node argument resolution, image/native adapters, and recursive child execution.

use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn execute_workflow_node(
    workflow_id: &str,
    parent_tool: &ToolDefinition,
    node: &StoredWorkflowNode,
    workflow_bindings: Option<&WorkflowExecutionBindings>,
    root_input: &Option<String>,
    root_arguments: &JsonValue,
    results: &HashMap<String, JsonValue>,
    mcp_servers: &[loom_mcp::McpServerConfig],
    workflow_store: &WorkflowStore,
    tool_registry: &ToolRegistry,
    deadline: Option<Instant>,
    cancellation: Option<&AtomicBool>,
    execution: &mut ExecutionContext,
) -> WorkflowRuntimeResult<JsonValue> {
    check_cancelled(cancellation)?;
    let (mut child_args, mut child_input) = resolve_node_params(node, results);

    let missing_explicit_image_binding = workflow_bindings
        .map(|bindings| {
            apply_input_bindings(
                workflow_id,
                bindings,
                node,
                root_arguments,
                root_input,
                &mut child_input,
                &mut child_args,
            )
        })
        .transpose()?
        .unwrap_or(false);

    if missing_explicit_image_binding {
        return Err(WorkflowRuntimeError::MissingImageInput {
            workflow_id: workflow_id.to_owned(),
            node_id: node.id.clone(),
        });
    }

    if child_input.is_none() {
        child_input = extract_root_input(&JsonValue::Object(child_args.clone()));
    }
    if child_input.is_none() {
        // Hook canvas edges are serialized as `needs`. When the node has no
        // explicit input binding, keep the visual pipeline semantics by
        // forwarding the first upstream image along that edge.
        child_input = node
            .needs
            .iter()
            .find_map(|dependency| results.get(dependency).and_then(json_value_as_image));
    }
    if child_input.is_none() && node.needs.is_empty() {
        child_input = root_input.clone();
    }
    if child_input.is_none() {
        child_input = node
            .meta
            .as_ref()
            .and_then(|meta| meta.preview_src.as_deref().or(meta.src.as_deref()))
            .and_then(normalize_embedded_image_reference);
    }

    if let Some(input) = &child_input {
        insert_child_input(&mut child_args, input);
    }

    if is_sticker_art(&node.uses) {
        let input = child_input.ok_or_else(|| WorkflowRuntimeError::MissingImageInput {
            workflow_id: workflow_id.to_owned(),
            node_id: node.id.clone(),
        })?;
        return Ok(image_content_response(&input, "image/png"));
    }

    if loom_native_image::is_native_art_id(&node.uses) {
        let input = child_input.ok_or_else(|| WorkflowRuntimeError::MissingImageInput {
            workflow_id: workflow_id.to_owned(),
            node_id: node.id.clone(),
        })?;
        let params = child_args.into_iter().collect::<HashMap<_, _>>();
        let result = loom_native_image::process_art(&node.uses, &input, params);
        if !result.success {
            return Err(WorkflowRuntimeError::NativeFailed {
                workflow_id: workflow_id.to_owned(),
                node_id: node.id.clone(),
                message: result
                    .error
                    .unwrap_or_else(|| "native image processor failed".to_owned()),
            });
        }
        let output = result
            .output_base64
            .ok_or_else(|| WorkflowRuntimeError::NativeFailed {
                workflow_id: workflow_id.to_owned(),
                node_id: node.id.clone(),
                message: "native image processor returned no output".to_owned(),
            })?;
        return Ok(image_content_response(&output, "image/png"));
    }

    let child_tool = resolve_workflow_child_tool(parent_tool, &node.uses, tool_registry)?
        .ok_or_else(|| WorkflowRuntimeError::ChildToolNotFound {
            workflow_id: workflow_id.to_owned(),
            tool_id: node.uses.clone(),
        })?;

    execute_tool_with_workflows_internal(
        &child_tool,
        mcp_servers,
        workflow_store,
        tool_registry,
        JsonValue::Object(child_args),
        &mut |_| {},
        deadline,
        cancellation,
        execution,
    )
}
