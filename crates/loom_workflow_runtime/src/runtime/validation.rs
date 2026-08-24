//! Bounded workflow shape and binding validation before scheduling starts.

use super::*;

pub(super) const MAX_WORKFLOW_YAML_BYTES: usize = 4 * 1024 * 1024;
pub(super) const MAX_WORKFLOW_NODES: usize = 256;
const MAX_NODE_ID_BYTES: usize = 128;
const MAX_NODE_USES_BYTES: usize = 256;
const MAX_NODE_DEPENDENCIES: usize = 256;
const MAX_NODE_PARAMS: usize = 128;
const MAX_WORKFLOW_VALUE_NODES: usize = 16_384;
const MAX_WORKFLOW_VALUE_DEPTH: usize = 32;
const MAX_WORKFLOW_BINDINGS: usize = 512;
pub(super) const MAX_BINDING_FIELD_BYTES: usize = 256;

pub(super) fn validate_workflow(
    workflow_id: &str,
    workflow: &StoredWorkflow,
) -> WorkflowRuntimeResult<()> {
    if workflow.nodes.len() > MAX_WORKFLOW_NODES {
        return invalid(
            workflow_id,
            format!("contains more than {MAX_WORKFLOW_NODES} nodes"),
        );
    }
    let mut node_ids = BTreeSet::new();
    let mut value_nodes = 0;
    for node in &workflow.nodes {
        if !valid_identifier(&node.id, MAX_NODE_ID_BYTES) {
            return invalid(workflow_id, format!("node id `{}` is invalid", node.id));
        }
        if !node_ids.insert(node.id.as_str()) {
            return invalid(workflow_id, format!("duplicate node id `{}`", node.id));
        }
        if node.uses.trim().is_empty() || node.uses.len() > MAX_NODE_USES_BYTES {
            return invalid(
                workflow_id,
                format!("node `{}` has an invalid tool id", node.id),
            );
        }
        if node.needs.len() > MAX_NODE_DEPENDENCIES {
            return invalid(
                workflow_id,
                format!("node `{}` has too many dependencies", node.id),
            );
        }
        if node.params.len() > MAX_NODE_PARAMS {
            return invalid(
                workflow_id,
                format!("node `{}` has too many parameters", node.id),
            );
        }
        for (target, value) in &node.params {
            if !valid_field(target, MAX_BINDING_FIELD_BYTES, false) {
                return invalid(
                    workflow_id,
                    format!("node `{}` has an invalid parameter target", node.id),
                );
            }
            validate_yaml_value(workflow_id, value, 0, &mut value_nodes)?;
        }
    }
    for node in &workflow.nodes {
        let mut dependencies = BTreeSet::new();
        for dependency in &node.needs {
            if !node_ids.contains(dependency.as_str()) {
                return invalid(
                    workflow_id,
                    format!("node `{}` depends on missing node `{dependency}`", node.id),
                );
            }
            if !dependencies.insert(dependency.as_str()) {
                return invalid(
                    workflow_id,
                    format!("node `{}` repeats dependency `{dependency}`", node.id),
                );
            }
            if dependency == &node.id {
                return invalid(workflow_id, format!("node `{}` depends on itself", node.id));
            }
        }
    }
    Ok(())
}

pub(super) fn validate_workflow_bindings(
    workflow_id: &str,
    workflow: &StoredWorkflow,
    bindings: Option<&WorkflowExecutionBindings>,
) -> WorkflowRuntimeResult<()> {
    let Some(bindings) = bindings else {
        return Ok(());
    };
    if bindings.inputs.len() > MAX_WORKFLOW_BINDINGS {
        return invalid(
            workflow_id,
            format!("contains more than {MAX_WORKFLOW_BINDINGS} input bindings"),
        );
    }
    let node_ids = workflow
        .nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut targets = BTreeSet::new();
    for binding in &bindings.inputs {
        if !matches!(
            binding.kind.as_str(),
            "input_image" | "input_value" | "param"
        ) {
            return invalid(
                workflow_id,
                format!("unsupported input binding kind `{}`", binding.kind),
            );
        }
        validate_binding_node(workflow_id, &node_ids, &binding.node_id, "input binding")?;
        if !valid_field(&binding.workflow_param, MAX_BINDING_FIELD_BYTES, false)
            || !valid_field(&binding.target, MAX_BINDING_FIELD_BYTES, false)
        {
            return invalid(
                workflow_id,
                "input binding contains an invalid field".to_owned(),
            );
        }
        if !targets.insert((binding.node_id.as_str(), binding.target.as_str())) {
            return invalid(
                workflow_id,
                format!(
                    "node `{}` has duplicate input target `{}`",
                    binding.node_id, binding.target
                ),
            );
        }
    }
    if let Some(output) = &bindings.primary_output {
        let name = "primary output";
        validate_binding_node(workflow_id, &node_ids, &output.node_id, name)?;
        if output.kind != "node_result" {
            return invalid(
                workflow_id,
                format!("unsupported {name} kind `{}`", output.kind),
            );
        }
        if !valid_field(&output.output, MAX_BINDING_FIELD_BYTES, true) {
            return invalid(
                workflow_id,
                format!("{name} contains an invalid output name"),
            );
        }
    }
    if bindings.preview_output.is_none() && !bindings.preview_required_nodes.is_empty() {
        return invalid(
            workflow_id,
            "preview requirements need a preview output".to_owned(),
        );
    }
    Ok(())
}

fn validate_binding_node(
    workflow_id: &str,
    node_ids: &BTreeSet<&str>,
    node_id: &str,
    subject: &str,
) -> WorkflowRuntimeResult<()> {
    if !node_ids.contains(node_id) {
        return invalid(
            workflow_id,
            format!("{subject} references missing node `{node_id}`"),
        );
    }
    Ok(())
}

fn validate_yaml_value(
    workflow_id: &str,
    value: &serde_yaml::Value,
    depth: usize,
    value_nodes: &mut usize,
) -> WorkflowRuntimeResult<()> {
    *value_nodes += 1;
    if *value_nodes > MAX_WORKFLOW_VALUE_NODES {
        return invalid(
            workflow_id,
            "parameter values contain too many elements".to_owned(),
        );
    }
    if depth > MAX_WORKFLOW_VALUE_DEPTH {
        return invalid(
            workflow_id,
            "parameter values are nested too deeply".to_owned(),
        );
    }
    match value {
        serde_yaml::Value::Sequence(values) => {
            for value in values {
                validate_yaml_value(workflow_id, value, depth + 1, value_nodes)?;
            }
        }
        serde_yaml::Value::Mapping(values) => {
            for (key, value) in values {
                validate_yaml_value(workflow_id, key, depth + 1, value_nodes)?;
                validate_yaml_value(workflow_id, value, depth + 1, value_nodes)?;
            }
        }
        serde_yaml::Value::Tagged(tagged) => {
            validate_yaml_value(workflow_id, &tagged.value, depth + 1, value_nodes)?;
        }
        _ => {}
    }
    Ok(())
}

fn valid_identifier(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_bytes
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

pub(super) fn valid_field(value: &str, max_bytes: usize, allow_empty: bool) -> bool {
    (allow_empty || !value.trim().is_empty())
        && value.len() <= max_bytes
        && !value.chars().any(char::is_control)
}

fn invalid<T>(workflow_id: &str, reason: String) -> WorkflowRuntimeResult<T> {
    Err(WorkflowRuntimeError::InvalidWorkflow {
        workflow_id: workflow_id.to_owned(),
        reason,
    })
}
