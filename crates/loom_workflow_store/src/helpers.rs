use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Map as JsonMap, Value as JsonValue};
use serde_yaml::Value as YamlValue;

use crate::{
    validation::parse_workflow_yaml, WorkflowMetadata, WorkflowStoreResult, LIVE_WORKFLOW_FILE,
};

#[must_use]
pub fn workflow_file_name(id: &str) -> String {
    if is_live_workflow_id(id) {
        LIVE_WORKFLOW_FILE.to_owned()
    } else {
        format!("{id}.yaml")
    }
}

pub(super) fn is_live_workflow_id(id: &str) -> bool {
    id == "hook-live"
}

pub(super) fn now_string() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().to_string())
        .unwrap_or_else(|_| "0".to_owned())
}

pub(super) fn workflow_details_from_yaml(
    yaml: &str,
) -> WorkflowStoreResult<(Option<String>, usize)> {
    let parsed = parse_workflow_yaml(yaml)?;
    let name = parsed
        .get("name")
        .and_then(YamlValue::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned);
    let node_count = parsed
        .get("nodes")
        .and_then(YamlValue::as_sequence)
        .map_or(0, Vec::len);
    Ok((name, node_count))
}

pub(super) fn workflow_details_from_graph(graph: &JsonValue) -> (Option<String>, usize) {
    let name = graph
        .get("name")
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned);
    let node_count = graph
        .get("nodes")
        .and_then(JsonValue::as_array)
        .map_or(0, Vec::len);
    (name, node_count)
}

pub(super) fn sort_metadata(workflows: &mut [WorkflowMetadata]) {
    workflows.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| left.id.cmp(&right.id))
    });
}

pub(super) fn clone_param_map(
    params: Option<&JsonMap<String, JsonValue>>,
) -> JsonMap<String, JsonValue> {
    params.cloned().unwrap_or_default()
}

pub(super) fn parse_workflow_output_reference(value: &str) -> Option<(String, String)> {
    let inner = value.trim().strip_prefix("${{")?.strip_suffix("}}")?.trim();
    let rest = inner.strip_prefix("nodes.")?;
    let (source, handle) = rest.split_once(".outputs.")?;

    if source.is_empty() || handle.is_empty() {
        return None;
    }

    Some((source.to_owned(), handle.to_owned()))
}
