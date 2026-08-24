//! Workflow parameter references and root-to-node input bindings.

use super::*;

pub(super) fn resolve_node_params(
    node: &StoredWorkflowNode,
    results: &HashMap<String, JsonValue>,
) -> (JsonMap<String, JsonValue>, Option<String>) {
    let mut child_args = JsonMap::new();
    let mut child_input = None;

    for (target, raw_value) in &node.params {
        let json_value = yaml_value_to_json(raw_value);
        if let Some(reference) = json_value.as_str() {
            if let Some(resolved) = resolve_workflow_reference(reference, results) {
                if let Some(image) = json_value_as_image(&resolved) {
                    if child_input.is_none() {
                        child_input = Some(image);
                    } else {
                        insert_child_argument(&mut child_args, target, resolved);
                    }
                } else {
                    insert_child_argument(&mut child_args, target, resolved);
                }
                continue;
            }
        }

        insert_child_argument(&mut child_args, target, json_value);
    }

    (child_args, child_input)
}

pub(super) fn apply_input_bindings(
    workflow_id: &str,
    bindings: &WorkflowExecutionBindings,
    node: &StoredWorkflowNode,
    root_arguments: &JsonValue,
    root_input: &Option<String>,
    child_input: &mut Option<String>,
    child_args: &mut JsonMap<String, JsonValue>,
) -> WorkflowRuntimeResult<bool> {
    let mut missing_image_binding = false;

    for binding in bindings
        .inputs
        .iter()
        .filter(|binding| binding.node_id == node.id)
    {
        match binding.kind.as_str() {
            "input_image" => {
                if let Some(value) = bound_argument_as_image(binding, root_arguments, root_input) {
                    *child_input = Some(value);
                } else {
                    missing_image_binding = true;
                }
            }
            "input_value" | "param" => {
                if let Some(value) = bound_argument_value(binding, root_arguments) {
                    insert_child_argument(child_args, &binding.target, value);
                }
            }
            _ => {
                return Err(WorkflowRuntimeError::InvalidWorkflow {
                    workflow_id: workflow_id.to_owned(),
                    reason: format!("unsupported input binding kind `{}`", binding.kind),
                });
            }
        }
    }

    Ok(missing_image_binding)
}

pub(super) fn bound_argument_value(
    binding: &WorkflowInputBinding,
    root_arguments: &JsonValue,
) -> Option<JsonValue> {
    let arguments = root_arguments.as_object()?;
    arguments
        .get(&binding.workflow_param)
        .or_else(|| {
            arguments
                .get("params")
                .and_then(JsonValue::as_object)
                .and_then(|params| params.get(&binding.workflow_param))
        })
        .or_else(|| {
            arguments
                .get("inputs")
                .and_then(JsonValue::as_object)
                .and_then(|inputs| inputs.get(&binding.workflow_param))
        })
        .cloned()
}

pub(super) fn bound_argument_as_image(
    binding: &WorkflowInputBinding,
    root_arguments: &JsonValue,
    root_input: &Option<String>,
) -> Option<String> {
    bound_argument_value(binding, root_arguments)
        .and_then(|value| json_value_as_image(&value))
        .or_else(|| {
            if binding.workflow_param == "input" {
                root_input.clone()
            } else {
                None
            }
        })
}
