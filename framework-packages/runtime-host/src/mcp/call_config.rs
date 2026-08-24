// MCP call normalization and identifier validation.
fn validate_call_config(config: &McpArtConfig) -> Result<(), String> {
    if config.calls.is_empty() {
        let Some(tool_name) = config.tool_name.as_deref() else {
            return Err(
                "MCP Art metadata.mcp.toolName or metadata.mcp.calls is required".to_owned(),
            );
        };
        // The legacy single-call shape used to be checked for emptiness only, so a megabyte-long
        // tool name or one carrying control characters reached `tools/call` verbatim on this path
        // while the multi-call path rejected it.
        return validate_tool_name(tool_name, "default");
    }
    if config.tool_name.is_some() {
        return Err(
            "MCP Art metadata.mcp.toolName cannot be combined with metadata.mcp.calls".to_owned(),
        );
    }
    if config.calls.len() > 8 {
        return Err("MCP Art metadata.mcp.calls cannot contain more than 8 calls".to_owned());
    }

    let mut ids = BTreeSet::new();
    for call in &config.calls {
        validate_identifier(&call.id, "metadata.mcp.calls[].id")?;
        if !ids.insert(call.id.as_str()) {
            return Err(format!("duplicate MCP call id `{}`", call.id));
        }
        validate_tool_name(&call.tool_name, &call.id)?;
        validate_argument_object(
            &call.arguments,
            &format!("metadata.mcp.calls[{}].arguments", call.id),
        )?;
    }
    Ok(())
}

/// Store the bytes validation accepted. `validate_identifier` trims before checking, so a call
/// declared as `" quote"` used to pass validation and then never match the `"quote"` a Surface
/// action selects — and the untrimmed id was also the key serialized back to the Surface in the
/// `results` map. Normalizing once here keeps validation, comparison, and output on the same bytes.
fn normalize_config(config: &mut McpArtConfig) -> Result<(), String> {
    trim_in_place(&mut config.server_id);
    trim_in_place(&mut config.package_id);
    trim_in_place(&mut config.version);
    if let Some(tool_name) = config.tool_name.as_mut() {
        trim_in_place(tool_name);
    }
    for call in &mut config.calls {
        trim_in_place(&mut call.id);
        trim_in_place(&mut call.tool_name);
    }
    let mut actions = BTreeMap::new();
    for (action_id, mut action) in std::mem::take(&mut config.surface_actions) {
        if let Some(selected_calls) = action.calls.as_mut() {
            for call_id in selected_calls.iter_mut() {
                trim_in_place(call_id);
            }
        }
        let action_id = action_id.trim().to_owned();
        // Two ids that differ only in whitespace collapse to one key here, and silently keeping
        // whichever sorted last would drop a declared action.
        if actions.insert(action_id.clone(), action).is_some() {
            return Err(format!(
                "duplicate MCP Surface action id `{action_id}` after trimming"
            ));
        }
    }
    config.surface_actions = actions;
    let mut aliases = BTreeMap::new();
    for (argument_name, values) in std::mem::take(&mut config.argument_aliases) {
        let argument_name = argument_name.trim().to_owned();
        let mut normalized_values = BTreeMap::new();
        for (alias, canonical) in values {
            let alias = alias.trim().to_owned();
            let canonical = canonical.trim().to_owned();
            if normalized_values.insert(alias.clone(), canonical).is_some() {
                return Err(format!(
                    "duplicate MCP argument alias `{alias}` for `{argument_name}` after trimming"
                ));
            }
        }
        if aliases
            .insert(argument_name.clone(), normalized_values)
            .is_some()
        {
            return Err(format!(
                "duplicate MCP argument alias table `{argument_name}` after trimming"
            ));
        }
    }
    config.argument_aliases = aliases;
    Ok(())
}

fn trim_in_place(value: &mut String) {
    if value.trim().len() != value.len() {
        *value = value.trim().to_owned();
    }
}

fn validate_tool_name(tool_name: &str, call_id: &str) -> Result<(), String> {
    if tool_name.trim().is_empty() || tool_name.len() > 256 {
        return Err(format!(
            "MCP call `{call_id}` must declare a non-empty toolName"
        ));
    }
    if tool_name.chars().any(char::is_control) {
        return Err(format!("MCP call `{call_id}` has an invalid toolName"));
    }
    Ok(())
}
