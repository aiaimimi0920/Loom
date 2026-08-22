//! Runtime for registry-backed Loom workflow Arts.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use loom_tool_registry::{
    execute_tool, execute_tool_with_timeout, execute_tool_with_timeout_and_cancellation,
    prepare_tool_arguments, ToolDefinition, ToolExecution, ToolRegistry, ToolRegistryError,
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
    #[error("workflow `{workflow_id}` preview policy is invalid: {reason}")]
    InvalidPreviewPolicy { workflow_id: String, reason: String },
    #[error("workflow execution exceeded its caller-owned timeout")]
    Timeout,
    #[error("workflow execution was cancelled")]
    Cancelled,
}

pub type WorkflowRuntimeResult<T> = Result<T, WorkflowRuntimeError>;

pub fn execute_tool_with_workflows(
    tool: &ToolDefinition,
    mcp_servers: &[loom_mcp::McpServerConfig],
    workflow_store: &WorkflowStore,
    tool_registry: &ToolRegistry,
    arguments: JsonValue,
) -> WorkflowRuntimeResult<serde_json::Value> {
    execute_tool_with_workflows_internal(
        tool,
        mcp_servers,
        workflow_store,
        tool_registry,
        arguments,
        &mut |_| {},
        None,
        None,
    )
}

pub fn execute_tool_with_workflows_timeout(
    tool: &ToolDefinition,
    mcp_servers: &[loom_mcp::McpServerConfig],
    workflow_store: &WorkflowStore,
    tool_registry: &ToolRegistry,
    arguments: JsonValue,
    timeout: Duration,
) -> WorkflowRuntimeResult<serde_json::Value> {
    execute_tool_with_workflows_internal(
        tool,
        mcp_servers,
        workflow_store,
        tool_registry,
        arguments,
        &mut |_| {},
        Some(Instant::now() + timeout.max(Duration::from_millis(1))),
        None,
    )
}

/// Run a tool under a caller-owned timeout and cancellation flag, with no preview stream.
///
/// The preview variant of this call already existed, and a caller that wants cancellation without
/// previews had to pass a callback that discards everything it is handed. The surface action runner is
/// such a caller: an action has nowhere to show intermediate output, but it does need to be cancellable.
pub fn execute_tool_with_workflows_timeout_and_cancellation(
    tool: &ToolDefinition,
    mcp_servers: &[loom_mcp::McpServerConfig],
    workflow_store: &WorkflowStore,
    tool_registry: &ToolRegistry,
    arguments: JsonValue,
    timeout: Duration,
    cancellation: &AtomicBool,
) -> WorkflowRuntimeResult<serde_json::Value> {
    execute_tool_with_workflows_internal(
        tool,
        mcp_servers,
        workflow_store,
        tool_registry,
        arguments,
        &mut |_| {},
        Some(Instant::now() + timeout.max(Duration::from_millis(1))),
        Some(cancellation),
    )
}

pub fn execute_tool_with_workflows_and_preview<F>(
    tool: &ToolDefinition,
    mcp_servers: &[loom_mcp::McpServerConfig],
    workflow_store: &WorkflowStore,
    tool_registry: &ToolRegistry,
    arguments: JsonValue,
    mut preview_callback: F,
) -> WorkflowRuntimeResult<serde_json::Value>
where
    F: FnMut(JsonValue),
{
    execute_tool_with_workflows_internal(
        tool,
        mcp_servers,
        workflow_store,
        tool_registry,
        arguments,
        &mut preview_callback,
        None,
        None,
    )
}

pub fn execute_tool_with_workflows_and_preview_timeout_and_cancellation<F>(
    tool: &ToolDefinition,
    mcp_servers: &[loom_mcp::McpServerConfig],
    workflow_store: &WorkflowStore,
    tool_registry: &ToolRegistry,
    arguments: JsonValue,
    timeout: Duration,
    cancellation: &AtomicBool,
    mut preview_callback: F,
) -> WorkflowRuntimeResult<serde_json::Value>
where
    F: FnMut(JsonValue),
{
    execute_tool_with_workflows_internal(
        tool,
        mcp_servers,
        workflow_store,
        tool_registry,
        arguments,
        &mut preview_callback,
        Some(Instant::now() + timeout.max(Duration::from_millis(1))),
        Some(cancellation),
    )
}

