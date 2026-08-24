// Manifest and Surface invocation projection into bounded MCP calls.
fn resolve_calls(
    request: &FrameworkExecuteRequest,
    config: &McpArtConfig,
) -> Result<Vec<ResolvedCall>, String> {
    let configured_calls = if config.calls.is_empty() {
        vec![McpCallConfig {
            id: "default".to_owned(),
            tool_name: config
                .tool_name
                .clone()
                .ok_or_else(|| "legacy MCP Art toolName is missing".to_owned())?,
            arguments: Value::Null,
        }]
    } else {
        config.calls.clone()
    };

    let surface_action = find_surface_action(request)?;
    if let Some(surface_action) = surface_action {
        let action_id = surface_action
            .get("actionId")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "MCP Surface invocation actionId is required".to_owned())?;
        let action = config.surface_actions.get(action_id).ok_or_else(|| {
            format!("MCP Surface action `{action_id}` is not declared by this Art")
        })?;
        let mapped_arguments = resolve_surface_argument_bindings(action, surface_action)?;
        if let Some(disabled) = request
            .disabled_params
            .iter()
            .find(|name| mapped_arguments.contains_key(name.as_str()))
        {
            return Err(format!(
                "MCP Surface argument `{disabled}` is both bound by action `{action_id}` and disabled; remove the binding or enable the parameter"
            ));
        }
        let selected_ids = action.calls.clone().unwrap_or_else(|| {
            configured_calls
                .iter()
                .map(|call| call.id.clone())
                .collect()
        });
        let calls_by_id = configured_calls
            .iter()
            .map(|call| (call.id.as_str(), call))
            .collect::<BTreeMap<_, _>>();
        return selected_ids
            .iter()
            .map(|call_id| {
                let call = calls_by_id.get(call_id.as_str()).ok_or_else(|| {
                    format!("MCP Surface action `{action_id}` selected unknown call `{call_id}`")
                })?;
                let mut arguments = Map::new();
                merge_argument_object(&mut arguments, &config.arguments, "metadata.mcp.arguments")?;
                merge_argument_object(
                    &mut arguments,
                    &call.arguments,
                    &format!("metadata.mcp.calls[{call_id}].arguments"),
                )?;
                arguments.extend(
                    mapped_arguments
                        .iter()
                        .map(|(name, value)| (name.clone(), value.clone())),
                );
                for name in &request.disabled_params {
                    arguments.remove(name);
                }
                let arguments = Value::Object(arguments);
                validate_resolved_arguments(&arguments)?;
                Ok(ResolvedCall {
                    id: call.id.clone(),
                    tool_name: call.tool_name.clone(),
                    arguments,
                })
            })
            .collect();
    }

    let caller_allowlist = declared_argument_allowlist(config);
    configured_calls
        .into_iter()
        .map(|call| {
            let arguments = build_call_arguments(
                request,
                &config.arguments,
                &call.arguments,
                &call.id,
                caller_allowlist.as_ref(),
            )?;
            validate_resolved_arguments(&arguments)?;
            Ok(ResolvedCall {
                id: call.id,
                tool_name: call.tool_name,
                arguments,
            })
        })
        .collect()
}

fn dropped_argument_warnings(
    request: &FrameworkExecuteRequest,
    config: &McpArtConfig,
) -> Result<Vec<McpExecutionWarning>, String> {
    if find_surface_action(request)?.is_some() {
        return Ok(Vec::new());
    }
    let Some(allowlist) = declared_argument_allowlist(config) else {
        return Ok(Vec::new());
    };
    let mut dropped = BTreeSet::new();
    for source in [&request.inputs, &request.params] {
        let Some(source) = source.as_object() else {
            continue;
        };
        for name in source.keys() {
            if name != SURFACE_ACTION_KEY && !allowlist.contains(name) {
                dropped.insert(name.clone());
            }
        }
    }
    if dropped.is_empty() {
        return Ok(Vec::new());
    }
    let dropped_argument_count = dropped.len();
    let dropped_argument_names = dropped.into_iter().take(32).collect::<Vec<_>>();
    Ok(vec![McpExecutionWarning {
        code: "undeclared_arguments_dropped".to_owned(),
        message: format!(
            "dropped {dropped_argument_count} caller argument(s) not declared by the Art; values were not recorded"
        ),
        dropped_argument_count,
        dropped_argument_names,
    }])
}

/// The argument names an Art that declares Surface bindings is allowed to receive from the caller.
///
/// An Art that declares `surfaceActions` has told the host exactly which arguments a caller may
/// influence, and `resolve_surface_argument_bindings` holds callers to that list on the Surface
/// path. Without this, the ordinary path — any execution that carries no `surfaceAction`, which is
/// every plain render — merged `inputs` and `params` wholesale, so the allowlist was worth nothing
/// to anyone who simply left the invocation object out.
///
/// The allowlist is the union of every argument name the manifest spells out: `metadata.mcp`
/// defaults, per-call defaults, and every Surface binding target. It is not the Surface bindings
/// alone, because the plain path has no invocation object to bind against — Stock Monitor's `code`
/// arrives as an Art param there and only as `payload.code` on the Surface path — so
/// bindings-only would leave that Art unable to run at all outside a Surface action.
///
/// `None` means "no allowlist": an Art that declares no bindings has expressed no policy, and
/// filtering it would break every existing MCP Art that passes its params straight through.
fn declared_argument_allowlist(config: &McpArtConfig) -> Option<BTreeSet<String>> {
    if config.surface_actions.is_empty() {
        return None;
    }
    let mut names = BTreeSet::new();
    collect_argument_names(&config.arguments, &mut names);
    for call in &config.calls {
        collect_argument_names(&call.arguments, &mut names);
    }
    for action in config.surface_actions.values() {
        names.extend(action.arguments.keys().cloned());
    }
    Some(names)
}

fn collect_argument_names(arguments: &Value, names: &mut BTreeSet<String>) {
    if let Some(arguments) = arguments.as_object() {
        names.extend(arguments.keys().cloned());
    }
}

fn find_surface_action(request: &FrameworkExecuteRequest) -> Result<Option<&Value>, String> {
    let from_inputs = request.inputs.get(SURFACE_ACTION_KEY);
    let from_params = request.params.get(SURFACE_ACTION_KEY);
    match (from_inputs, from_params) {
        (Some(left), Some(right)) if left != right => {
            Err("conflicting MCP Surface invocations were provided".to_owned())
        }
        (Some(value), _) | (_, Some(value)) => {
            if value.is_object() {
                Ok(Some(value))
            } else {
                Err("MCP Surface invocation must be a JSON object".to_owned())
            }
        }
        (None, None) => Ok(None),
    }
}
