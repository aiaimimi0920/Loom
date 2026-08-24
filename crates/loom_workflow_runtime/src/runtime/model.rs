//! Stored workflow schema, loading, and public node-to-tool projection.

use super::*;

#[derive(Debug, Deserialize, Clone)]
pub(super) struct StoredWorkflow {
    #[serde(default)]
    pub(super) nodes: Vec<StoredWorkflowNode>,
}

#[derive(Debug, Deserialize, Clone)]
pub(super) struct StoredWorkflowNode {
    pub(super) id: String,
    pub(super) uses: String,
    #[serde(default)]
    pub(super) needs: Vec<String>,
    #[serde(rename = "with", default)]
    pub(super) params: BTreeMap<String, serde_yaml::Value>,
    #[serde(default)]
    pub(super) meta: Option<StoredWorkflowNodeMeta>,
}

#[derive(Debug, Deserialize, Clone)]
pub(super) struct StoredWorkflowNodeMeta {
    #[serde(default)]
    pub(super) src: Option<String>,
    #[serde(default, rename = "previewSrc")]
    pub(super) preview_src: Option<String>,
}

pub(super) fn load_stored_workflow(
    workflow_store: &WorkflowStore,
    workflow_id: &str,
) -> WorkflowRuntimeResult<StoredWorkflow> {
    let workflow_yaml = workflow_store.load_workflow(workflow_id)?;
    if workflow_yaml.len() > MAX_WORKFLOW_YAML_BYTES {
        return Err(WorkflowRuntimeError::InvalidWorkflow {
            workflow_id: workflow_id.to_owned(),
            reason: format!("YAML exceeds {MAX_WORKFLOW_YAML_BYTES} bytes"),
        });
    }
    let workflow = serde_yaml::from_str(&workflow_yaml).map_err(|source| {
        WorkflowRuntimeError::WorkflowYaml {
            workflow_id: workflow_id.to_owned(),
            source,
        }
    })?;
    validate_workflow(workflow_id, &workflow)?;
    Ok(workflow)
}

pub fn workflow_node_tool_ids(
    workflow_store: &WorkflowStore,
    workflow_id: &str,
) -> WorkflowRuntimeResult<BTreeMap<String, String>> {
    let workflow = load_stored_workflow(workflow_store, workflow_id)?;
    Ok(workflow
        .nodes
        .into_iter()
        .map(|node| (node.id, node.uses))
        .collect())
}
