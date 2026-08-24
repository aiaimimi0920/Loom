// Enabled-tool filtering, capability metadata, execution defaults, and Python Art listing.
fn delete_tool(
    path_id: &str,
    tool_registry: &ToolRegistry,
    hook_bridge: &SharedHookBridgeRuntime,
) -> Result<(u16, String)> {
    let deleted = match tool_registry.delete_tool(path_id) {
        Ok(deleted) => deleted,
        Err(error) => return tool_registry_error_response(error),
    };

    if !deleted {
        return structured_error(
            404,
            json!({
                "code": "tool_not_found",
                "message": format!("tool `{path_id}` was not found"),
                "tool_id": path_id,
            }),
        );
    }

    broadcast_hook_bridge_json(hook_bridge, capabilities_updated_event());
    Ok((
        200,
        serde_json::to_string(&json!({ "toolId": path_id, "deleted": true }))?,
    ))
}

fn list_enabled_tools(tool_registry: &ToolRegistry) -> Result<(u16, String)> {
    let tools = match tool_registry.list_tools() {
        Ok(tools) => tools
            .into_iter()
            .filter(|tool| tool.enabled)
            .collect::<Vec<_>>(),
        Err(error) => return tool_registry_error_response(error),
    };
    let count = tools.len();
    Ok((
        200,
        serde_json::to_string(&json!({ "tools": tools, "count": count }))?,
    ))
}

fn broadcast_tool_capabilities_updated(hook_bridge: &SharedHookBridgeRuntime) {
    broadcast_hook_bridge_json(hook_bridge, capabilities_updated_event());
}

fn get_tool(tool_id: &str, tool_registry: &ToolRegistry) -> Result<(u16, String)> {
    let tool = match get_registered_tool(tool_id, tool_registry) {
        Ok(tool) => tool,
        Err(response) => return response,
    };
    Ok((200, serde_json::to_string(&json!({ "tool": tool }))?))
}

fn set_tool_enabled(
    tool_id: &str,
    enabled: bool,
    tool_registry: &ToolRegistry,
    hook_bridge: &SharedHookBridgeRuntime,
) -> Result<(u16, String)> {
    let mut tool = match get_registered_tool(tool_id, tool_registry) {
        Ok(tool) => tool,
        Err(response) => return response,
    };
    tool.enabled = enabled;
    let saved = match tool_registry.save_tool(tool) {
        Ok(saved) => saved,
        Err(error) => return tool_registry_error_response(error),
    };
    broadcast_hook_bridge_json(hook_bridge, capabilities_updated_event());
    Ok((200, serde_json::to_string(&json!({ "tool": saved }))?))
}

fn update_tool_defaults(
    tool_id: &str,
    body: &str,
    tool_registry: &ToolRegistry,
    hook_bridge: &SharedHookBridgeRuntime,
) -> Result<(u16, String)> {
    let request = match serde_json::from_str::<Value>(body) {
        Ok(value) => value,
        Err(error) => return invalid_request(error.to_string()),
    };
    let mut tool = match get_registered_tool(tool_id, tool_registry) {
        Ok(tool) => tool,
        Err(response) => return response,
    };
    apply_tool_defaults_update(&mut tool, &request);
    let saved = match tool_registry.save_tool(tool) {
        Ok(saved) => saved,
        Err(error) => return tool_registry_error_response(error),
    };
    broadcast_hook_bridge_json(hook_bridge, capabilities_updated_event());
    Ok((200, serde_json::to_string(&json!({ "tool": saved }))?))
}

fn get_registered_tool(
    tool_id: &str,
    tool_registry: &ToolRegistry,
) -> std::result::Result<ToolDefinition, Result<(u16, String)>> {
    match tool_registry.get_tool(tool_id) {
        Ok(Some(tool)) => Ok(tool),
        Ok(None) => Err(tool_not_found_response(tool_id)),
        Err(error) => Err(tool_registry_error_response(error)),
    }
}

fn tool_not_found_response(tool_id: &str) -> Result<(u16, String)> {
    structured_error(
        404,
        json!({
            "code": "tool_not_found",
            "message": format!("tool `{tool_id}` was not found"),
            "tool_id": tool_id,
        }),
    )
}

fn hook_image_input_names(tool: &ToolDefinition) -> Vec<String> {
    tool.inputs
        .iter()
        .filter_map(|input| {
            let object = input.as_object()?;
            let name = object.get("name").and_then(Value::as_str)?.trim();
            if name.is_empty() {
                return None;
            }
            let image_like = ["type", "data_type", "execution_type"]
                .iter()
                .filter_map(|key| object.get(*key).and_then(Value::as_str))
                .any(|value| value.to_ascii_lowercase().contains("image"))
                || object.get("widget").and_then(Value::as_str) == Some("image_link");
            image_like.then(|| name.to_owned())
        })
        .collect()
}

fn workflow_preview_tool<'a>(
    tool: &ToolDefinition,
    tools: &'a [ToolDefinition],
    workflow_store: &WorkflowStore,
) -> Option<&'a ToolDefinition> {
    let ToolExecution::Workflow {
        workflow_id,
        workflow_bindings: Some(bindings),
    } = &tool.execution
    else {
        return None;
    };
    let preview_node_id = bindings.preview_output.as_ref()?.node_id.as_str();
    let node_tool_ids = workflow_node_tool_ids(workflow_store, workflow_id).ok()?;
    let preview_tool_id = node_tool_ids.get(preview_node_id)?;
    tools.iter().find(|candidate| {
        candidate.id == *preview_tool_id || candidate.qualified_id() == *preview_tool_id
    })
}