fn execute_tool_with_workflows_internal(
    tool: &ToolDefinition,
    mcp_servers: &[loom_mcp::McpServerConfig],
    workflow_store: &WorkflowStore,
    tool_registry: &ToolRegistry,
    arguments: JsonValue,
    preview_callback: &mut dyn FnMut(JsonValue),
    deadline: Option<Instant>,
    cancellation: Option<&AtomicBool>,
) -> WorkflowRuntimeResult<serde_json::Value> {
    check_cancelled(cancellation)?;
    let arguments = prepare_tool_arguments(tool, arguments)?;
    match &tool.execution {
        ToolExecution::Workflow {
            workflow_id,
            workflow_bindings,
        } => execute_workflow_tool(
            tool,
            workflow_id,
            workflow_bindings.as_ref(),
            mcp_servers,
            workflow_store,
            tool_registry,
            arguments,
            preview_callback,
            deadline,
            cancellation,
        ),
        _ => {
            let result = match (remaining_timeout(deadline)?, cancellation) {
                (Some(timeout), Some(cancellation)) => execute_tool_with_timeout_and_cancellation(
                    tool,
                    mcp_servers,
                    arguments,
                    timeout,
                    cancellation,
                ),
                (Some(timeout), None) => {
                    execute_tool_with_timeout(tool, mcp_servers, arguments, timeout)
                }
                (None, _) => execute_tool(tool, mcp_servers, arguments),
            }
            .map_err(WorkflowRuntimeError::from)?;
            check_cancelled(cancellation)?;
            Ok(result)
        }
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

fn load_stored_workflow(
    workflow_store: &WorkflowStore,
    workflow_id: &str,
) -> WorkflowRuntimeResult<StoredWorkflow> {
    let workflow_yaml = workflow_store.load_workflow(workflow_id)?;
    serde_yaml::from_str(&workflow_yaml).map_err(|source| WorkflowRuntimeError::WorkflowYaml {
        workflow_id: workflow_id.to_owned(),
        source,
    })
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

fn execute_workflow_tool(
    root_tool: &ToolDefinition,
    workflow_id: &str,
    workflow_bindings: Option<&WorkflowExecutionBindings>,
    mcp_servers: &[loom_mcp::McpServerConfig],
    workflow_store: &WorkflowStore,
    tool_registry: &ToolRegistry,
    arguments: JsonValue,
    preview_callback: &mut dyn FnMut(JsonValue),
    deadline: Option<Instant>,
    cancellation: Option<&AtomicBool>,
) -> WorkflowRuntimeResult<JsonValue> {
    check_cancelled(cancellation)?;
    let workflow = load_stored_workflow(workflow_store, workflow_id)?;
    let preview_policy = workflow_bindings
        .and_then(|bindings| bindings.preview_output.as_ref())
        .map(|preview_output| {
            validate_preview_policy(
                workflow_id,
                &workflow,
                workflow_bindings.expect("preview output requires bindings"),
                preview_output,
                root_tool,
                tool_registry,
            )
        })
        .transpose()?;

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
    let mut preview_emitted = false;

    while !pending.is_empty() {
        check_cancelled(cancellation)?;
        let mut ready = order
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

        if let Some(policy) = &preview_policy {
            ready.sort_by_key(|node_id| !policy.priority_nodes.contains(node_id));
        }

        if ready.is_empty() {
            return Err(WorkflowRuntimeError::UnresolvedDependencies {
                workflow_id: workflow_id.to_owned(),
            });
        }

        for node_id in ready {
            check_cancelled(cancellation)?;
            remaining_timeout(deadline)?;
            let node = pending
                .remove(&node_id)
                .expect("ready node is present in pending map");
            let result = execute_workflow_node(
                workflow_id,
                root_tool,
                &node,
                workflow_bindings,
                &root_input,
                &arguments,
                &results,
                mcp_servers,
                workflow_store,
                tool_registry,
                deadline,
                cancellation,
            )?;
            check_cancelled(cancellation)?;
            results.insert(node_id, result);
            if !preview_emitted {
                if let Some(policy) = &preview_policy {
                    if policy
                        .required_nodes
                        .iter()
                        .all(|required| results.contains_key(required))
                    {
                        let preview = select_bound_workflow_output(&policy.output, &results)
                            .ok_or_else(|| WorkflowRuntimeError::InvalidPreviewPolicy {
                                workflow_id: workflow_id.to_owned(),
                                reason: format!(
                                    "node `{}` did not produce output `{}`",
                                    policy.output.node_id, policy.output.output
                                ),
                            })?;
                        preview_callback(preview);
                        preview_emitted = true;
                    }
                }
            }
        }
    }

    Ok(select_workflow_output(
        workflow_id,
        &workflow.nodes,
        workflow_bindings.and_then(|bindings| bindings.primary_output.as_ref()),
        &results,
    ))
}

#[derive(Debug)]
struct WorkflowPreviewPolicy {
    output: WorkflowOutputBinding,
    required_nodes: BTreeSet<String>,
    priority_nodes: BTreeSet<String>,
}

fn validate_preview_policy(
    workflow_id: &str,
    workflow: &StoredWorkflow,
    bindings: &WorkflowExecutionBindings,
    preview_output: &WorkflowOutputBinding,
    root_tool: &ToolDefinition,
    tool_registry: &ToolRegistry,
) -> WorkflowRuntimeResult<WorkflowPreviewPolicy> {
    let nodes = workflow
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect::<HashMap<_, _>>();
    let preview_node = nodes.get(preview_output.node_id.as_str()).ok_or_else(|| {
        WorkflowRuntimeError::InvalidPreviewPolicy {
            workflow_id: workflow_id.to_owned(),
            reason: format!("preview node `{}` does not exist", preview_output.node_id),
        }
    })?;

    if !preview_output.output.trim().is_empty()
        && !is_sticker_art(&preview_node.uses)
        && !loom_native_image::is_native_art_id(&preview_node.uses)
    {
        if let Some(tool) =
            resolve_workflow_child_tool(root_tool, &preview_node.uses, tool_registry)?
        {
            let output_names = tool
                .outputs
                .iter()
                .filter_map(|output| output.get("name").and_then(JsonValue::as_str))
                .collect::<BTreeSet<_>>();
            if !output_names.is_empty() && !output_names.contains(preview_output.output.as_str()) {
                return Err(WorkflowRuntimeError::InvalidPreviewPolicy {
                    workflow_id: workflow_id.to_owned(),
                    reason: format!(
                        "node `{}` has no output `{}`",
                        preview_output.node_id, preview_output.output
                    ),
                });
            }
        }
    }

    let mut required_nodes = bindings
        .preview_required_nodes
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    required_nodes.insert(preview_output.node_id.clone());
    if let Some(missing) = required_nodes
        .iter()
        .find(|node_id| !nodes.contains_key(node_id.as_str()))
    {
        return Err(WorkflowRuntimeError::InvalidPreviewPolicy {
            workflow_id: workflow_id.to_owned(),
            reason: format!("required node `{missing}` does not exist"),
        });
    }

    let mut priority_nodes = required_nodes.clone();
    let mut stack = required_nodes.iter().cloned().collect::<Vec<_>>();
    while let Some(node_id) = stack.pop() {
        let node = nodes.get(node_id.as_str()).ok_or_else(|| {
            WorkflowRuntimeError::InvalidPreviewPolicy {
                workflow_id: workflow_id.to_owned(),
                reason: format!("preview dependency `{node_id}` does not exist"),
            }
        })?;
        for dependency in &node.needs {
            if !nodes.contains_key(dependency.as_str()) {
                return Err(WorkflowRuntimeError::InvalidPreviewPolicy {
                    workflow_id: workflow_id.to_owned(),
                    reason: format!("preview dependency `{dependency}` does not exist"),
                });
            }
            if priority_nodes.insert(dependency.clone()) {
                stack.push(dependency.clone());
            }
        }
    }

    Ok(WorkflowPreviewPolicy {
        output: preview_output.clone(),
        required_nodes,
        priority_nodes,
    })
}

#[allow(clippy::too_many_arguments)]
fn execute_workflow_node(
    workflow_id: &str,
    parent_tool: &ToolDefinition,
    node: &StoredWorkflowNode,
    workflow_bindings: Option<&WorkflowExecutionBindings>,
    root_input: &Option<String>,
    root_arguments: &JsonValue,
    results: &HashMap<String, JsonValue>,
    mcp_servers: &[loom_mcp::McpServerConfig],
    workflow_store: &WorkflowStore,
    tool_registry: &ToolRegistry,
    deadline: Option<Instant>,
    cancellation: Option<&AtomicBool>,
) -> WorkflowRuntimeResult<JsonValue> {
    check_cancelled(cancellation)?;
    let (mut child_args, mut child_input) = resolve_node_params(node, results);

    let missing_explicit_image_binding = workflow_bindings
        .map(|bindings| {
            apply_input_bindings(
                bindings,
                node,
                root_arguments,
                root_input,
                &mut child_input,
                &mut child_args,
            )
        })
        .unwrap_or(false);

    if missing_explicit_image_binding {
        return Err(WorkflowRuntimeError::MissingImageInput {
            workflow_id: workflow_id.to_owned(),
            node_id: node.id.clone(),
        });
    }

    if child_input.is_none() {
        child_input = extract_root_input(&JsonValue::Object(child_args.clone()));
    }
    if child_input.is_none() {
        // Hook canvas edges are serialized as `needs`. When the node has no
        // explicit input binding, keep the visual pipeline semantics by
        // forwarding the first upstream image along that edge.
        child_input = node
            .needs
            .iter()
            .find_map(|dependency| results.get(dependency).and_then(json_value_as_image));
    }
    if child_input.is_none() && node.needs.is_empty() {
        child_input = root_input.clone();
    }
    if child_input.is_none() {
        child_input = node
            .meta
            .as_ref()
            .and_then(|meta| meta.preview_src.as_deref().or(meta.src.as_deref()))
            .and_then(normalize_image_reference);
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

    let child_tool = resolve_workflow_child_tool(parent_tool, &node.uses, tool_registry)?
        .ok_or_else(|| WorkflowRuntimeError::ChildToolNotFound {
            workflow_id: workflow_id.to_owned(),
            tool_id: node.uses.clone(),
        })?;

    execute_tool_with_workflows_internal(
        &child_tool,
        mcp_servers,
        workflow_store,
        tool_registry,
        JsonValue::Object(child_args),
        &mut |_| {},
        deadline,
        cancellation,
    )
}

fn check_cancelled(cancellation: Option<&AtomicBool>) -> WorkflowRuntimeResult<()> {
    if cancellation.is_some_and(|token| token.load(Ordering::Acquire)) {
        return Err(WorkflowRuntimeError::Cancelled);
    }
    Ok(())
}

fn remaining_timeout(deadline: Option<Instant>) -> WorkflowRuntimeResult<Option<Duration>> {
    let Some(deadline) = deadline else {
        return Ok(None);
    };
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(WorkflowRuntimeError::Timeout);
    }
    Ok(Some(remaining))
}

fn resolve_workflow_child_tool(
    parent_tool: &ToolDefinition,
    child_id: &str,
    tool_registry: &ToolRegistry,
) -> WorkflowRuntimeResult<Option<ToolDefinition>> {
    let locked = parent_tool
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("artPackage"))
        .and_then(|package| package.get("lockedArts"))
        .and_then(JsonValue::as_object);
    if let Some(locked) = locked {
        if let Some(tool) = locked.get(child_id) {
            return serde_json::from_value::<ToolDefinition>(tool.clone())
                .map(Some)
                .map_err(|error| ToolRegistryError::InvalidToolDefinition {
                    id: child_id.to_owned(),
                    reason: format!("locked workflow child is invalid: {error}"),
                })
                .map_err(WorkflowRuntimeError::from);
        }
        for tool in locked.values() {
            let candidate =
                serde_json::from_value::<ToolDefinition>(tool.clone()).map_err(|error| {
                    ToolRegistryError::InvalidToolDefinition {
                        id: child_id.to_owned(),
                        reason: format!("locked workflow child is invalid: {error}"),
                    }
                })?;
            if candidate.qualified_id() == child_id {
                return Ok(Some(candidate));
            }
        }
    }

    let declared_locked_dependency = parent_tool
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("dependencies"))
        .and_then(|dependencies| dependencies.get("arts"))
        .and_then(JsonValue::as_array)
        .is_some_and(|arts| {
            arts.iter().any(|dependency| {
                dependency
                    .as_str()
                    .is_some_and(|dependency| dependency == child_id)
            })
        });
    if parent_tool
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("artPackage"))
        .is_some()
        && declared_locked_dependency
    {
        return Ok(None);
    }

    tool_registry
        .get_tool(child_id)
        .map_err(WorkflowRuntimeError::from)
}

