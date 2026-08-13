//! Workflow persistence and graph codec contracts for Loom.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{Map as JsonMap, Value as JsonValue};
use serde_yaml::Value as YamlValue;
use thiserror::Error;

const WORKFLOW_INDEX_FILE: &str = "workflow_index.json";
const LIVE_WORKFLOW_FILE: &str = "latest.yaml";
const STICKER_USES: &str = "__sticker__";
const VISUAL_META_KEYS: [&str; 7] = [
    "src",
    "previewSrc",
    "minified",
    "savedRect",
    "cropOffset",
    "opacityNormal",
    "opacityMini",
];

#[derive(Debug, Error)]
pub enum WorkflowStoreError {
    #[error("invalid workflow id `{0}`")]
    InvalidWorkflowId(String),
    #[error("workflow `{0}` was not found")]
    NotFound(String),
    #[error("workflow YAML is invalid: {0}")]
    InvalidWorkflowYaml(String),
    #[error("workflow graph is invalid: {0}")]
    InvalidWorkflowGraph(String),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),
}

pub type WorkflowStoreResult<T> = Result<T, WorkflowStoreError>;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowMetadata {
    pub id: String,
    pub name: String,
    pub node_count: usize,
    pub updated_at: String,
}

#[derive(Clone, Debug)]
pub struct WorkflowStore {
    root: PathBuf,
}

impl WorkflowStore {
    #[must_use]
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    pub fn save_workflow(&self, id: &str, yaml: &str) -> WorkflowStoreResult<WorkflowMetadata> {
        workflow_yaml_to_graph_json(yaml)?;
        self.ensure_root()?;
        let path = self.workflow_path(id)?;
        fs::write(&path, yaml)?;

        let metadata = WorkflowMetadata {
            id: id.to_owned(),
            name: workflow_name_from_yaml(yaml).unwrap_or_else(|| id.to_owned()),
            node_count: count_nodes_in_yaml(yaml),
            updated_at: now_string(),
        };

        if !is_live_workflow_id(id) {
            let mut workflows = self.read_index()?;
            if let Some(existing) = workflows.iter_mut().find(|workflow| workflow.id == id) {
                *existing = metadata.clone();
            } else {
                workflows.push(metadata.clone());
            }
            sort_metadata(&mut workflows);
            self.write_index(&workflows)?;
        }

        Ok(metadata)
    }

    pub fn load_workflow(&self, id: &str) -> WorkflowStoreResult<String> {
        let path = self.workflow_path(id)?;
        if !path.exists() {
            return Err(WorkflowStoreError::NotFound(id.to_owned()));
        }

        let yaml = fs::read_to_string(path)?;
        workflow_yaml_to_graph_json(&yaml)?;
        Ok(yaml)
    }

    pub fn list_workflows(&self) -> WorkflowStoreResult<Vec<WorkflowMetadata>> {
        self.ensure_root()?;

        let indexed = self.read_index()?;
        let mut by_id = BTreeMap::new();
        let mut live_workflow = None;
        let mut changed = false;

        for workflow in indexed {
            if workflow.id.trim().is_empty() || is_live_workflow_id(&workflow.id) {
                changed = true;
                continue;
            }

            let path = self.workflow_path(&workflow.id)?;
            if path.exists() {
                by_id.insert(workflow.id.clone(), workflow);
            } else {
                changed = true;
            }
        }

        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("yaml") {
                continue;
            }

            let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            if stem == "latest" {
                let yaml = fs::read_to_string(&path).unwrap_or_default();
                live_workflow = Some(WorkflowMetadata {
                    id: "hook-live".to_owned(),
                    name: workflow_name_from_yaml(&yaml)
                        .unwrap_or_else(|| "Hook 实时工作流".to_owned()),
                    node_count: count_nodes_in_yaml(&yaml),
                    updated_at: now_string(),
                });
                continue;
            }

            let yaml = fs::read_to_string(&path).unwrap_or_default();
            let id = stem.to_owned();
            let name = workflow_name_from_yaml(&yaml).unwrap_or_else(|| id.clone());
            let node_count = count_nodes_in_yaml(&yaml);

            match by_id.get_mut(&id) {
                Some(existing) if existing.name == name && existing.node_count == node_count => {}
                Some(existing) => {
                    existing.name = name;
                    existing.node_count = node_count;
                    changed = true;
                }
                None => {
                    by_id.insert(
                        id.clone(),
                        WorkflowMetadata {
                            id,
                            name,
                            node_count,
                            updated_at: now_string(),
                        },
                    );
                    changed = true;
                }
            }
        }

