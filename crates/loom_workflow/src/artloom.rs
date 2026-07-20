use std::collections::{BTreeMap, BTreeSet};

use loom_core::WorkflowId;
use serde::Deserialize;
use thiserror::Error;

use crate::{WorkflowEdge, WorkflowGraph, WorkflowNode};

/// Converted ArtLoom workflow plus descriptive metadata that does not belong
/// in Loom's runtime graph contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConvertedArtLoomWorkflow {
    pub name: Option<String>,
    pub description: Option<String>,
    pub graph: WorkflowGraph,
}

#[derive(Debug, Error)]
pub enum ArtLoomConversionError {
    #[error("failed to parse ArtLoom workflow YAML: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("ArtLoom workflow has no nodes")]
    MissingNodes,
    #[error("ArtLoom workflow node is missing id")]
    MissingNodeId,
    #[error("ArtLoom workflow has multiple root nodes: {0}")]
    MultipleRoots(String),
    #[error("converted ArtLoom workflow is invalid: {0}")]
    InvalidWorkflow(#[from] crate::WorkflowError),
}

pub type ArtLoomConversionResult<T> = Result<T, ArtLoomConversionError>;

#[derive(Debug, Deserialize)]
struct ArtLoomWorkflowDocument {
    name: Option<String>,
    description: Option<String>,
    #[serde(default)]
    nodes: Vec<ArtLoomNode>,
}

#[derive(Debug, Deserialize)]
struct ArtLoomNode {
    id: Option<String>,
    #[serde(default = "default_uses")]
    uses: String,
    #[serde(default)]
    needs: Vec<String>,
    #[serde(default, rename = "with")]
    with_payload: BTreeMap<String, serde_yaml::Value>,
}

fn default_uses() -> String {
    "unknown".to_owned()
}

/// Convert the selected ArtLoom YAML workflow shape into Loom's native
/// workflow graph. Gateway/provider routing and ArtHook UI behavior are not
/// migrated here; ArtLoom `uses` values become Loom actor ids.
pub fn convert_artloom_yaml(
    workflow_id: impl Into<String>,
    yaml_content: &str,
) -> ArtLoomConversionResult<ConvertedArtLoomWorkflow> {
    let document: ArtLoomWorkflowDocument = serde_yaml::from_str(yaml_content)?;
    if document.nodes.is_empty() {
        return Err(ArtLoomConversionError::MissingNodes);
    }

    let workflow_id = WorkflowId::new(workflow_id);
    let mut nodes = BTreeMap::new();
    let mut edges = Vec::new();
    let mut edge_keys = BTreeSet::new();

    for node in document.nodes {
        let node_id = node
            .id
            .filter(|id| !id.trim().is_empty())
            .ok_or(ArtLoomConversionError::MissingNodeId)?;

        for source in node.needs {
            push_edge(&mut edges, &mut edge_keys, source, node_id.clone());
        }

        for value in node.with_payload.values() {
            collect_reference_edges(value, &node_id, &mut edges, &mut edge_keys);
        }

        nodes.insert(node_id.clone(), WorkflowNode::agent(node_id, node.uses));
    }

    let entry_node = entry_node(&nodes, &edges)?;
    let graph = WorkflowGraph {
        id: workflow_id,
        entry_node,
        nodes,
        edges,
    };
    graph.validate()?;

    Ok(ConvertedArtLoomWorkflow {
        name: document.name,
        description: document.description,
        graph,
    })
}

fn push_edge(
    edges: &mut Vec<WorkflowEdge>,
    edge_keys: &mut BTreeSet<(String, String)>,
    from: String,
    to: String,
) {
    if edge_keys.insert((from.clone(), to.clone())) {
        edges.push(WorkflowEdge { from, to });
    }
}

fn collect_reference_edges(
    value: &serde_yaml::Value,
    target_node: &str,
    edges: &mut Vec<WorkflowEdge>,
    edge_keys: &mut BTreeSet<(String, String)>,
) {
    match value {
        serde_yaml::Value::String(raw) => {
            if let Some(source) = parse_artloom_output_reference(raw) {
                push_edge(edges, edge_keys, source, target_node.to_owned());
            }
        }
        serde_yaml::Value::Sequence(values) => {
            for value in values {
                collect_reference_edges(value, target_node, edges, edge_keys);
            }
        }
        serde_yaml::Value::Mapping(values) => {
            for value in values.values() {
                collect_reference_edges(value, target_node, edges, edge_keys);
            }
        }
        _ => {}
    }
}

fn parse_artloom_output_reference(raw: &str) -> Option<String> {
    let inner = raw.trim().strip_prefix("${{")?.strip_suffix("}}")?.trim();
    let rest = inner.strip_prefix("nodes.")?;
    let (source, handle) = rest.split_once(".outputs.")?;
    if source.is_empty() || handle.is_empty() {
        return None;
    }
    Some(source.to_owned())
}

fn entry_node(
    nodes: &BTreeMap<String, WorkflowNode>,
    edges: &[WorkflowEdge],
) -> ArtLoomConversionResult<String> {
    let targets: BTreeSet<&str> = edges.iter().map(|edge| edge.to.as_str()).collect();
    let roots: Vec<_> = nodes
        .keys()
        .filter(|node_id| !targets.contains(node_id.as_str()))
        .cloned()
        .collect();

    match roots.as_slice() {
        [root] => Ok(root.clone()),
        [] => nodes
            .keys()
            .next()
            .cloned()
            .ok_or(ArtLoomConversionError::MissingNodes),
        roots => Err(ArtLoomConversionError::MultipleRoots(roots.join(", "))),
    }
}