fn resolve_node_params(
    node: &StoredWorkflowNode,
    results: &HashMap<String, JsonValue>,
) -> (JsonMap<String, JsonValue>, Option<String>) {
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

    (child_args, child_input)
}

fn apply_input_bindings(
    bindings: &WorkflowExecutionBindings,
    node: &StoredWorkflowNode,
    root_arguments: &JsonValue,
    root_input: &Option<String>,
    child_input: &mut Option<String>,
    child_args: &mut JsonMap<String, JsonValue>,
) -> bool {
    let mut missing_image_binding = false;

    for binding in bindings
        .inputs
        .iter()
        .filter(|binding| binding.node_id == node.id)
    {
        match binding.kind.as_str() {
            "input_image" => {
                if let Some(value) = bound_argument_as_image(binding, root_arguments, root_input) {
                    *child_input = Some(value);
                } else {
                    missing_image_binding = true;
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

    missing_image_binding
}

fn bound_argument_value(
    binding: &WorkflowInputBinding,
    root_arguments: &JsonValue,
) -> Option<JsonValue> {
    let arguments = root_arguments.as_object()?;
    arguments
        .get(&binding.workflow_param)
        .or_else(|| {
            arguments
                .get("params")
                .and_then(JsonValue::as_object)
                .and_then(|params| params.get(&binding.workflow_param))
        })
        .or_else(|| {
            arguments
                .get("inputs")
                .and_then(JsonValue::as_object)
                .and_then(|inputs| inputs.get(&binding.workflow_param))
        })
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

fn select_bound_workflow_output(
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
    let target = target.strip_prefix("params.").unwrap_or(target);
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
                .and_then(normalize_image_reference)
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
                .and_then(|value| normalize_image_reference(&value))
        })
}

fn json_value_as_image(value: &JsonValue) -> Option<String> {
    value
        .as_str()
        .and_then(normalize_image_reference)
        .or_else(|| {
            value
                .get("data")
                .and_then(JsonValue::as_str)
                .and_then(normalize_image_reference)
        })
        .or_else(|| {
            extract_image_output(value)
                .as_deref()
                .and_then(normalize_image_reference)
        })
}

fn normalize_image_reference(value: &str) -> Option<String> {
    let value = value.trim();
    if is_image_like(value) {
        return Some(value.to_owned());
    }

    let path = Path::new(value);
    if !path.is_file() {
        return None;
    }

    loom_image_io::read_image_path_as_data_url(path).ok()
}

fn is_sticker_art(uses: &str) -> bool {
    uses == "__sticker__"
}

fn is_image_like(value: &str) -> bool {
    value.starts_with("data:image/") || loom_image_io::looks_like_base64_image_payload(value)
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
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;
    use std::time::{SystemTime, UNIX_EPOCH};

    use loom_tool_registry::{
        ToolDefinition, ToolExecution, ToolRegistry, WorkflowExecutionBindings,
        WorkflowInputBinding, WorkflowOutputBinding,
    };
    use loom_workflow_store::WorkflowStore;
    use serde_json::json;

    use super::*;

    const TEST_IMAGE: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGPgEpH7DwABpAE8k4sOtwAAAABJRU5ErkJggg==";
    const TEST_REFERENCE_IMAGE: &str = "data:image/png;base64,cmVmZXJlbmNlLWltYWdl";

    fn temp_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("loom-workflow-runtime-{name}-{nonce}"));
        fs::create_dir_all(&root).expect("create temp workflow runtime root");
        root
    }

    fn workflow_tool(workflow_id: &str) -> ToolDefinition {
        let mut tool = ToolDefinition::new(
            "fixture-workflow",
            "Fixture Workflow",
            "Workflow child runner",
            ToolExecution::Workflow {
                workflow_id: workflow_id.to_owned(),
                workflow_bindings: None,
            },
        );
        tool.metadata = Some(json!({
            "packageSecurity": {
                "publisher": { "id": "test.publisher", "name": "Test Publisher" }
            }
        }));
        tool
    }

    #[test]
    fn packaged_workflow_resolves_its_immutable_locked_child_instead_of_active_registry() {
        let root = temp_root("locked-child-resolution");
        let registry = ToolRegistry::new(root.join("tools"));
        let mut active = ToolDefinition::new(
            "child",
            "Active v2",
            "active child",
            ToolExecution::CloudApi {
                endpoint: "https://active.invalid".to_owned(),
                method: "POST".to_owned(),
                content_type: None,
                headers: None,
                body: None,
            },
        );
        active.metadata = Some(json!({
            "packageSecurity": {
                "publisher": { "id": "test.publisher", "name": "Test Publisher" }
            }
        }));
        registry.save_tool(active).expect("save active child");
        let mut locked = ToolDefinition::new(
            "child",
            "Locked v1",
            "locked child",
            ToolExecution::CloudApi {
                endpoint: "https://locked.invalid".to_owned(),
                method: "POST".to_owned(),
                content_type: None,
                headers: None,
                body: None,
            },
        );
        locked.metadata = Some(json!({
            "packageSecurity": {
                "publisher": { "id": "test.publisher", "name": "Test Publisher" }
            }
        }));
        let mut parent = workflow_tool("locked-workflow");
        parent.metadata = Some(json!({
            "dependencies": { "arts": ["test.publisher/child"] },
            "artPackage": {
                "lockedArts": { "test.publisher/child": locked }
            }
        }));

        let resolved = resolve_workflow_child_tool(&parent, "test.publisher/child", &registry)
            .expect("resolve child")
            .expect("locked child");
        assert_eq!(resolved.name, "Locked v1");

        let mut missing = parent.clone();
        missing.metadata.as_mut().unwrap()["artPackage"]["lockedArts"] = json!({});
        assert!(
            resolve_workflow_child_tool(&missing, "test.publisher/child", &registry)
                .expect("resolve missing lock")
                .is_none()
        );
        fs::remove_dir_all(root).expect("cleanup locked child resolution root");
    }

    fn workflow_tool_with_bindings(
        workflow_id: &str,
        bindings: WorkflowExecutionBindings,
    ) -> ToolDefinition {
        ToolDefinition::new(
            "fixture-workflow",
            "Fixture Workflow",
            "Workflow child runner",
            ToolExecution::Workflow {
                workflow_id: workflow_id.to_owned(),
                workflow_bindings: Some(bindings),
            },
        )
    }

    fn output_binding(node_id: &str, output: &str) -> WorkflowOutputBinding {
        WorkflowOutputBinding {
            node_id: node_id.to_owned(),
            output: output.to_owned(),
            kind: "node_result".to_owned(),
        }
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

    #[test]
    fn workflow_runtime_forwards_images_across_needs_edges() {
        let root = temp_root("implicit-image-edges");
        let workflow_store = WorkflowStore::new(root.join("workflows"));
        let tool_registry = ToolRegistry::new(root.join("tools"));
        workflow_store
            .save_workflow(
                "sticker-chain",
                r#"name: Sticker Chain
nodes:
  - id: a
    uses: __sticker__
  - id: b
    uses: __sticker__
    needs: [a]
  - id: c
    uses: __sticker__
    needs: [b]
"#,
            )
            .expect("save workflow");

        let result = execute_tool_with_workflows(
            &workflow_tool("sticker-chain"),
            &[],
            &workflow_store,
            &tool_registry,
            json!({ "input_base64": TEST_IMAGE }),
        )
        .expect("execute sticker chain");

        assert_eq!(result["content"][0]["type"], "image");
        assert_eq!(result["content"][0]["data"], TEST_IMAGE);
        fs::remove_dir_all(root).expect("cleanup temp workflow runtime root");
    }

    #[test]
    fn workflow_runtime_emits_preview_before_non_required_formal_failure() {
        let root = temp_root("preview-before-final");
        let workflow_store = WorkflowStore::new(root.join("workflows"));
        let tool_registry = ToolRegistry::new(root.join("tools"));
        workflow_store
            .save_workflow(
                "preview-before-final",
                r#"name: Preview Before Final
nodes:
  - id: formal
    uses: test.publisher/missing-formal-tool
  - id: preview
    uses: __sticker__
"#,
            )
            .expect("save workflow");
        let bindings = WorkflowExecutionBindings {
            preview_output: Some(output_binding("preview", "output_image")),
            preview_required_nodes: vec!["preview".to_owned()],
            primary_output: Some(output_binding("formal", "result")),
            ..WorkflowExecutionBindings::default()
        };
        let mut previews = Vec::new();

        let error = execute_tool_with_workflows_and_preview(
            &workflow_tool_with_bindings("preview-before-final", bindings),
            &[],
            &workflow_store,
            &tool_registry,
            json!({ "input_base64": TEST_IMAGE }),
            |preview| previews.push(preview),
        )
        .expect_err("formal node should still execute and fail");

        assert!(matches!(
            error,
            WorkflowRuntimeError::ChildToolNotFound { .. }
        ));
        assert_eq!(previews.len(), 1);
        assert_eq!(previews[0]["content"][0]["data"], TEST_IMAGE);
        fs::remove_dir_all(root).expect("cleanup temp workflow runtime root");
    }

    #[test]
    fn workflow_runtime_required_node_blocks_preview() {
        let root = temp_root("required-blocks-preview");
        let workflow_store = WorkflowStore::new(root.join("workflows"));
        let tool_registry = ToolRegistry::new(root.join("tools"));
        workflow_store
            .save_workflow(
                "required-blocks-preview",
                r#"name: Required Blocks Preview
nodes:
  - id: required
    uses: test.publisher/missing-required-tool
  - id: preview
    uses: __sticker__
"#,
            )
            .expect("save workflow");
        let bindings = WorkflowExecutionBindings {
            preview_output: Some(output_binding("preview", "output_image")),
            preview_required_nodes: vec!["required".to_owned(), "preview".to_owned()],
            ..WorkflowExecutionBindings::default()
        };
        let mut previews = Vec::new();

        let error = execute_tool_with_workflows_and_preview(
            &workflow_tool_with_bindings("required-blocks-preview", bindings),
            &[],
            &workflow_store,
            &tool_registry,
            json!({ "input_base64": TEST_IMAGE }),
            |preview| previews.push(preview),
        )
        .expect_err("required node should fail before preview publication");

        assert!(matches!(
            error,
            WorkflowRuntimeError::ChildToolNotFound { .. }
        ));
        assert!(previews.is_empty());
        fs::remove_dir_all(root).expect("cleanup temp workflow runtime root");
    }

    #[test]
    fn workflow_runtime_keeps_preview_and_formal_outputs_separate() {
        let root = temp_root("separate-preview-formal");
        let workflow_store = WorkflowStore::new(root.join("workflows"));
        let tool_registry = ToolRegistry::new(root.join("tools"));
        workflow_store
            .save_workflow(
                "separate-preview-formal",
                r#"name: Separate Preview And Formal
nodes:
  - id: preview
    uses: __sticker__
  - id: formal
    uses: __sticker__
"#,
            )
            .expect("save workflow");
        let bindings = WorkflowExecutionBindings {
            inputs: vec![
                WorkflowInputBinding {
                    workflow_param: "input".to_owned(),
                    node_id: "preview".to_owned(),
                    target: "image".to_owned(),
                    kind: "input_image".to_owned(),
                },
                WorkflowInputBinding {
                    workflow_param: "input_2".to_owned(),
                    node_id: "formal".to_owned(),
                    target: "image".to_owned(),
                    kind: "input_image".to_owned(),
                },
            ],
            primary_output: Some(output_binding("formal", "output_image")),
            preview_output: Some(output_binding("preview", "output_image")),
            preview_required_nodes: vec!["preview".to_owned()],
        };
        let mut previews = Vec::new();

        let result = execute_tool_with_workflows_and_preview(
            &workflow_tool_with_bindings("separate-preview-formal", bindings),
            &[],
            &workflow_store,
            &tool_registry,
            json!({
                "input": TEST_IMAGE,
                "input_2": TEST_REFERENCE_IMAGE,
            }),
            |preview| previews.push(preview),
        )
        .expect("execute workflow");

        assert_eq!(previews.len(), 1);
        assert_eq!(previews[0]["content"][0]["data"], TEST_IMAGE);
        assert_eq!(result["content"][0]["data"], TEST_REFERENCE_IMAGE);
        fs::remove_dir_all(root).expect("cleanup temp workflow runtime root");
    }

    #[test]
    fn workflow_runtime_without_preview_binding_keeps_single_result_behavior() {
        let root = temp_root("no-preview-binding");
        let workflow_store = WorkflowStore::new(root.join("workflows"));
        let tool_registry = ToolRegistry::new(root.join("tools"));
        workflow_store
            .save_workflow(
                "no-preview-binding",
                "name: No Preview\nnodes:\n  - id: only\n    uses: __sticker__\n",
            )
            .expect("save workflow");
        let mut preview_count = 0;

        let result = execute_tool_with_workflows_and_preview(
            &workflow_tool("no-preview-binding"),
            &[],
            &workflow_store,
            &tool_registry,
            json!({ "input_base64": TEST_IMAGE }),
            |_| preview_count += 1,
        )
        .expect("execute workflow");

        assert_eq!(preview_count, 0);
        assert_eq!(result["content"][0]["data"], TEST_IMAGE);
        fs::remove_dir_all(root).expect("cleanup temp workflow runtime root");
    }

    #[test]
    fn workflow_runtime_rejects_invalid_preview_node_and_port() {
        let root = temp_root("invalid-preview-policy");
        let workflow_store = WorkflowStore::new(root.join("workflows"));
        let tool_registry = ToolRegistry::new(root.join("tools"));
        workflow_store
            .save_workflow(
                "invalid-preview-policy",
                "name: Invalid Preview\nnodes:\n  - id: child\n    uses: test.publisher/child-tool\n",
            )
            .expect("save workflow");
        let mut child = ToolDefinition::new(
            "child-tool",
            "Child Tool",
            "Preview output fixture",
            ToolExecution::CloudApi {
                endpoint: "https://example.invalid".to_owned(),
                method: "GET".to_owned(),
                content_type: None,
                headers: None,
                body: None,
            },
        );
        child.metadata = Some(json!({
            "packageSecurity": {
                "publisher": { "id": "test.publisher", "name": "Test Publisher" }
            }
        }));
        child.outputs = vec![json!({ "name": "image", "type": "image" })];
        tool_registry.save_tool(child).expect("save child tool");

        for (node_id, output) in [("missing", "image"), ("child", "missing-output")] {
            let bindings = WorkflowExecutionBindings {
                preview_output: Some(output_binding(node_id, output)),
                ..WorkflowExecutionBindings::default()
            };
            let error = execute_tool_with_workflows_and_preview(
                &workflow_tool_with_bindings("invalid-preview-policy", bindings),
                &[],
                &workflow_store,
                &tool_registry,
                json!({}),
                |_| panic!("invalid preview policy must not emit"),
            )
            .expect_err("invalid preview policy");
            assert!(matches!(
                error,
                WorkflowRuntimeError::InvalidPreviewPolicy { .. }
            ));
        }
        fs::remove_dir_all(root).expect("cleanup temp workflow runtime root");
    }

    #[test]
    fn workflow_runtime_preserves_named_secondary_image_references() {
        let node = StoredWorkflowNode {
            id: "color".to_owned(),
            uses: "color-transfer".to_owned(),
            needs: vec!["input-source".to_owned(), "reference-source".to_owned()],
            params: BTreeMap::from([
                (
                    "input".to_owned(),
                    serde_yaml::Value::String(
                        "${{ nodes.input-source.outputs.output_image }}".to_owned(),
                    ),
                ),
                (
                    "reference".to_owned(),
                    serde_yaml::Value::String(
                        "${{ nodes.reference-source.outputs.output_image }}".to_owned(),
                    ),
                ),
            ]),
            meta: None,
        };
        let results = HashMap::from([
            (
                "input-source".to_owned(),
                image_content_response(TEST_IMAGE, "image/png"),
            ),
            (
                "reference-source".to_owned(),
                image_content_response(TEST_REFERENCE_IMAGE, "image/png"),
            ),
        ]);

        let (mut child_args, child_input) = resolve_node_params(&node, &results);
        assert_eq!(child_input.as_deref(), Some(TEST_IMAGE));
        assert_eq!(
            child_args.get("reference"),
            Some(&JsonValue::String(TEST_REFERENCE_IMAGE.to_owned()))
        );
        insert_child_input(
            &mut child_args,
            child_input.as_deref().expect("primary image input"),
        );
        assert_eq!(
            child_args.get("input"),
            Some(&JsonValue::String(TEST_IMAGE.to_owned()))
        );
    }

    #[test]
    fn workflow_param_binding_overrides_baked_value_from_nested_params() {
        let node = StoredWorkflowNode {
            id: "transfer".to_owned(),
            uses: "color-transfer".to_owned(),
            needs: vec![],
            params: BTreeMap::from([("strength".to_owned(), serde_yaml::Value::Number(20.into()))]),
            meta: None,
        };
        let bindings = WorkflowExecutionBindings {
            inputs: vec![WorkflowInputBinding {
                workflow_param: "strength".to_owned(),
                node_id: node.id.clone(),
                target: "strength".to_owned(),
                kind: "param".to_owned(),
            }],
            primary_output: None,
            ..WorkflowExecutionBindings::default()
        };
        let (mut child_args, mut child_input) = resolve_node_params(&node, &HashMap::new());

        apply_input_bindings(
            &bindings,
            &node,
            &json!({ "params": { "strength": 87 } }),
            &None,
            &mut child_input,
            &mut child_args,
        );

        assert_eq!(child_args.get("strength"), Some(&json!(87)));
    }

    #[test]
    fn workflow_param_binding_keeps_baked_value_when_argument_is_missing() {
        let node = StoredWorkflowNode {
            id: "transfer".to_owned(),
            uses: "color-transfer".to_owned(),
            needs: vec![],
            params: BTreeMap::from([("strength".to_owned(), serde_yaml::Value::Number(20.into()))]),
            meta: None,
        };
        let bindings = WorkflowExecutionBindings {
            inputs: vec![WorkflowInputBinding {
                workflow_param: "strength".to_owned(),
                node_id: node.id.clone(),
                target: "strength".to_owned(),
                kind: "param".to_owned(),
            }],
            primary_output: None,
            ..WorkflowExecutionBindings::default()
        };
        let (mut child_args, mut child_input) = resolve_node_params(&node, &HashMap::new());

        apply_input_bindings(
            &bindings,
            &node,
            &json!({ "params": {} }),
            &None,
            &mut child_input,
            &mut child_args,
        );

        assert_eq!(child_args.get("strength"), Some(&json!(20)));
    }

    #[test]
    fn bound_workflow_argument_prefers_top_level_then_params_then_inputs() {
        let binding = WorkflowInputBinding {
            workflow_param: "strength".to_owned(),
            node_id: "transfer".to_owned(),
            target: "strength".to_owned(),
            kind: "param".to_owned(),
        };

        assert_eq!(
            bound_argument_value(
                &binding,
                &json!({
                    "strength": 90,
                    "params": { "strength": 80 },
                    "inputs": { "strength": 70 }
                }),
            ),
            Some(json!(90)),
        );
        assert_eq!(
            bound_argument_value(
                &binding,
                &json!({
                    "params": { "strength": 80 },
                    "inputs": { "strength": 70 }
                }),
            ),
            Some(json!(80)),
        );
        assert_eq!(
            bound_argument_value(&binding, &json!({ "inputs": { "strength": 70 } })),
            Some(json!(70)),
        );
    }

    #[test]
    fn workflow_runtime_resolves_local_path_image_bindings() {
        let root = temp_root("local-path-binding");
        let reference_path = root.join("reference.png");
        let reference_data = loom_image_io::rgba8_to_png_data_url(1, 1, &[10, 20, 30, 255])
            .expect("encode reference fixture");
        fs::write(
            &reference_path,
            loom_image_io::decode_data_url_bytes(&reference_data)
                .expect("decode reference fixture"),
        )
        .expect("write reference fixture");

        let workflow_store = WorkflowStore::new(root.join("workflows"));
        let tool_registry = ToolRegistry::new(root.join("tools"));
        let node = StoredWorkflowNode {
            id: "reference-source".to_owned(),
            uses: "__sticker__".to_owned(),
            needs: vec![],
            params: BTreeMap::new(),
            meta: None,
        };
        let bindings = WorkflowExecutionBindings {
            inputs: vec![WorkflowInputBinding {
                workflow_param: "input_2".to_owned(),
                node_id: node.id.clone(),
                target: "image".to_owned(),
                kind: "input_image".to_owned(),
            }],
            primary_output: None,
            ..WorkflowExecutionBindings::default()
        };
        let root_input = Some(TEST_IMAGE.to_owned());
        let result = execute_workflow_node(
            "local-path-flow",
            &workflow_tool("local-path-flow"),
            &node,
            Some(&bindings),
            &root_input,
            &json!({
                "input_base64": TEST_IMAGE,
                "input_2": reference_path.to_string_lossy()
            }),
            &HashMap::new(),
            &[],
            &workflow_store,
            &tool_registry,
            None,
            None,
        )
        .expect("resolve local path binding");

        let output = result["content"][0]["data"]
            .as_str()
            .expect("sticker image output");
        let decoded =
            loom_image_io::decode_image_base64_to_rgba8(output).expect("decode sticker output");
        assert_eq!(decoded.data, vec![10, 20, 30, 255]);
        fs::remove_dir_all(root).expect("cleanup local path binding root");
    }

    #[test]
    fn workflow_runtime_rejects_missing_explicit_image_binding() {
        let root = temp_root("missing-explicit-image-binding");
        let workflow_store = WorkflowStore::new(root.join("workflows"));
        let tool_registry = ToolRegistry::new(root.join("tools"));
        let node = StoredWorkflowNode {
            id: "reference-source".to_owned(),
            uses: "__sticker__".to_owned(),
            needs: vec![],
            params: BTreeMap::new(),
            meta: None,
        };
        let bindings = WorkflowExecutionBindings {
            inputs: vec![WorkflowInputBinding {
                workflow_param: "input_2".to_owned(),
                node_id: node.id.clone(),
                target: "image".to_owned(),
                kind: "input_image".to_owned(),
            }],
            primary_output: None,
            ..WorkflowExecutionBindings::default()
        };
        let root_input = Some(TEST_IMAGE.to_owned());

        let error = execute_workflow_node(
            "missing-binding-flow",
            &workflow_tool("missing-binding-flow"),
            &node,
            Some(&bindings),
            &root_input,
            &json!({ "input_base64": TEST_IMAGE }),
            &HashMap::new(),
            &[],
            &workflow_store,
            &tool_registry,
            None,
            None,
        )
        .expect_err("missing input_2 must not reuse the root input");

        assert!(matches!(
            error,
            WorkflowRuntimeError::MissingImageInput {
                workflow_id,
                node_id
            } if workflow_id == "missing-binding-flow" && node_id == "reference-source"
        ));
        fs::remove_dir_all(root).expect("cleanup missing binding root");
    }

    #[test]
    fn workflow_runtime_rejects_an_already_cancelled_request() {
        let root = temp_root("already-cancelled");
        let workflow_store = WorkflowStore::new(root.join("workflows"));
        workflow_store
            .save_workflow("cancelled-flow", "name: Cancelled\nnodes: []\n")
            .expect("save cancelled workflow");
        let tool_registry = ToolRegistry::new(root.join("tools"));
        let cancellation = AtomicBool::new(true);

        let error = execute_tool_with_workflows_and_preview_timeout_and_cancellation(
            &workflow_tool("cancelled-flow"),
            &[],
            &workflow_store,
            &tool_registry,
            json!({}),
            Duration::from_secs(1),
            &cancellation,
            |_| {},
        )
        .expect_err("cancelled workflow must not execute");

        assert!(matches!(error, WorkflowRuntimeError::Cancelled));
        fs::remove_dir_all(root).expect("cleanup cancelled workflow root");
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
    uses: test.publisher/fixture-script
    needs: [b]
  - id: b
    uses: test.publisher/fixture-script
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
}