        let mut workflows: Vec<_> = by_id.into_values().collect();
        sort_metadata(&mut workflows);
        if changed {
            self.write_index(&workflows)?;
        }
        if let Some(workflow) = live_workflow {
            workflows.push(workflow);
            sort_metadata(&mut workflows);
        }
        Ok(workflows)
    }

    pub fn delete_workflow(&self, id: &str) -> WorkflowStoreResult<()> {
        self.ensure_root()?;
        let path = self.workflow_path(id)?;
        if path.exists() {
            fs::remove_file(path)?;
        }

        if !is_live_workflow_id(id) {
            let mut workflows = self.read_index()?;
            let before = workflows.len();
            workflows.retain(|workflow| workflow.id != id);
            if workflows.len() != before {
                self.write_index(&workflows)?;
            }
        }

        Ok(())
    }

    fn ensure_root(&self) -> WorkflowStoreResult<()> {
        fs::create_dir_all(&self.root)?;
        Ok(())
    }

    fn workflow_path(&self, id: &str) -> WorkflowStoreResult<PathBuf> {
        validate_workflow_id(id)?;
        Ok(self.root.join(workflow_file_name(id)))
    }

    fn index_path(&self) -> PathBuf {
        self.root.join(WORKFLOW_INDEX_FILE)
    }

    fn read_index(&self) -> WorkflowStoreResult<Vec<WorkflowMetadata>> {
        let path = self.index_path();
        if !path.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(path)?;
        serde_json::from_str(&content).map_err(WorkflowStoreError::from)
    }

    fn write_index(&self, workflows: &[WorkflowMetadata]) -> WorkflowStoreResult<()> {
        let content = serde_json::to_string_pretty(workflows)?;
        fs::write(self.index_path(), content)?;
        Ok(())
    }
}

#[must_use]
pub fn workflow_file_name(id: &str) -> String {
    if is_live_workflow_id(id) {
        LIVE_WORKFLOW_FILE.to_owned()
    } else {
        format!("{id}.yaml")
    }
}

pub fn graph_json_to_workflow_yaml(
    graph: &JsonValue,
    workflow_name: Option<&str>,
    workflow_description: Option<&str>,
) -> WorkflowStoreResult<String> {
    let nodes = graph
        .get("nodes")
        .and_then(JsonValue::as_array)
        .cloned()
        .unwrap_or_default();
    let edges = graph
        .get("edges")
        .and_then(JsonValue::as_array)
        .cloned()
        .unwrap_or_default();

    let mut yaml_nodes = Vec::new();

    for node in nodes {
        let Some(node_id) = node.get("id").and_then(JsonValue::as_str) else {
            continue;
        };

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

        for edge in edges
            .iter()
            .filter(|edge| edge.get("target").and_then(JsonValue::as_str) == Some(node_id))
        {
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

    serde_yaml::to_string(&JsonValue::Object(workflow)).map_err(WorkflowStoreError::from)
}

/// Collect the art ids referenced by a workflow — every node's `uses` value,
/// excluding the sticker placeholder. Used to resolve a workflow art's
/// dependent arts (phase-1 recursive install). Returns unique ids in order.
pub fn collect_workflow_uses(yaml: &str) -> WorkflowStoreResult<Vec<String>> {
    let parsed: YamlValue = serde_yaml::from_str(yaml)?;
    let mut seen = std::collections::BTreeSet::new();
    let mut uses = Vec::new();
    if let Some(nodes) = parsed.get("nodes").and_then(YamlValue::as_sequence) {
        for node in nodes {
            if let Some(value) = node.get("uses").and_then(YamlValue::as_str) {
                if value == STICKER_USES {
                    continue;
                }
                if seen.insert(value.to_owned()) {
                    uses.push(value.to_owned());
                }
            }
        }
    }
    Ok(uses)
}

pub fn workflow_yaml_to_graph_json(yaml: &str) -> WorkflowStoreResult<JsonValue> {
    let parsed: YamlValue = serde_yaml::from_str(yaml)?;
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

fn validate_workflow_id(id: &str) -> WorkflowStoreResult<()> {
    if id.trim().is_empty()
        || id.contains("..")
        || id.contains('/')
        || id.contains('\\')
        || id.contains(':')
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

fn is_live_workflow_id(id: &str) -> bool {
    id == "hook-live"
}

fn now_string() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().to_string())
        .unwrap_or_else(|_| "0".to_owned())
}

fn workflow_name_from_yaml(yaml: &str) -> Option<String> {
    serde_yaml::from_str::<YamlValue>(yaml)
        .ok()
        .and_then(|parsed| {
            parsed
                .get("name")
                .and_then(YamlValue::as_str)
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
        })
}

fn count_nodes_in_yaml(yaml: &str) -> usize {
    serde_yaml::from_str::<YamlValue>(yaml)
        .ok()
        .and_then(|parsed| {
            parsed
                .get("nodes")
                .and_then(YamlValue::as_sequence)
                .map(Vec::len)
        })
        .unwrap_or(0)
}

fn sort_metadata(workflows: &mut [WorkflowMetadata]) {
    workflows.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| left.id.cmp(&right.id))
    });
}

