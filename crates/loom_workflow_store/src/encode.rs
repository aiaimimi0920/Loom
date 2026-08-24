use std::collections::{BTreeSet, HashMap};

use serde_json::{Map as JsonMap, Value as JsonValue};

use crate::{
    helpers::clone_param_map,
    validate_art_id,
    validation::{validate_graph_budget, MAX_WORKFLOW_YAML_BYTES},
    WorkflowStoreError, WorkflowStoreResult, STICKER_USES, VISUAL_META_KEYS,
};

pub fn graph_json_to_workflow_yaml(
    graph: &JsonValue,
    workflow_name: Option<&str>,
    workflow_description: Option<&str>,
) -> WorkflowStoreResult<String> {
    validate_graph_budget(graph)?;
    let nodes = graph
        .get("nodes")
        .and_then(JsonValue::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let edges = graph
        .get("edges")
        .and_then(JsonValue::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();

    let mut edges_by_target: HashMap<&str, Vec<&JsonValue>> = HashMap::new();
    for edge in edges {
        if let Some(target) = edge.get("target").and_then(JsonValue::as_str) {
            edges_by_target.entry(target).or_default().push(edge);
        }
    }

    let mut yaml_nodes = Vec::new();

    for node in nodes {
        let node_id = node
            .get("id")
            .and_then(JsonValue::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                WorkflowStoreError::InvalidWorkflowGraph(
                    "every graph node requires a non-empty string id".to_owned(),
                )
            })?;

        let node_type = node
            .get("type")
            .and_then(JsonValue::as_str)
            .ok_or_else(|| {
                WorkflowStoreError::InvalidWorkflowGraph(format!(
                    "node `{node_id}` is missing canonical type `sticker` or `artNode`"
                ))
            })?;
        if !matches!(node_type, "sticker" | "artNode") {
            return Err(WorkflowStoreError::InvalidWorkflowGraph(format!(
                "node `{node_id}` has unsupported type `{node_type}`"
            )));
        }
        let is_sticker = node_type == "sticker";
        let art_id = if is_sticker {
            STICKER_USES.to_owned()
        } else {
            let art_id = node
                .get("data")
                .and_then(|data| data.get("artId"))
                .and_then(JsonValue::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    WorkflowStoreError::InvalidWorkflowGraph(format!(
                        "Art node `{node_id}` is missing data.artId"
                    ))
                })?;
            validate_art_id(art_id)?;
            art_id.to_owned()
        };

        let mut with_payload = clone_param_map(
            node.get("data")
                .and_then(|data| data.get("params"))
                .and_then(JsonValue::as_object),
        );
        let mut needs = Vec::new();
        let mut seen_needs = BTreeSet::new();

        for edge in edges_by_target.get(node_id).into_iter().flatten() {
            let Some(source_node) = edge.get("source").and_then(JsonValue::as_str) else {
                continue;
            };
            let source_handle = edge
                .get("sourceHandle")
                .and_then(JsonValue::as_str)
                .unwrap_or("output");
            let target_handle = edge
                .get("targetHandle")
                .and_then(JsonValue::as_str)
                .unwrap_or("input");

            with_payload.insert(
                target_handle.to_owned(),
                serde_json::json!(format!(
                    "${{{{ nodes.{source_node}.outputs.{source_handle} }}}}"
                )),
            );

            if seen_needs.insert(source_node.to_owned()) {
                needs.push(source_node.to_owned());
            }
        }

        let mut meta = JsonMap::new();
        meta.insert(
            "position".to_owned(),
            node.get("position")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({ "x": 0, "y": 0 })),
        );
        meta.insert("type".to_owned(), serde_json::json!(node_type));

        if let Some(label) = node
            .get("data")
            .and_then(|data| data.get("label"))
            .and_then(JsonValue::as_str)
        {
            meta.insert("label".to_owned(), serde_json::json!(label));
        }

        let mut size = JsonMap::new();
        if let Some(width) = node
            .get("data")
            .and_then(|data| data.get("w"))
            .and_then(JsonValue::as_f64)
        {
            size.insert("w".to_owned(), serde_json::json!(width));
        }
        if let Some(height) = node
            .get("data")
            .and_then(|data| data.get("h"))
            .and_then(JsonValue::as_f64)
        {
            size.insert("h".to_owned(), serde_json::json!(height));
        }
        if !size.is_empty() {
            meta.insert("size".to_owned(), JsonValue::Object(size));
        }

        if let Some(execution_type) = node
            .get("data")
            .and_then(|data| data.get("executionType"))
            .and_then(JsonValue::as_str)
        {
            meta.insert(
                "executionType".to_owned(),
                serde_json::json!(execution_type),
            );
        }

        if let Some(execution_config) = node
            .get("data")
            .and_then(|data| data.get("executionConfig"))
        {
            meta.insert("executionConfig".to_owned(), execution_config.clone());
        }

        for key in VISUAL_META_KEYS {
            if let Some(value) = node.get("data").and_then(|data| data.get(key)) {
                if !value.is_null() {
                    meta.insert(key.to_owned(), value.clone());
                }
            }
        }

        let mut yaml_node = JsonMap::new();
        yaml_node.insert("id".to_owned(), serde_json::json!(node_id));
        yaml_node.insert("uses".to_owned(), serde_json::json!(art_id));
        if !needs.is_empty() {
            yaml_node.insert("needs".to_owned(), serde_json::json!(needs));
        }
        if !with_payload.is_empty() {
            yaml_node.insert("with".to_owned(), JsonValue::Object(with_payload));
        }
        yaml_node.insert("meta".to_owned(), JsonValue::Object(meta));
        yaml_nodes.push(JsonValue::Object(yaml_node));
    }

    let mut workflow = JsonMap::new();
    workflow.insert(
        "name".to_owned(),
        serde_json::json!(workflow_name.unwrap_or("ArtWorkflow")),
    );
    if let Some(description) = workflow_description {
        if !description.trim().is_empty() {
            workflow.insert("description".to_owned(), serde_json::json!(description));
        }
    }
    workflow.insert("nodes".to_owned(), JsonValue::Array(yaml_nodes));

    let yaml = serde_yaml::to_string(&JsonValue::Object(workflow))?;
    if yaml.len() > MAX_WORKFLOW_YAML_BYTES {
        return Err(WorkflowStoreError::InvalidWorkflowGraph(format!(
            "encoded workflow exceeds the {MAX_WORKFLOW_YAML_BYTES} byte limit"
        )));
    }
    Ok(yaml)
}
