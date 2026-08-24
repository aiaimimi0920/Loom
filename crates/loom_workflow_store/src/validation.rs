use serde_json::Value as JsonValue;
use serde_yaml::Value as YamlValue;

use crate::{WorkflowStoreError, WorkflowStoreResult, STICKER_USES};

pub(super) const MAX_WORKFLOW_YAML_BYTES: usize = 8 * 1024 * 1024;
pub(super) const MAX_WORKFLOW_INDEX_BYTES: usize = 4 * 1024 * 1024;
pub(super) const MAX_WORKFLOW_NODES: usize = 4_096;
pub(super) const MAX_WORKFLOW_EDGES: usize = 16_384;
pub(super) const MAX_STORED_WORKFLOWS: usize = 4_096;
pub(super) const MAX_WORKFLOW_VALUE_COUNT: usize = 100_000;
pub(super) const MAX_WORKFLOW_DEPTH: usize = 64;
pub(super) const MAX_WORKFLOW_ID_BYTES: usize = 240;

pub(super) fn parse_workflow_yaml(yaml: &str) -> WorkflowStoreResult<YamlValue> {
    if yaml.len() > MAX_WORKFLOW_YAML_BYTES {
        return Err(WorkflowStoreError::InvalidWorkflowYaml(format!(
            "document exceeds the {MAX_WORKFLOW_YAML_BYTES} byte limit"
        )));
    }
    let parsed: YamlValue = serde_yaml::from_str(yaml)?;
    validate_yaml_structure(&parsed)?;
    let node_count = parsed
        .get("nodes")
        .and_then(YamlValue::as_sequence)
        .map_or(0, Vec::len);
    if node_count > MAX_WORKFLOW_NODES {
        return Err(WorkflowStoreError::InvalidWorkflowYaml(format!(
            "nodes exceed the {MAX_WORKFLOW_NODES} item limit"
        )));
    }
    Ok(parsed)
}

pub(super) fn validate_graph_budget(graph: &JsonValue) -> WorkflowStoreResult<()> {
    loom_security::json::ensure_within_limits(
        graph,
        "workflow graph",
        MAX_WORKFLOW_YAML_BYTES,
        MAX_WORKFLOW_DEPTH,
    )
    .map_err(WorkflowStoreError::InvalidWorkflowGraph)?;
    let nodes = graph
        .get("nodes")
        .and_then(JsonValue::as_array)
        .map_or(0, Vec::len);
    let edges = graph
        .get("edges")
        .and_then(JsonValue::as_array)
        .map_or(0, Vec::len);
    if nodes > MAX_WORKFLOW_NODES {
        return Err(WorkflowStoreError::InvalidWorkflowGraph(format!(
            "nodes exceed the {MAX_WORKFLOW_NODES} item limit"
        )));
    }
    if edges > MAX_WORKFLOW_EDGES {
        return Err(WorkflowStoreError::InvalidWorkflowGraph(format!(
            "edges exceed the {MAX_WORKFLOW_EDGES} item limit"
        )));
    }
    Ok(())
}

fn validate_yaml_structure(root: &YamlValue) -> WorkflowStoreResult<()> {
    let mut pending = vec![(root, 0_usize)];
    let mut values = 0_usize;
    while let Some((value, depth)) = pending.pop() {
        values = values.saturating_add(1);
        if values > MAX_WORKFLOW_VALUE_COUNT {
            return Err(WorkflowStoreError::InvalidWorkflowYaml(format!(
                "document exceeds the {MAX_WORKFLOW_VALUE_COUNT} value limit"
            )));
        }
        if depth > MAX_WORKFLOW_DEPTH {
            return Err(WorkflowStoreError::InvalidWorkflowYaml(format!(
                "document exceeds the nesting limit of {MAX_WORKFLOW_DEPTH} levels"
            )));
        }
        match value {
            YamlValue::Sequence(sequence) => {
                pending.extend(sequence.iter().map(|value| (value, depth + 1)));
            }
            YamlValue::Mapping(mapping) => {
                for (key, value) in mapping {
                    pending.push((key, depth + 1));
                    pending.push((value, depth + 1));
                }
            }
            YamlValue::Tagged(tagged) => pending.push((&tagged.value, depth + 1)),
            _ => {}
        }
    }
    Ok(())
}

pub(super) fn validate_workflow_id(id: &str) -> WorkflowStoreResult<()> {
    if id.trim().is_empty()
        || id.len() > MAX_WORKFLOW_ID_BYTES
        || id.contains("..")
        || id.contains('/')
        || id.contains('\\')
        || id.contains(':')
        || id.chars().any(char::is_control)
        || id.ends_with(['.', ' '])
        || is_windows_reserved_device_name(id)
    {
        return Err(WorkflowStoreError::InvalidWorkflowId(id.to_owned()));
    }
    Ok(())
}

fn validate_qualified_art_id(value: &str) -> WorkflowStoreResult<()> {
    let mut parts = value.split('/');
    let publisher = parts.next().unwrap_or_default();
    let package = parts.next().unwrap_or_default();
    if parts.next().is_some() || !safe_package_segment(publisher) || !safe_package_segment(package)
    {
        return Err(WorkflowStoreError::InvalidWorkflowGraph(format!(
            "Art identity `{value}` must be publisher-qualified as `<publisher>/<id>`"
        )));
    }
    Ok(())
}

pub fn validate_workflow_uses(value: &str) -> WorkflowStoreResult<()> {
    if value == STICKER_USES {
        return Ok(());
    }
    validate_art_id(value)
}

pub fn validate_art_id(value: &str) -> WorkflowStoreResult<()> {
    if is_native_art_id(value) {
        return Ok(());
    }
    validate_qualified_art_id(value)
}

fn is_native_art_id(value: &str) -> bool {
    matches!(
        value,
        "core.image.pixelate"
            | "core.image.blur"
            | "core.image.grayscale"
            | "core.image.brightness"
            | "core.image.contrast"
            | "core.image.invert"
    )
}

fn safe_package_segment(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        && !value.starts_with('.')
        && !value.ends_with('.')
        && !value.contains("..")
        && !is_windows_reserved_device_name(value)
}

fn is_windows_reserved_device_name(value: &str) -> bool {
    let base = value
        .split('.')
        .next()
        .unwrap_or(value)
        .trim_end_matches(['.', ' '])
        .to_ascii_uppercase();
    matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || base
            .strip_prefix("COM")
            .and_then(|suffix| suffix.parse::<u8>().ok())
            .is_some_and(|number| (1..=9).contains(&number))
        || base
            .strip_prefix("LPT")
            .and_then(|suffix| suffix.parse::<u8>().ok())
            .is_some_and(|number| (1..=9).contains(&number))
}
