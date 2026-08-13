//! Workflow persistence services for the `loom.hook.v1` bridge.

use std::path::Path;

use loom_protocol::{
    HookEvent, HOOK_EVENT_CAPABILITIES_UPDATED, HOOK_EVENT_WORKFLOW_INSTANTIATED,
    HOOK_EVENT_WORKFLOW_UPDATED, HOOK_PROTOCOL_VERSION,
};
use serde_json::Value;
use thiserror::Error;

pub const HOOK_BRIDGE_PORT: u16 = 19820;

#[derive(Debug, Error)]
pub enum HookBridgeError {
    #[error("workflow store error: {0}")]
    WorkflowStore(#[from] loom_workflow_store::WorkflowStoreError),
    #[error("node `{node_id}` does not exist in workflow `{workflow_id}`")]
    NodeNotFound {
        workflow_id: String,
        node_id: String,
    },
}

pub type HookBridgeResult<T> = Result<T, HookBridgeError>;

/// Persist a workflow snapshot received from Hook and return its formal update event.
pub fn sync_workflow(
    workflow_root: &Path,
    workflow_id: &str,
    snapshot: &Value,
) -> HookBridgeResult<HookEvent> {
    let workflow_name = snapshot
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or(workflow_id);
    let workflow_description = snapshot.get("description").and_then(Value::as_str);
    let yaml = loom_workflow_store::graph_json_to_workflow_yaml(
        snapshot,
        Some(workflow_name),
        workflow_description,
    )?;
    loom_workflow_store::WorkflowStore::new(workflow_root).save_workflow(workflow_id, &yaml)?;

    Ok(hook_event(
        HOOK_EVENT_WORKFLOW_UPDATED,
        serde_json::json!({
            "workflowId": workflow_id,
            "overwrite": true,
            "data": snapshot,
        }),
    ))
}

/// Update one persisted workflow parameter and return the resulting formal snapshot event.
pub fn update_workflow_node(
    workflow_root: &Path,
    workflow_id: &str,
    node_id: &str,
    parameter_id: &str,
    value: Value,
) -> HookBridgeResult<HookEvent> {
    let store = loom_workflow_store::WorkflowStore::new(workflow_root);
    let yaml = store.load_workflow(workflow_id)?;
    let mut graph = loom_workflow_store::workflow_yaml_to_graph_json(&yaml)?;
    if !set_node_parameter(&mut graph, node_id, parameter_id, value) {
        return Err(HookBridgeError::NodeNotFound {
            workflow_id: workflow_id.to_owned(),
            node_id: node_id.to_owned(),
        });
    }

    let workflow_name = graph.get("name").and_then(Value::as_str);
    let workflow_description = graph.get("description").and_then(Value::as_str);
    let updated_yaml = loom_workflow_store::graph_json_to_workflow_yaml(
        &graph,
        workflow_name,
        workflow_description,
    )?;
    store.save_workflow(workflow_id, &updated_yaml)?;

    Ok(hook_event(
        HOOK_EVENT_WORKFLOW_UPDATED,
        serde_json::json!({
            "workflowId": workflow_id,
            "nodeId": node_id,
            "data": graph,
        }),
    ))
}

/// Persist the current Hook live graph and return the formal instantiation event.
pub fn instantiate_workflow(
    workflow_root: &Path,
    nodes: Vec<Value>,
    edges: Vec<Value>,
    mode: &str,
    workflow_id: Option<String>,
) -> HookBridgeResult<HookEvent> {
    let graph = serde_json::json!({
        "nodes": nodes,
        "edges": edges,
    });
    let yaml = loom_workflow_store::graph_json_to_workflow_yaml(
        &graph,
        Some("Hook live workflow"),
        workflow_id.as_deref(),
    )?;
    loom_workflow_store::WorkflowStore::new(workflow_root).save_workflow("hook-live", &yaml)?;

    Ok(workflow_instantiated_event(
        graph["nodes"].as_array().cloned().unwrap_or_default(),
        graph["edges"].as_array().cloned().unwrap_or_default(),
        mode,
        workflow_id,
    ))
}

#[must_use]
pub fn workflow_instantiated_event(
    nodes: Vec<Value>,
    edges: Vec<Value>,
    mode: &str,
    workflow_id: Option<String>,
) -> HookEvent {
    hook_event(
        HOOK_EVENT_WORKFLOW_INSTANTIATED,
        serde_json::json!({
            "mode": mode,
            "workflowId": workflow_id,
            "nodes": nodes,
            "edges": edges,
        }),
    )
}

#[must_use]
pub fn capabilities_updated_event() -> HookEvent {
    hook_event(HOOK_EVENT_CAPABILITIES_UPDATED, serde_json::json!({}))
}

fn hook_event(method: &str, params: Value) -> HookEvent {
    HookEvent {
        protocol_version: HOOK_PROTOCOL_VERSION.to_owned(),
        method: method.to_owned(),
        params,
    }
}

fn set_node_parameter(graph: &mut Value, node_id: &str, parameter_id: &str, value: Value) -> bool {
    let Some(nodes) = graph.get_mut("nodes").and_then(Value::as_array_mut) else {
        return false;
    };
    let Some(node) = nodes
        .iter_mut()
        .find(|node| node.get("id").and_then(Value::as_str) == Some(node_id))
    else {
        return false;
    };
    let Some(node_object) = node.as_object_mut() else {
        return false;
    };
    let data = node_object
        .entry("data")
        .or_insert_with(|| serde_json::json!({}));
    if !data.is_object() {
        *data = serde_json::json!({});
    }
    let parameters = data
        .as_object_mut()
        .expect("data was normalized to an object")
        .entry("params")
        .or_insert_with(|| serde_json::json!({}));
    if !parameters.is_object() {
        *parameters = serde_json::json!({});
    }
    parameters
        .as_object_mut()
        .expect("params was normalized to an object")
        .insert(parameter_id.to_owned(), value);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("loom-hook-bridge-{label}-{nonce}"))
    }

    #[test]
    fn instantiation_uses_only_the_formal_hook_event() {
        let root = temp_root("instantiate");
        let event = instantiate_workflow(
            &root,
            vec![serde_json::json!({
                "id": "node-a",
                "type": "artNode",
                "data": { "artId": "neuro.official/example-echo", "params": {} }
            })],
            Vec::new(),
            "reference",
            Some("workflow-a".to_owned()),
        )
        .expect("instantiate workflow");

        assert_eq!(event.protocol_version, HOOK_PROTOCOL_VERSION);
        assert_eq!(event.method, HOOK_EVENT_WORKFLOW_INSTANTIATED);
        assert_eq!(event.params["workflowId"], "workflow-a");
        assert!(loom_workflow_store::WorkflowStore::new(&root)
            .load_workflow("hook-live")
            .is_ok());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn node_update_persists_and_emits_a_formal_snapshot() {
        let root = temp_root("update");
        instantiate_workflow(
            &root,
            vec![serde_json::json!({
                "id": "node-a",
                "type": "artNode",
                "data": { "artId": "neuro.official/example-echo", "params": { "strength": 1 } }
            })],
            Vec::new(),
            "reference",
            None,
        )
        .expect("seed workflow");

        let event = update_workflow_node(
            &root,
            "hook-live",
            "node-a",
            "strength",
            serde_json::json!(2),
        )
        .expect("update workflow");

        assert_eq!(event.method, HOOK_EVENT_WORKFLOW_UPDATED);
        assert_eq!(event.params["nodeId"], "node-a");
        assert_eq!(
            event.params["data"]["nodes"][0]["data"]["params"]["strength"],
            2
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
