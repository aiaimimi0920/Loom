//! Per-node and cumulative result budgets prevent bounded workflows retaining unbounded tool output.

use super::*;

const MAX_NODE_RESULT_BYTES: usize = 64 * 1024 * 1024;
const MAX_WORKFLOW_RESULT_BYTES: usize = 256 * 1024 * 1024;
const MAX_RESULT_VALUE_NODES: usize = 1_000_000;
pub(super) const MAX_RESULT_VALUE_DEPTH: usize = 128;

pub(super) fn reserve_workflow_result(
    workflow_id: &str,
    node_id: &str,
    result: &JsonValue,
    stored_bytes: usize,
) -> WorkflowRuntimeResult<usize> {
    let result_bytes = bounded_json_size(workflow_id, node_id, result)?;
    let total = stored_bytes.checked_add(result_bytes).ok_or_else(|| {
        resource_limit(
            workflow_id,
            format!("result size overflow while retaining node `{node_id}`"),
        )
    })?;
    if total > MAX_WORKFLOW_RESULT_BYTES {
        return Err(resource_limit(
            workflow_id,
            format!("retained results exceed {MAX_WORKFLOW_RESULT_BYTES} bytes"),
        ));
    }
    Ok(total)
}

fn bounded_json_size(
    workflow_id: &str,
    node_id: &str,
    root: &JsonValue,
) -> WorkflowRuntimeResult<usize> {
    let mut bytes = 0usize;
    let mut visited = 0usize;
    let mut stack = vec![(root, 0usize)];
    while let Some((value, depth)) = stack.pop() {
        visited += 1;
        if visited > MAX_RESULT_VALUE_NODES || depth > MAX_RESULT_VALUE_DEPTH {
            return Err(resource_limit(
                workflow_id,
                format!("node `{node_id}` returned overly complex JSON"),
            ));
        }
        let own_bytes = match value {
            JsonValue::Null | JsonValue::Bool(_) | JsonValue::Number(_) => 16,
            JsonValue::String(value) => value.len(),
            JsonValue::Array(values) => {
                stack.extend(values.iter().map(|value| (value, depth + 1)));
                values.len()
            }
            JsonValue::Object(values) => {
                stack.extend(values.values().map(|value| (value, depth + 1)));
                values.keys().map(String::len).sum()
            }
        };
        bytes = bytes.saturating_add(own_bytes);
        if bytes > MAX_NODE_RESULT_BYTES {
            return Err(resource_limit(
                workflow_id,
                format!("node `{node_id}` result exceeds {MAX_NODE_RESULT_BYTES} bytes"),
            ));
        }
    }
    Ok(bytes)
}

fn resource_limit(workflow_id: &str, reason: String) -> WorkflowRuntimeError {
    WorkflowRuntimeError::ResourceLimit {
        workflow_id: workflow_id.to_owned(),
        reason,
    }
}
