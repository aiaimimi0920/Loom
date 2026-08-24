// Surface binding lookup and resolved-argument resource limits.
fn resolve_surface_argument_bindings(
    action: &McpSurfaceActionConfig,
    invocation: &Value,
) -> Result<Map<String, Value>, String> {
    action
        .arguments
        .iter()
        .map(|(argument_name, binding)| {
            let value = binding
                .from
                .iter()
                .find_map(|path| value_at_binding_path(invocation, path))
                .ok_or_else(|| {
                    format!(
                        "MCP Surface argument `{argument_name}` is missing from all declared source paths"
                    )
                })?;
            validate_bound_value(argument_name, value)?;
            Ok((argument_name.clone(), value.clone()))
        })
        .collect()
}

fn value_at_binding_path<'a>(invocation: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = invocation;
    for segment in path.split('.') {
        current = current.get(segment)?;
    }
    Some(current)
}

fn validate_bound_value(argument_name: &str, value: &Value) -> Result<(), String> {
    let encoded = serde_json::to_vec(value).map_err(|error| {
        format!("cannot encode MCP Surface argument `{argument_name}`: {error}")
    })?;
    if encoded.len() > 65_536 {
        return Err(format!(
            "MCP Surface argument `{argument_name}` exceeds the 64 KiB limit"
        ));
    }
    if !value_is_within_depth(value, 0, 16) {
        return Err(format!(
            "MCP Surface argument `{argument_name}` exceeds the nesting limit"
        ));
    }
    Ok(())
}

fn value_is_within_depth(value: &Value, depth: usize, max_depth: usize) -> bool {
    if depth > max_depth {
        return false;
    }
    match value {
        Value::Array(values) => values
            .iter()
            .all(|value| value_is_within_depth(value, depth + 1, max_depth)),
        Value::Object(values) => values
            .values()
            .all(|value| value_is_within_depth(value, depth + 1, max_depth)),
        _ => true,
    }
}

fn validate_resolved_arguments(arguments: &Value) -> Result<(), String> {
    let encoded = serde_json::to_vec(arguments)
        .map_err(|error| format!("cannot encode resolved MCP arguments: {error}"))?;
    if encoded.len() > 262_144 {
        return Err("resolved MCP arguments exceed the 256 KiB limit".to_owned());
    }
    if !value_is_within_depth(arguments, 0, 24) {
        return Err("resolved MCP arguments exceed the nesting limit".to_owned());
    }
    Ok(())
}

#[cfg(test)]
fn build_arguments(request: &FrameworkExecuteRequest, configured: &Value) -> Result<Value, String> {
    build_call_arguments(request, configured, &Value::Null, "default", None)
}
