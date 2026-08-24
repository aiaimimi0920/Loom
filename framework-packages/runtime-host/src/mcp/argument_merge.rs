// Ordered merge policy for manifest, caller, and disabled arguments.
fn build_call_arguments(
    request: &FrameworkExecuteRequest,
    configured: &Value,
    call_configured: &Value,
    call_id: &str,
    caller_allowlist: Option<&BTreeSet<String>>,
) -> Result<Value, String> {
    let mut arguments = Map::new();
    merge_argument_object(&mut arguments, configured, "metadata.mcp.arguments")?;
    merge_argument_object(
        &mut arguments,
        call_configured,
        &format!("metadata.mcp.calls[{call_id}].arguments"),
    )?;
    merge_caller_arguments(&mut arguments, &request.inputs, "inputs", caller_allowlist)?;
    merge_caller_arguments(&mut arguments, &request.params, "params", caller_allowlist)?;
    for name in &request.disabled_params {
        arguments.remove(name);
    }
    Ok(Value::Object(arguments))
}

/// Merge caller-supplied values, minus the Surface control key and minus anything outside the
/// Art's declared argument names when it has declared any.
fn merge_caller_arguments(
    target: &mut Map<String, Value>,
    source: &Value,
    label: &str,
    allowlist: Option<&BTreeSet<String>>,
) -> Result<(), String> {
    if source.is_null() {
        return Ok(());
    }
    let source = source
        .as_object()
        .ok_or_else(|| format!("MCP Art {label} must be a JSON object"))?;
    for (name, value) in source {
        // `surfaceAction` is how a caller addresses this framework, never a tool argument. An Art
        // that declares no `surfaceActions` used to forward the whole invocation object — payload,
        // authoritative state and all — to the MCP server under that name.
        if name == SURFACE_ACTION_KEY {
            continue;
        }
        if allowlist.is_some_and(|allowlist| !allowlist.contains(name)) {
            continue;
        }
        target.insert(name.clone(), value.clone());
    }
    Ok(())
}

fn merge_argument_object(
    target: &mut Map<String, Value>,
    source: &Value,
    label: &str,
) -> Result<(), String> {
    if source.is_null() {
        return Ok(());
    }
    let source = source
        .as_object()
        .ok_or_else(|| format!("MCP Art {label} must be a JSON object"))?;
    target.extend(
        source
            .iter()
            .map(|(name, value)| (name.clone(), value.clone())),
    );
    Ok(())
}
