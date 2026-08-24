//! Named/default output extraction and MCP-style content normalization.

use super::*;

pub(super) fn select_workflow_output(
    workflow_id: &str,
    nodes: &[StoredWorkflowNode],
    primary_output: Option<&WorkflowOutputBinding>,
    results: &HashMap<String, JsonValue>,
) -> JsonValue {
    if let Some(primary_output) = primary_output {
        if let Some(output) = select_bound_workflow_output(primary_output, results) {
            return output;
        }
    }

    let depended_on = nodes
        .iter()
        .flat_map(|node| node.needs.iter().cloned())
        .collect::<BTreeSet<_>>();
    if let Some(result) = nodes
        .iter()
        .rev()
        .filter(|node| !depended_on.contains(&node.id))
        .find_map(|node| results.get(&node.id))
    {
        return result.clone();
    }
    if let Some(result) = nodes.iter().rev().find_map(|node| results.get(&node.id)) {
        return result.clone();
    }

    text_content_response(&format!("workflow `{workflow_id}` completed"))
}

pub(super) fn select_bound_workflow_output(
    binding: &WorkflowOutputBinding,
    results: &HashMap<String, JsonValue>,
) -> Option<JsonValue> {
    let result = results.get(&binding.node_id)?;
    if !binding.output.trim().is_empty() {
        if let Some(output) = extract_named_output(result, &binding.output) {
            return Some(value_to_content_response(output));
        }
    }
    (binding.kind == "node_result").then(|| result.clone())
}

pub(super) fn resolve_workflow_reference(
    raw_value: &str,
    results: &HashMap<String, JsonValue>,
) -> Option<JsonValue> {
    let trimmed = raw_value.trim();
    if !trimmed.starts_with("${{") || !trimmed.ends_with("}}") {
        return None;
    }

    let inner = trimmed
        .trim_start_matches("${{")
        .trim_end_matches("}}")
        .trim();
    let parts = inner.split('.').collect::<Vec<_>>();
    if parts.len() != 4 || parts[0] != "nodes" || parts[2] != "outputs" {
        return None;
    }
    let node_id = parts[1];
    let output = parts[3];
    results
        .get(node_id)
        .and_then(|result| extract_named_output(result, output))
}

pub(super) fn extract_named_output(result: &JsonValue, output: &str) -> Option<JsonValue> {
    let result = unwrap_nested_result(result);
    if let Some(value) = result.get(output).cloned() {
        return Some(value);
    }
    match output {
        "image" | "data" | "output_base64" | "input" | "input_base64" => {
            extract_image_output(result).map(JsonValue::String)
        }
        "text" | "output_text" => extract_text_output(result).map(JsonValue::String),
        _ => extract_default_output(result),
    }
}

pub(super) fn extract_default_output(result: &JsonValue) -> Option<JsonValue> {
    extract_image_output(result)
        .map(JsonValue::String)
        .or_else(|| extract_text_output(result).map(JsonValue::String))
        .or_else(|| {
            let result = unwrap_nested_result(result);
            result
                .get("output_base64")
                .cloned()
                .or_else(|| result.get("output_text").cloned())
        })
}

pub(super) fn extract_image_output(value: &JsonValue) -> Option<String> {
    let value = unwrap_nested_result(value);
    if let Some(output) = value.get("output_base64").and_then(JsonValue::as_str) {
        return Some(output.to_owned());
    }
    if let Some(data) = value.get("data").and_then(JsonValue::as_str) {
        if is_image_like(data) {
            return Some(data.to_owned());
        }
    }
    value
        .get("content")
        .and_then(JsonValue::as_array)
        .and_then(|content| {
            content.iter().find_map(|entry| {
                if entry.get("type").and_then(JsonValue::as_str) == Some("image") {
                    entry
                        .get("data")
                        .and_then(JsonValue::as_str)
                        .map(str::to_owned)
                } else {
                    None
                }
            })
        })
}

pub(super) fn extract_text_output(value: &JsonValue) -> Option<String> {
    let value = unwrap_nested_result(value);
    if let Some(output) = value.get("output_text").and_then(JsonValue::as_str) {
        return Some(output.to_owned());
    }
    if let Some(text) = value.get("text").and_then(JsonValue::as_str) {
        return Some(text.to_owned());
    }
    value
        .get("content")
        .and_then(JsonValue::as_array)
        .and_then(|content| {
            content.iter().find_map(|entry| {
                if entry.get("type").and_then(JsonValue::as_str) == Some("text") {
                    entry
                        .get("text")
                        .and_then(JsonValue::as_str)
                        .map(str::to_owned)
                } else {
                    None
                }
            })
        })
}

pub(super) fn unwrap_nested_result(value: &JsonValue) -> &JsonValue {
    value.get("result").unwrap_or(value)
}

pub(super) fn value_to_content_response(value: JsonValue) -> JsonValue {
    if let Some(image) = json_value_as_image(&value) {
        image_content_response(&image, "image/png")
    } else if let Some(text) = value.as_str() {
        text_content_response(text)
    } else {
        text_content_response(&value.to_string())
    }
}
