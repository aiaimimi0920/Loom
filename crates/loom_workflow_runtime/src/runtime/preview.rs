//! Preview binding validation and dependency-priority closure.

use super::*;

#[derive(Debug)]
pub(super) struct WorkflowPreviewPolicy {
    pub(super) output: WorkflowOutputBinding,
    pub(super) required_nodes: BTreeSet<String>,
    pub(super) priority_nodes: BTreeSet<String>,
}

pub(super) fn validate_preview_policy(
    workflow_id: &str,
    workflow: &StoredWorkflow,
    bindings: &WorkflowExecutionBindings,
    preview_output: &WorkflowOutputBinding,
    root_tool: &ToolDefinition,
    tool_registry: &ToolRegistry,
) -> WorkflowRuntimeResult<WorkflowPreviewPolicy> {
    if preview_output.kind != "node_result" {
        return Err(WorkflowRuntimeError::InvalidPreviewPolicy {
            workflow_id: workflow_id.to_owned(),
            reason: format!("unsupported preview output kind `{}`", preview_output.kind),
        });
    }
    if !valid_field(&preview_output.output, MAX_BINDING_FIELD_BYTES, true) {
        return Err(WorkflowRuntimeError::InvalidPreviewPolicy {
            workflow_id: workflow_id.to_owned(),
            reason: "preview output name is invalid".to_owned(),
        });
    }
    if bindings.preview_required_nodes.len() > MAX_WORKFLOW_NODES {
        return Err(WorkflowRuntimeError::InvalidPreviewPolicy {
            workflow_id: workflow_id.to_owned(),
            reason: "preview policy has too many required nodes".to_owned(),
        });
    }
    let nodes = workflow
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect::<HashMap<_, _>>();
    let preview_node = nodes.get(preview_output.node_id.as_str()).ok_or_else(|| {
        WorkflowRuntimeError::InvalidPreviewPolicy {
            workflow_id: workflow_id.to_owned(),
            reason: format!("preview node `{}` does not exist", preview_output.node_id),
        }
    })?;

    if !preview_output.output.trim().is_empty()
        && !is_sticker_art(&preview_node.uses)
        && !loom_native_image::is_native_art_id(&preview_node.uses)
    {
        if let Some(tool) =
            resolve_workflow_child_tool(root_tool, &preview_node.uses, tool_registry)?
        {
            let output_names = tool
                .outputs
                .iter()
                .filter_map(|output| output.get("name").and_then(JsonValue::as_str))
                .collect::<BTreeSet<_>>();
            if !output_names.is_empty() && !output_names.contains(preview_output.output.as_str()) {
                return Err(WorkflowRuntimeError::InvalidPreviewPolicy {
                    workflow_id: workflow_id.to_owned(),
                    reason: format!(
                        "node `{}` has no output `{}`",
                        preview_output.node_id, preview_output.output
                    ),
                });
            }
        }
    }

    let mut required_nodes = bindings
        .preview_required_nodes
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    if required_nodes.len() != bindings.preview_required_nodes.len() {
        return Err(WorkflowRuntimeError::InvalidPreviewPolicy {
            workflow_id: workflow_id.to_owned(),
            reason: "preview policy repeats a required node".to_owned(),
        });
    }
    required_nodes.insert(preview_output.node_id.clone());
    if let Some(missing) = required_nodes
        .iter()
        .find(|node_id| !nodes.contains_key(node_id.as_str()))
    {
        return Err(WorkflowRuntimeError::InvalidPreviewPolicy {
            workflow_id: workflow_id.to_owned(),
            reason: format!("required node `{missing}` does not exist"),
        });
    }

    let mut priority_nodes = required_nodes.clone();
    let mut stack = required_nodes.iter().cloned().collect::<Vec<_>>();
    while let Some(node_id) = stack.pop() {
        let node = nodes.get(node_id.as_str()).ok_or_else(|| {
            WorkflowRuntimeError::InvalidPreviewPolicy {
                workflow_id: workflow_id.to_owned(),
                reason: format!("preview dependency `{node_id}` does not exist"),
            }
        })?;
        for dependency in &node.needs {
            if !nodes.contains_key(dependency.as_str()) {
                return Err(WorkflowRuntimeError::InvalidPreviewPolicy {
                    workflow_id: workflow_id.to_owned(),
                    reason: format!("preview dependency `{dependency}` does not exist"),
                });
            }
            if priority_nodes.insert(dependency.clone()) {
                stack.push(dependency.clone());
            }
        }
    }

    Ok(WorkflowPreviewPolicy {
        output: preview_output.clone(),
        required_nodes,
        priority_nodes,
    })
}