fn tool_supports_shader_preview(tool: &ToolDefinition) -> bool {
    tool.metadata
        .as_ref()
        .and_then(|metadata| metadata.get("capabilities"))
        .is_some_and(|capabilities| {
            capabilities.get("preview").and_then(Value::as_str) == Some("shader")
                || capabilities.get("shader").and_then(Value::as_bool) == Some(true)
        })
}

fn hook_capability_parameters(
    tool: &ToolDefinition,
    tools: &[ToolDefinition],
    workflow_store: &WorkflowStore,
) -> Vec<Value> {
    let ToolExecution::Workflow {
        workflow_id,
        workflow_bindings: Some(bindings),
    } = &tool.execution
    else {
        return tool.params.clone();
    };
    let Ok(node_tool_ids) = workflow_node_tool_ids(workflow_store, workflow_id) else {
        return tool.params.clone();
    };

    tool.params
        .iter()
        .map(|param| {
            let Some(param_id) = hook_parameter_id(param) else {
                return param.clone();
            };
            let Some(binding) = bindings.inputs.iter().find(|binding| {
                binding.kind == "param" && binding.workflow_param.trim() == param_id
            }) else {
                return param.clone();
            };
            let Some(node_tool_id) = node_tool_ids.get(&binding.node_id) else {
                return param.clone();
            };
            let Some(node_tool) = find_workflow_node_tool(tools, node_tool_id) else {
                return param.clone();
            };
            let target = binding
                .target
                .strip_prefix("params.")
                .unwrap_or(&binding.target);
            let Some(node_param) = node_tool
                .params
                .iter()
                .find(|candidate| hook_parameter_id(candidate) == Some(target))
            else {
                return param.clone();
            };
            merge_hook_parameter_ui_schema(param, node_param)
        })
        .collect()
}

fn hook_parameter_id(param: &Value) -> Option<&str> {
    param.get("id").and_then(Value::as_str)
}

fn find_workflow_node_tool<'a>(
    tools: &'a [ToolDefinition],
    tool_id: &str,
) -> Option<&'a ToolDefinition> {
    if let Some(tool) = tools.iter().find(|tool| tool.qualified_id() == tool_id) {
        return Some(tool);
    }
    let mut matches = tools.iter().filter(|tool| tool.id == tool_id);
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

fn merge_hook_parameter_ui_schema(param: &Value, source: &Value) -> Value {
    const UI_SCHEMA_KEYS: &[&str] = &[
        "widget",
        "type",
        "data_type",
        "min",
        "minimum",
        "max",
        "maximum",
        "step",
        "options",
        "multiline",
        "group",
        "required",
        "secret",
    ];

    let (Some(param), Some(source)) = (param.as_object(), source.as_object()) else {
        return param.clone();
    };
    let mut merged = param.clone();
    for key in UI_SCHEMA_KEYS {
        if let Some(value) = source.get(*key) {
            merged.insert((*key).to_owned(), value.clone());
        }
    }
    Value::Object(merged)
}

fn tool_defaults_json(tool: &ToolDefinition) -> Value {
    if let Some(defaults) = tool
        .metadata
        .as_ref()
        .and_then(Value::as_object)
        .and_then(|metadata| metadata.get("artUserSettings"))
        .and_then(|settings| settings.get("defaults"))
    {
        return defaults.clone();
    }
    manifest_parameter_defaults_json(tool)
}

fn manifest_parameter_defaults_json(tool: &ToolDefinition) -> Value {
    let mut defaults = serde_json::Map::new();
    for param in &tool.params {
        let Some(param_object) = param.as_object() else {
            continue;
        };
        let key = param_object.get("id").and_then(Value::as_str);
        let Some(key) = key else {
            continue;
        };
        if let Some(default) = param_object.get("default") {
            defaults.insert(key.to_owned(), default.clone());
        }
    }
    Value::Object(defaults)
}

fn apply_tool_defaults_update(tool: &mut ToolDefinition, request: &Value) {
    if let Some(params) = request.get("params").and_then(Value::as_array) {
        tool.params = params.clone();
    }
    if let Some(inputs) = request.get("inputs").and_then(Value::as_array) {
        tool.inputs = inputs.clone();
    }
    if let Some(outputs) = request.get("outputs").and_then(Value::as_array) {
        tool.outputs = outputs.clone();
    }

    let defaults = request
        .get("defaults")
        .and_then(Value::as_object)
        .or_else(|| request.as_object());
    let Some(defaults) = defaults else {
        return;
    };
    if defaults.is_empty() {
        return;
    }

    if tool.params.is_empty() {
        tool.params = defaults
            .iter()
            .map(|(key, value)| {
                json!({
                    "id": key,
                    "default": value,
                })
            })
            .collect();
        return;
    }

    for param in &mut tool.params {
        let Some(param_object) = param.as_object_mut() else {
            continue;
        };
        let key = param_object
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_owned);
        if let Some(key) = key {
            if let Some(default_value) = defaults.get(&key) {
                param_object.insert("default".to_owned(), default_value.clone());
            }
        }
    }
}

fn list_python_arts() -> Result<(u16, String)> {
    let arts = collect_python_arts();
    Ok((200, serde_json::to_string(&json!({ "arts": arts }))?))
}
