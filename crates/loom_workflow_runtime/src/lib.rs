//! Runtime for registry-backed Loom workflow Arts.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use loom_tool_registry::{
    execute_tool, ToolDefinition, ToolExecution, ToolRegistry, ToolRegistryError,
    WorkflowExecutionBindings, WorkflowInputBinding, WorkflowOutputBinding,
};
use loom_workflow_store::{WorkflowStore, WorkflowStoreError};
use serde::Deserialize;
use serde_json::{json, Map as JsonMap, Value as JsonValue};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WorkflowRuntimeError {
    #[error("tool registry error: {0}")]
    ToolRegistry(#[from] ToolRegistryError),
    #[error("workflow store error: {0}")]
    WorkflowStore(#[from] WorkflowStoreError),
    #[error("workflow `{workflow_id}` is invalid: {source}")]
    WorkflowYaml {
        workflow_id: String,
        source: serde_yaml::Error,
    },
    #[error("workflow `{workflow_id}` child tool `{tool_id}` was not found")]
    ChildToolNotFound {
        workflow_id: String,
        tool_id: String,
    },
    #[error("workflow `{workflow_id}` contains unresolved dependencies or a cycle")]
    UnresolvedDependencies { workflow_id: String },
    #[error("workflow `{workflow_id}` node `{node_id}` requires image input")]
    MissingImageInput {
        workflow_id: String,
        node_id: String,
    },
    #[error("workflow `{workflow_id}` native node `{node_id}` failed: {message}")]
    NativeFailed {
        workflow_id: String,
        node_id: String,
        message: String,
    },
}

pub type WorkflowRuntimeResult<T> = Result<T, WorkflowRuntimeError>;

pub fn execute_tool_with_workflows(
    tool: &ToolDefinition,
    mcp_servers: &[loom_mcp::McpServerConfig],
    workflow_store: &WorkflowStore,
    tool_registry: &ToolRegistry,
    arguments: JsonValue,
) -> WorkflowRuntimeResult<serde_json::Value> {
    match &tool.execution {
        ToolExecution::Workflow {
            workflow_id,
            workflow_bindings,
        } => execute_workflow_tool(
            workflow_id,
            workflow_bindings.as_ref(),
            mcp_servers,
            workflow_store,
            tool_registry,
            arguments,
        ),
        _ => execute_tool(tool, mcp_servers, arguments).map_err(WorkflowRuntimeError::from),
    }
}

#[derive(Debug, Deserialize, Clone)]
struct StoredWorkflow {
    #[serde(default)]
    nodes: Vec<StoredWorkflowNode>,
}

#[derive(Debug, Deserialize, Clone)]
struct StoredWorkflowNode {
    id: String,
    uses: String,
    #[serde(default)]
    needs: Vec<String>,
    #[serde(rename = "with", default)]
    params: BTreeMap<String, serde_yaml::Value>,
    #[serde(default)]
    meta: Option<StoredWorkflowNodeMeta>,
}

#[derive(Debug, Deserialize, Clone)]
struct StoredWorkflowNodeMeta {
    #[serde(default)]
    src: Option<String>,
    #[serde(default, rename = "previewSrc")]
    preview_src: Option<String>,
}

fn execute_workflow_tool(
    workflow_id: &str,
    workflow_bindings: Option<&WorkflowExecutionBindings>,
    mcp_servers: &[loom_mcp::McpServerConfig],
    workflow_store: &WorkflowStore,
    tool_registry: &ToolRegistry,
    arguments: JsonValue,
) -> WorkflowRuntimeResult<JsonValue> {
    let workflow_yaml = workflow_store.load_workflow(workflow_id)?;
    let workflow: StoredWorkflow = serde_yaml::from_str(&workflow_yaml).map_err(|source| {
        WorkflowRuntimeError::WorkflowYaml {
            workflow_id: workflow_id.to_owned(),
            source,
        }
    })?;

    if workflow.nodes.is_empty() {
        return Ok(text_content_response(&format!(
            "workflow `{workflow_id}` completed with no nodes"
        )));
    }

    let root_input = extract_root_input(&arguments);
    let mut pending = workflow
        .nodes
        .iter()
        .cloned()
        .map(|node| (node.id.clone(), node))
        .collect::<BTreeMap<_, _>>();
    let order = workflow
        .nodes
        .iter()
        .map(|node| node.id.clone())
        .collect::<Vec<_>>();
    let mut results = HashMap::<String, JsonValue>::new();

    while !pending.is_empty() {
        let ready = order
            .iter()
            .filter(|node_id| pending.contains_key(*node_id))
            .filter(|node_id| {
                pending[*node_id]
                    .needs
                    .iter()
                    .all(|dependency| results.contains_key(dependency))
            })
            .cloned()
            .collect::<Vec<_>>();

        if ready.is_empty() {
            return Err(WorkflowRuntimeError::UnresolvedDependencies {
                workflow_id: workflow_id.to_owned(),
            });
        }

        for node_id in ready {
            let node = pending
                .remove(&node_id)
                .expect("ready node is present in pending map");
            let result = execute_workflow_node(
                workflow_id,
                &node,
                workflow_bindings,
                &root_input,
                &arguments,
                &results,
                mcp_servers,
                workflow_store,
                tool_registry,
            )?;
            results.insert(node_id, result);
        }
    }

    Ok(select_workflow_output(
        workflow_id,
        &workflow.nodes,
        workflow_bindings.and_then(|bindings| bindings.primary_output.as_ref()),
        &results,
    ))
}

#[allow(clippy::too_many_arguments)]
fn execute_workflow_node(
    workflow_id: &str,
    node: &StoredWorkflowNode,
    workflow_bindings: Option<&WorkflowExecutionBindings>,
    root_input: &Option<String>,
    root_arguments: &JsonValue,
    results: &HashMap<String, JsonValue>,
    mcp_servers: &[loom_mcp::McpServerConfig],
    workflow_store: &WorkflowStore,
    tool_registry: &ToolRegistry,
) -> WorkflowRuntimeResult<JsonValue> {
    let mut child_args = JsonMap::new();
    let mut child_input = None;

    for (target, raw_value) in &node.params {
        let json_value = yaml_value_to_json(raw_value);
        if let Some(reference) = json_value.as_str() {
            if let Some(resolved) = resolve_workflow_reference(reference, results) {
                if let Some(image) = json_value_as_image(&resolved) {
                    if child_input.is_none() {
                        child_input = Some(image);
                    } else {
                        insert_child_argument(&mut child_args, target, resolved);
                    }
                } else {
                    insert_child_argument(&mut child_args, target, resolved);
                }
                continue;
            }
        }

        insert_child_argument(&mut child_args, target, json_value);
    }

    if let Some(bindings) = workflow_bindings {
        apply_input_bindings(
            bindings,
            node,
            root_arguments,
            root_input,
            &mut child_input,
            &mut child_args,
        );
    }

    if child_input.is_none() {
        child_input = extract_root_input(&JsonValue::Object(child_args.clone()));
    }
    if child_input.is_none() && node.needs.is_empty() {
        child_input = root_input.clone();
    }
    if child_input.is_none() {
        child_input = node
            .meta
            .as_ref()
            .and_then(|meta| meta.preview_src.clone().or_else(|| meta.src.clone()));
    }

    if let Some(input) = &child_input {
        insert_child_input(&mut child_args, input);
    }

    if is_sticker_art(&node.uses) {
        let input = child_input.ok_or_else(|| WorkflowRuntimeError::MissingImageInput {
            workflow_id: workflow_id.to_owned(),
            node_id: node.id.clone(),
        })?;
        return Ok(image_content_response(&input, "image/png"));
    }

    if loom_native_image::is_native_art_id(&node.uses) {
        let input = child_input.ok_or_else(|| WorkflowRuntimeError::MissingImageInput {
            workflow_id: workflow_id.to_owned(),
            node_id: node.id.clone(),
        })?;
        let params = child_args.into_iter().collect::<HashMap<_, _>>();
        let result = loom_native_image::process_art(&node.uses, &input, params);
        if !result.success {
            return Err(WorkflowRuntimeError::NativeFailed {
                workflow_id: workflow_id.to_owned(),
                node_id: node.id.clone(),
                message: result
                    .error
                    .unwrap_or_else(|| "native image processor failed".to_owned()),
            });
        }
        let output = result
            .output_base64
            .ok_or_else(|| WorkflowRuntimeError::NativeFailed {
                workflow_id: workflow_id.to_owned(),
                node_id: node.id.clone(),
                message: "native image processor returned no output".to_owned(),
            })?;
        return Ok(image_content_response(&output, "image/png"));
    }

    let child_tool = tool_registry.get_tool(&node.uses)?.ok_or_else(|| {
        WorkflowRuntimeError::ChildToolNotFound {
            workflow_id: workflow_id.to_owned(),
            tool_id: node.uses.clone(),
        }
    })?;

    execute_tool_with_workflows(
        &child_tool,
        mcp_servers,
        workflow_store,
        tool_registry,
        JsonValue::Object(child_args),
    )
}

fn apply_input_bindings(
    bindings: &WorkflowExecutionBindings,
    node: &StoredWorkflowNode,
    root_arguments: &JsonValue,
    root_input: &Option<String>,
    child_input: &mut Option<String>,
    child_args: &mut JsonMap<String, JsonValue>,
) {
    for binding in bindings
        .inputs
        .iter()
        .filter(|binding| binding.node_id == node.id)
    {
        match binding.kind.as_str() {
            "input_image" => {
                if let Some(value) = bound_argument_as_image(binding, root_arguments, root_input) {
                    *child_input = Some(value);
                }
            }
            "input_value" | "param" => {
                if let Some(value) = bound_argument_value(binding, root_arguments) {
                    insert_child_argument(child_args, &binding.target, value);
                }
            }
            _ => {
                if let Some(value) = bound_argument_value(binding, root_arguments) {
                    insert_child_argument(child_args, &binding.target, value);
                }
            }
        }
    }
}

fn bound_argument_value(
    binding: &WorkflowInputBinding,
    root_arguments: &JsonValue,
) -> Option<JsonValue> {
    root_arguments
        .as_object()
        .and_then(|arguments| arguments.get(&binding.workflow_param))
        .cloned()
}

fn bound_argument_as_image(
    binding: &WorkflowInputBinding,
    root_arguments: &JsonValue,
    root_input: &Option<String>,
) -> Option<String> {
    bound_argument_value(binding, root_arguments)
        .and_then(|value| json_value_as_image(&value))
        .or_else(|| {
            if binding.workflow_param == "input" {
                root_input.clone()
            } else {
                None
            }
        })
}

fn select_workflow_output(
    workflow_id: &str,
    nodes: &[StoredWorkflowNode],
    primary_output: Option<&WorkflowOutputBinding>,
    results: &HashMap<String, JsonValue>,
) -> JsonValue {
    if let Some(primary_output) = primary_output {
        if let Some(result) = results.get(&primary_output.node_id) {
            if primary_output.kind == "node_result" {
                return result.clone();
            }
            if let Some(output) = extract_named_output(result, &primary_output.output) {
                return value_to_content_response(output);
            }
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

fn resolve_workflow_reference(
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

fn extract_named_output(result: &JsonValue, output: &str) -> Option<JsonValue> {
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

fn extract_default_output(result: &JsonValue) -> Option<JsonValue> {
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

fn extract_image_output(value: &JsonValue) -> Option<String> {
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

fn extract_text_output(value: &JsonValue) -> Option<String> {
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

fn unwrap_nested_result(value: &JsonValue) -> &JsonValue {
    value.get("result").unwrap_or(value)
}

fn value_to_content_response(value: JsonValue) -> JsonValue {
    if let Some(image) = json_value_as_image(&value) {
        image_content_response(&image, "image/png")
    } else if let Some(text) = value.as_str() {
        text_content_response(text)
    } else {
        text_content_response(&value.to_string())
    }
}

fn yaml_value_to_json(value: &serde_yaml::Value) -> JsonValue {
    serde_json::to_value(value).unwrap_or(JsonValue::Null)
}

fn insert_child_argument(
    child_args: &mut JsonMap<String, JsonValue>,
    target: &str,
    value: JsonValue,
) {
    let target = target
        .strip_prefix("params.")
        .or_else(|| target.strip_prefix("with."))
        .unwrap_or(target);
    child_args.insert(target.to_owned(), value);
}

fn insert_child_input(child_args: &mut JsonMap<String, JsonValue>, input: &str) {
    child_args
        .entry("input_base64".to_owned())
        .or_insert_with(|| JsonValue::String(input.to_owned()));
    child_args
        .entry("input".to_owned())
        .or_insert_with(|| JsonValue::String(input.to_owned()));
    child_args
        .entry("image".to_owned())
        .or_insert_with(|| JsonValue::String(input.to_owned()));
}

fn extract_root_input(arguments: &JsonValue) -> Option<String> {
    let object = arguments.as_object()?;
    ["input_base64", "image", "data", "output_base64"]
        .iter()
        .find_map(|key| {
            object
                .get(*key)
                .and_then(JsonValue::as_str)
                .filter(|value| is_image_like(value))
                .map(str::to_owned)
        })
        .or_else(|| {
            object
                .get("input")
                .and_then(|value| {
                    value.as_str().map(str::to_owned).or_else(|| {
                        value
                            .get("data")
                            .and_then(JsonValue::as_str)
                            .map(str::to_owned)
                    })
                })
                .filter(|value| is_image_like(value))
        })
}

fn json_value_as_image(value: &JsonValue) -> Option<String> {
    value
        .as_str()
        .filter(|value| is_image_like(value))
        .map(str::to_owned)
        .or_else(|| {
            value
                .get("data")
                .and_then(JsonValue::as_str)
                .filter(|value| is_image_like(value))
                .map(str::to_owned)
        })
        .or_else(|| extract_image_output(value))
}

fn is_sticker_art(uses: &str) -> bool {
    matches!(uses, "__sticker__" | "sticker")
}

fn is_image_like(value: &str) -> bool {
    value.starts_with("data:image/") || looks_like_base64_payload(value)
}

fn looks_like_base64_payload(value: &str) -> bool {
    value.len() >= 8
        && !value.chars().any(char::is_whitespace)
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '/' | '=' | '-' | '_'))
}

fn image_content_response(data: &str, mime_type: &str) -> JsonValue {
    let data = if data.starts_with("data:image/") && data.contains(";base64,") {
        data.to_owned()
    } else {
        format!("data:{mime_type};base64,{data}")
    };
    json!({
        "content": [
            {
                "type": "image",
                "data": data,
                "mimeType": mime_type
            }
        ]
    })
}

fn text_content_response(text: &str) -> JsonValue {
    json!({
        "content": [
            {
                "type": "text",
                "text": text
            }
        ]
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use loom_tool_registry::{
        ToolDefinition, ToolExecution, ToolRegistry, WorkflowExecutionBindings,
        WorkflowOutputBinding,
    };
    use loom_workflow_store::WorkflowStore;
    use serde_json::json;

    use super::*;

    const TEST_IMAGE: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGPgEpH7DwABpAE8k4sOtwAAAABJRU5ErkJggg==";

    fn temp_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("loom-workflow-runtime-{name}-{nonce}"));
        fs::create_dir_all(&root).expect("create temp workflow runtime root");
        root
    }

    fn save_script_tool(registry: &ToolRegistry, script_path: &Path) {
        registry
            .save_tool(ToolDefinition::new(
                "fixture-script",
                "Fixture Script",
                "Script child",
                ToolExecution::Script {
                    path: script_path.display().to_string(),
                },
            ))
            .expect("save script tool");
    }

    fn workflow_tool(workflow_id: &str) -> ToolDefinition {
        ToolDefinition::new(
            "fixture-workflow",
            "Fixture Workflow",
            "Workflow child runner",
            ToolExecution::Workflow {
                workflow_id: workflow_id.to_owned(),
                workflow_bindings: None,
            },
        )
    }

    #[test]
    fn workflow_tool_executes_saved_script_child() {
        let root = temp_root("script-child");
        let workflow_store = WorkflowStore::new(root.join("workflows"));
        let tool_registry = ToolRegistry::new(root.join("tools"));
        let script_path = write_script_fixture(&root);
        save_script_tool(&tool_registry, &script_path);
        workflow_store
            .save_workflow(
                "runtime-flow",
                r#"name: Runtime Flow
nodes:
  - id: prompt
    uses: fixture-script
    with:
      text: hello workflow runtime
"#,
            )
            .expect("save workflow");

        let result = execute_tool_with_workflows(
            &workflow_tool("runtime-flow"),
            &[],
            &workflow_store,
            &tool_registry,
            json!({}),
        )
        .expect("execute workflow tool");

        assert_eq!(result["content"][0]["type"], "text");
        assert_eq!(
            result["content"][0]["text"],
            "script saw hello workflow runtime"
        );
        fs::remove_dir_all(root).expect("cleanup temp workflow runtime root");
    }

    #[test]
    fn workflow_runtime_resolves_node_output_reference() {
        let root = temp_root("reference");
        let workflow_store = WorkflowStore::new(root.join("workflows"));
        let tool_registry = ToolRegistry::new(root.join("tools"));
        let script_path = write_script_fixture(&root);
        save_script_tool(&tool_registry, &script_path);
        workflow_store
            .save_workflow(
                "reference-flow",
                r#"name: Reference Flow
nodes:
  - id: prompt
    uses: fixture-script
    with:
      text: hello reference
  - id: followup
    uses: fixture-script
    needs:
      - prompt
    with:
      text: ${{ nodes.prompt.outputs.text }}
"#,
            )
            .expect("save workflow");

        let result = execute_tool_with_workflows(
            &workflow_tool("reference-flow"),
            &[],
            &workflow_store,
            &tool_registry,
            json!({}),
        )
        .expect("execute workflow tool");

        assert_eq!(result["content"][0]["type"], "text");
        assert_eq!(
            result["content"][0]["text"],
            "script saw script saw hello reference"
        );
        fs::remove_dir_all(root).expect("cleanup temp workflow runtime root");
    }

    #[test]
    fn workflow_runtime_primary_output_binding_selects_configured_node() {
        let root = temp_root("primary-output");
        let workflow_store = WorkflowStore::new(root.join("workflows"));
        let tool_registry = ToolRegistry::new(root.join("tools"));
        let script_path = write_script_fixture(&root);
        save_script_tool(&tool_registry, &script_path);
        workflow_store
            .save_workflow(
                "primary-flow",
                r#"name: Primary Flow
nodes:
  - id: prompt
    uses: fixture-script
    with:
      text: first output
  - id: tail
    uses: fixture-script
    needs:
      - prompt
    with:
      text: second output
"#,
            )
            .expect("save workflow");
        let tool = ToolDefinition::new(
            "fixture-workflow",
            "Fixture Workflow",
            "Workflow child runner",
            ToolExecution::Workflow {
                workflow_id: "primary-flow".to_owned(),
                workflow_bindings: Some(WorkflowExecutionBindings {
                    inputs: vec![],
                    primary_output: Some(WorkflowOutputBinding {
                        node_id: "prompt".to_owned(),
                        output: "text".to_owned(),
                        kind: "node_result".to_owned(),
                    }),
                }),
            },
        );

        let result =
            execute_tool_with_workflows(&tool, &[], &workflow_store, &tool_registry, json!({}))
                .expect("execute workflow tool");

        assert_eq!(result["content"][0]["type"], "text");
        assert_eq!(result["content"][0]["text"], "script saw first output");
        fs::remove_dir_all(root).expect("cleanup temp workflow runtime root");
    }

    #[test]
    fn workflow_runtime_executes_native_image_child() {
        let root = temp_root("native-image");
        let workflow_store = WorkflowStore::new(root.join("workflows"));
        let tool_registry = ToolRegistry::new(root.join("tools"));
        workflow_store
            .save_workflow(
                "native-flow",
                r#"name: Native Flow
nodes:
  - id: invert
    uses: core.image.invert
"#,
            )
            .expect("save workflow");

        let result = execute_tool_with_workflows(
            &workflow_tool("native-flow"),
            &[],
            &workflow_store,
            &tool_registry,
            json!({ "input_base64": TEST_IMAGE }),
        )
        .expect("execute workflow tool");

        assert_eq!(result["content"][0]["type"], "image");
        let output = result["content"][0]["data"].as_str().expect("image data");
        assert!(output.starts_with("data:image/png;base64,"));
        fs::remove_dir_all(root).expect("cleanup temp workflow runtime root");
    }

    #[cfg(windows)]
    #[test]
    fn workflow_runtime_executes_image_blend_then_cli_compress_with_bound_inputs_and_params() {
        let root = temp_root("image-blend-compress");
        let workflow_store = WorkflowStore::new(root.join("workflows"));
        let tool_registry = ToolRegistry::new(root.join("tools"));
        let workflow_yaml =
            fs::read_to_string(workspace_image_blend_compress_resource("workflow.yaml"))
                .expect("read image blend compress workflow");
        let workflow_tool: ToolDefinition = serde_json::from_str(
            &fs::read_to_string(workspace_image_blend_compress_resource("manifest.json"))
                .expect("read image blend compress manifest"),
        )
        .expect("parse image blend compress manifest");

        tool_registry
            .save_tool(ToolDefinition::new(
                "custom-image-blend-script",
                "Fixture Image Blend",
                "Production image blend script child",
                ToolExecution::Script {
                    path: workspace_image_blend_script().display().to_string(),
                },
            ))
            .expect("save production image blend script tool");
        let (compress_script, compress_evidence) = write_cli_image_copy_fixture(&root);
        save_fixture_compress_tool(&tool_registry, &compress_script, &compress_evidence);
        workflow_store
            .save_workflow("image-blend-compress-workflow", &workflow_yaml)
            .expect("save image blend compress workflow");

        let source = loom_image_io::rgba8_to_png_data_url(1, 1, &[240, 60, 0, 255])
            .expect("encode workflow source image");
        let reference = loom_image_io::rgba8_to_png_data_url(1, 1, &[40, 160, 200, 255])
            .expect("encode workflow reference image");
        let result = execute_tool_with_workflows(
            &workflow_tool,
            &[],
            &workflow_store,
            &tool_registry,
            json!({
                "input_base64": source,
                "reference": reference,
                "mix_ratio": 25,
                "quality_num": 73
            }),
        )
        .expect("execute image blend compress workflow");

        assert_eq!(result["content"][0]["type"], "image");
        let output = loom_image_io::decode_image_base64_to_rgba8(
            result["content"][0]["data"]
                .as_str()
                .expect("workflow image output data"),
        )
        .expect("decode workflow image output");
        assert_eq!(output.width, 1);
        assert_eq!(output.height, 1);
        assert_eq!(output.data, vec![190, 85, 50, 255]);
        assert_eq!(
            fs::read_to_string(&compress_evidence).expect("read compression evidence"),
            "73"
        );
        fs::remove_dir_all(root).expect("cleanup image blend compress workflow root");
    }

    #[test]
    fn workflow_runtime_reports_unresolved_dependencies() {
        let root = temp_root("cycle");
        let workflow_store = WorkflowStore::new(root.join("workflows"));
        let tool_registry = ToolRegistry::new(root.join("tools"));
        workflow_store
            .save_workflow(
                "cycle-flow",
                r#"name: Cycle Flow
nodes:
  - id: a
    uses: fixture-script
    needs: [b]
  - id: b
    uses: fixture-script
    needs: [a]
"#,
            )
            .expect("save workflow");

        let error = execute_tool_with_workflows(
            &workflow_tool("cycle-flow"),
            &[],
            &workflow_store,
            &tool_registry,
            json!({}),
        )
        .expect_err("cycle fails");

        assert!(error.to_string().contains("unresolved dependencies"));
        fs::remove_dir_all(root).expect("cleanup temp workflow runtime root");
    }

    fn write_script_fixture(root: &Path) -> PathBuf {
        #[cfg(windows)]
        {
            let script_path = root.join("fixture-script.ps1");
            let source = r#"
$ErrorActionPreference = "Stop"
$payload = $args[0] | ConvertFrom-Json
$arguments = $payload.arguments
$image = $arguments.input_base64
if (-not $image) { $image = $arguments.input }
if (-not $image) { $image = $arguments.image }
if ($image) {
    $response = [ordered]@{
        content = @(
            [ordered]@{
                type = "image"
                data = [string]$image
                mimeType = "image/png"
            }
        )
    }
} else {
    $response = [ordered]@{
        content = @(
            [ordered]@{
                type = "text"
                text = "script saw $($arguments.text)"
            }
        )
    }
}
[Console]::Out.WriteLine(($response | ConvertTo-Json -Depth 20 -Compress))
"#;
            fs::write(&script_path, source).expect("write PowerShell script fixture");
            script_path
        }
        #[cfg(not(windows))]
        {
            use std::os::unix::fs::PermissionsExt;

            let script_path = root.join("fixture-script.sh");
            let source = r#"#!/usr/bin/env sh
python3 - "$1" <<'PY'
import json
import sys

payload = json.loads(sys.argv[1])
arguments = payload.get("arguments", {})
image = arguments.get("input_base64") or arguments.get("input") or arguments.get("image")
if image:
    response = {
        "content": [
            {
                "type": "image",
                "data": image,
                "mimeType": "image/png",
            }
        ]
    }
else:
    response = {
        "content": [
            {
                "type": "text",
                "text": "script saw " + str(arguments.get("text", "")),
            }
        ]
    }
print(json.dumps(response))
PY
"#;
            fs::write(&script_path, source).expect("write shell script fixture");
            let mut permissions = fs::metadata(&script_path)
                .expect("script fixture metadata")
                .permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&script_path, permissions).expect("make shell fixture executable");
            script_path
        }
    }

    #[cfg(windows)]
    fn write_cli_image_copy_fixture(root: &Path) -> (PathBuf, PathBuf) {
        let script_path = root.join("fixture-compress.ps1");
        let evidence_path = root.join("compress-evidence.txt");
        let source = r#"
param(
    [Parameter(Mandatory = $true)][string]$InputPath,
    [Parameter(Mandatory = $true)][string]$OutputPath,
    [Parameter(Mandatory = $true)][int]$Quality,
    [Parameter(Mandatory = $true)][string]$EvidencePath
)
$ErrorActionPreference = "Stop"
Copy-Item -LiteralPath $InputPath -Destination $OutputPath -Force
[System.IO.File]::WriteAllText(
    $EvidencePath,
    [string]$Quality,
    [System.Text.UTF8Encoding]::new($false)
)
"#;
        fs::write(&script_path, source).expect("write CLI image copy fixture");
        (script_path, evidence_path)
    }

    #[cfg(windows)]
    fn save_fixture_compress_tool(
        registry: &ToolRegistry,
        script_path: &Path,
        evidence_path: &Path,
    ) {
        registry
            .save_tool(ToolDefinition::new(
                "custom-1770146354922",
                "Fixture Image Compress",
                "Deterministic cli_wrapper child for workflow tests",
                ToolExecution::CliWrapper {
                    command: "powershell.exe".to_owned(),
                    args: vec![
                        "-NoProfile".to_owned(),
                        "-ExecutionPolicy".to_owned(),
                        "Bypass".to_owned(),
                        "-File".to_owned(),
                        script_path.display().to_string(),
                        "-InputPath".to_owned(),
                        "{{input}}".to_owned(),
                        "-OutputPath".to_owned(),
                        "{{output}}".to_owned(),
                        "-Quality".to_owned(),
                        "{{quality_num}}".to_owned(),
                        "-EvidencePath".to_owned(),
                        evidence_path.display().to_string(),
                    ],
                },
            ))
            .expect("save fixture compression tool");
    }

    fn workspace_image_blend_compress_resource(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .find_map(|candidate| {
                let path = candidate
                    .join("resources")
                    .join("workflow-arts")
                    .join("image-blend-compress")
                    .join(name);
                path.exists().then_some(path)
            })
            .unwrap_or_else(|| panic!("locate image-blend-compress resource `{name}`"))
    }

    fn workspace_image_blend_script() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .find_map(|candidate| {
                let path = candidate
                    .join("resources")
                    .join("script-arts")
                    .join("image-blend")
                    .join("main.ps1");
                path.exists().then_some(path)
            })
            .expect("locate production image blend script")
    }
}