fn clone_param_map(params: Option<&JsonMap<String, JsonValue>>) -> JsonMap<String, JsonValue> {
    params.cloned().unwrap_or_default()
}

fn parse_workflow_output_reference(value: &str) -> Option<(String, String)> {
    let inner = value.trim().strip_prefix("${{")?.strip_suffix("}}")?.trim();
    let rest = inner.strip_prefix("nodes.")?;
    let (source, handle) = rest.split_once(".outputs.")?;

    if source.is_empty() || handle.is_empty() {
        return None;
    }

    Some((source.to_owned(), handle.to_owned()))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn collect_workflow_uses_dedupes_and_skips_stickers() {
        let yaml = r#"
name: wf
nodes:
  - id: a
    uses: __sticker__
  - id: b
    uses: neuro.official/resize
  - id: c
    uses: neuro.official/ocr
    needs: [b]
  - id: d
    uses: neuro.official/resize
    needs: [c]
"#;
        let uses = super::collect_workflow_uses(yaml).expect("collect uses");
        assert_eq!(
            uses,
            vec![
                "neuro.official/resize".to_owned(),
                "neuro.official/ocr".to_owned()
            ]
        );
    }

    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("loom-workflow-store-{name}-{nonce}"));
        fs::create_dir_all(&root).expect("create temp workflow root");
        root
    }

    #[test]
    fn hook_live_alias_uses_latest_yaml() {
        let root = temp_root("hook-live-alias");
        let store = WorkflowStore::new(&root);
        let yaml = "name: Live\nnodes: []\n";

        assert_eq!(workflow_file_name("hook-live"), "latest.yaml");

        store
            .save_workflow("hook-live", yaml)
            .expect("save hook live workflow");

        assert!(root.join("latest.yaml").exists());
        assert!(!root.join("hook-live.yaml").exists());
        assert_eq!(
            store
                .load_workflow("hook-live")
                .expect("load live workflow"),
            yaml
        );

        fs::remove_dir_all(root).expect("cleanup temp workflow root");
    }

    #[test]
    fn list_workflows_includes_hook_live_alias_when_latest_yaml_exists() {
        let root = temp_root("hook-live-list");
        let store = WorkflowStore::new(&root);
        let yaml = "name: Hook 实时工作流\nnodes:\n  - id: screenshot\n    uses: __sticker__\n";

        store
            .save_workflow("hook-live", yaml)
            .expect("save hook live workflow");

        let listed = store.list_workflows().expect("list workflows");
        let live = listed
            .iter()
            .find(|workflow| workflow.id == "hook-live")
            .expect("hook live workflow should be listed");

        assert_eq!(live.name, "Hook 实时工作流");
        assert_eq!(live.node_count, 1);
        assert!(!root.join("workflow_index.json").exists());

        fs::remove_dir_all(root).expect("cleanup temp workflow root");
    }

    #[test]
    fn save_load_list_and_delete_workflow_roundtrip() {
        let root = temp_root("roundtrip");
        let store = WorkflowStore::new(&root);
        let yaml = r#"name: Paint Flow
description: demo
nodes:
  - id: prompt
    uses: neuro.official/text-prompt
  - id: image
    uses: neuro.official/image-generate
    needs: [prompt]
"#;

        let metadata = store
            .save_workflow("paint-flow", yaml)
            .expect("save workflow");

        assert_eq!(metadata.id, "paint-flow");
        assert_eq!(metadata.name, "Paint Flow");
        assert_eq!(metadata.node_count, 2);
        assert_eq!(
            store.load_workflow("paint-flow").expect("load workflow"),
            yaml
        );
        assert!(root.join("workflow_index.json").exists());

        let listed = store.list_workflows().expect("list workflows");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "paint-flow");
        assert_eq!(listed[0].node_count, 2);

        store
            .delete_workflow("paint-flow")
            .expect("delete workflow");
        assert!(!root.join("paint-flow.yaml").exists());
        assert!(store.load_workflow("paint-flow").is_err());
        assert!(store
            .list_workflows()
            .expect("list after delete")
            .is_empty());

        fs::remove_dir_all(root).expect("cleanup temp workflow root");
    }

    #[test]
    fn graph_json_roundtrips_to_yaml_and_back() {
        let graph = serde_json::json!({
            "nodes": [
                {
                    "id": "prompt",
                    "type": "artNode",
                    "position": { "x": 10, "y": 20 },
                    "data": {
                        "artId": "neuro.official/text-prompt",
                        "label": "Prompt",
                        "params": { "prompt": "castle", "strength": 0.75 }
                    }
                },
                {
                    "id": "image",
                    "type": "artNode",
                    "position": { "x": 300, "y": 20 },
                    "data": {
                        "artId": "neuro.official/image-generate",
                        "label": "Generate",
                        "params": { "steps": 20 }
                    }
                }
            ],
            "edges": [
                {
                    "source": "prompt",
                    "target": "image",
                    "sourceHandle": "text",
                    "targetHandle": "prompt"
                }
            ]
        });

        let yaml = graph_json_to_workflow_yaml(&graph, Some("Roundtrip"), Some("demo"))
            .expect("graph to yaml");
        assert!(yaml.contains("name: Roundtrip"));
        assert!(yaml.contains("description: demo"));
        assert!(yaml.contains("uses: neuro.official/text-prompt"));
        assert!(yaml.contains("uses: neuro.official/image-generate"));

        let parsed = workflow_yaml_to_graph_json(&yaml).expect("yaml to graph");
        let nodes = parsed["nodes"].as_array().expect("nodes array");
        assert_eq!(nodes.len(), 2);

        let prompt = nodes
            .iter()
            .find(|node| node["id"] == "prompt")
            .expect("prompt node");
        assert_eq!(prompt["data"]["params"]["prompt"], "castle");
        assert_eq!(prompt["data"]["params"]["strength"], 0.75);

        let image = nodes
            .iter()
            .find(|node| node["id"] == "image")
            .expect("image node");
        assert_eq!(image["data"]["params"]["steps"], 20);

        let edges = parsed["edges"].as_array().expect("edges array");
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0]["source"], "prompt");
        assert_eq!(edges[0]["target"], "image");
        assert_eq!(edges[0]["sourceHandle"], "text");
        assert_eq!(edges[0]["targetHandle"], "prompt");
    }

    #[test]
    fn graph_codec_rejects_noncanonical_node_types_and_unqualified_art_ids() {
        for graph in [
            serde_json::json!({ "nodes": [{ "id": "missing", "data": {} }], "edges": [] }),
            serde_json::json!({ "nodes": [{ "id": "old", "type": "art", "data": { "artId": "neuro.official/demo" } }], "edges": [] }),
            serde_json::json!({ "nodes": [{ "id": "bare", "type": "artNode", "data": { "artId": "demo" } }], "edges": [] }),
            serde_json::json!({ "nodes": [{ "id": "empty", "type": "artNode", "data": {} }], "edges": [] }),
        ] {
            assert!(matches!(
                graph_json_to_workflow_yaml(&graph, None, None),
                Err(WorkflowStoreError::InvalidWorkflowGraph(_))
            ));
        }
    }

    #[test]
    fn workflow_yaml_codec_rejects_missing_or_unqualified_uses() {
        for yaml in [
            "name: invalid\nnodes:\n  - id: missing\n",
            "name: invalid\nnodes:\n  - id: bare\n    uses: demo\n",
        ] {
            assert!(matches!(
                workflow_yaml_to_graph_json(yaml),
                Err(WorkflowStoreError::InvalidWorkflowYaml(_))
            ));
        }
    }
}
