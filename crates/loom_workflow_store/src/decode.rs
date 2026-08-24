use serde_json::{Map as JsonMap, Value as JsonValue};
use serde_yaml::Value as YamlValue;

use crate::{
    helpers::{clone_param_map, parse_workflow_output_reference},
    validate_workflow_uses,
    validation::parse_workflow_yaml,
    WorkflowStoreError, WorkflowStoreResult, STICKER_USES, VISUAL_META_KEYS,
};

/// Collect the art ids referenced by a workflow — every node's `uses` value,
/// excluding the sticker placeholder. Used to resolve a workflow art's
/// dependent arts (phase-1 recursive install). Returns unique ids in order.
pub fn collect_workflow_uses(yaml: &str) -> WorkflowStoreResult<Vec<String>> {
    let parsed = parse_workflow_yaml(yaml)?;
    let mut seen = std::collections::BTreeSet::new();
    let mut uses = Vec::new();
    if let Some(nodes) = parsed.get("nodes").and_then(YamlValue::as_sequence) {
        for node in nodes {
            if let Some(value) = node.get("uses").and_then(YamlValue::as_str) {
                if value == STICKER_USES {
                    continue;
                }
                validate_workflow_uses(value)
                    .map_err(|error| WorkflowStoreError::InvalidWorkflowYaml(error.to_string()))?;
                if seen.insert(value.to_owned()) {
                    uses.push(value.to_owned());
                }
            }
        }
    }
    Ok(uses)
}

pub fn workflow_yaml_to_graph_json(yaml: &str) -> WorkflowStoreResult<JsonValue> {
    let parsed = parse_workflow_yaml(yaml)?;
    let yaml_nodes = parsed
        .get("nodes")
        .and_then(YamlValue::as_sequence)
        .ok_or_else(|| WorkflowStoreError::InvalidWorkflowYaml("nodes array missing".to_owned()))?;

    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    for yaml_node in yaml_nodes {
        let node_id = yaml_node
            .get("id")
            .and_then(YamlValue::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                WorkflowStoreError::InvalidWorkflowYaml(
                    "every workflow node requires a non-empty id".to_owned(),
                )
            })?;

        let uses = yaml_node
            .get("uses")
            .and_then(YamlValue::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                WorkflowStoreError::InvalidWorkflowYaml(format!("node `{node_id}` is missing uses"))
            })?;
        validate_workflow_uses(uses)
            .map_err(|error| WorkflowStoreError::InvalidWorkflowYaml(error.to_string()))?;
        let meta = yaml_node.get("meta");
        let node_type = if uses == STICKER_USES {
            "sticker"
        } else {
            "artNode"
        };

        let mut data = JsonMap::new();
        let mut params = JsonMap::new();
        data.insert(
            "label".to_owned(),
            serde_json::json!(meta
                .and_then(|value| value.get("label"))
                .and_then(YamlValue::as_str)
                .unwrap_or(uses)),
        );

        if let Some(with_map) = yaml_node.get("with").and_then(YamlValue::as_mapping) {
            for (key, value) in with_map {
                let Some(target_handle) = key.as_str() else {
                    continue;
                };

                let json_value = serde_json::to_value(value).unwrap_or(JsonValue::Null);
                if let Some(raw) = value.as_str() {
                    if let Some((source_node, source_handle)) = parse_workflow_output_reference(raw)
                    {
                        edges.push(serde_json::json!({
                            "id": format!("e-{source_node}-{source_handle}-{node_id}-{target_handle}"),
                            "source": source_node,
                            "target": node_id,
                            "sourceHandle": source_handle,
                            "targetHandle": target_handle,
                            "animated": true,
                            "style": { "stroke": "#7c5dfa" }
                        }));
                        continue;
                    }
                }

                params.insert(target_handle.to_owned(), json_value);
            }
        }

        data.insert(
            "params".to_owned(),
            JsonValue::Object(clone_param_map(Some(&params))),
        );

        if let Some(execution_type) = meta
            .and_then(|value| value.get("executionType"))
            .and_then(YamlValue::as_str)
        {
            data.insert(
                "executionType".to_owned(),
                serde_json::json!(execution_type),
            );
        }

        if let Some(execution_config) = meta.and_then(|value| value.get("executionConfig")) {
            data.insert(
                "executionConfig".to_owned(),
                serde_json::to_value(execution_config).unwrap_or(JsonValue::Null),
            );
        }

        if let Some(width) = meta
            .and_then(|value| value.get("size"))
            .and_then(|value| value.get("w"))
            .and_then(YamlValue::as_f64)
        {
            data.insert("w".to_owned(), serde_json::json!(width));
        }

        if let Some(height) = meta
            .and_then(|value| value.get("size"))
            .and_then(|value| value.get("h"))
            .and_then(YamlValue::as_f64)
        {
            data.insert("h".to_owned(), serde_json::json!(height));
        }

        for key in VISUAL_META_KEYS {
            if let Some(value) = meta.and_then(|meta| meta.get(key)) {
                data.insert(
                    key.to_owned(),
                    serde_json::to_value(value).unwrap_or(JsonValue::Null),
                );
            }
        }

        if node_type != "sticker" {
            data.insert("artId".to_owned(), serde_json::json!(uses));
        }

        let position = meta
            .and_then(|value| value.get("position"))
            .and_then(|value| serde_json::to_value(value).ok())
            .unwrap_or_else(|| serde_json::json!({ "x": 0, "y": 0 }));

        nodes.push(serde_json::json!({
            "id": node_id,
            "type": node_type,
            "position": position,
            "data": JsonValue::Object(data)
        }));
    }

    let mut graph = JsonMap::new();
    if let Some(name) = parsed.get("name").and_then(YamlValue::as_str) {
        graph.insert("name".to_owned(), serde_json::json!(name));
    }
    if let Some(description) = parsed.get("description").and_then(YamlValue::as_str) {
        graph.insert("description".to_owned(), serde_json::json!(description));
    }
    graph.insert("nodes".to_owned(), JsonValue::Array(nodes));
    graph.insert("edges".to_owned(), JsonValue::Array(edges));
    Ok(JsonValue::Object(graph))
}
