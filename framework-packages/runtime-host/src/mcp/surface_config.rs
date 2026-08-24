// Surface action, argument alias, and binding-path validation.
fn validate_surface_actions(config: &McpArtConfig) -> Result<(), String> {
    if config.surface_actions.len() > 32 {
        return Err(
            "MCP Art metadata.mcp.surfaceActions cannot contain more than 32 actions".to_owned(),
        );
    }
    let call_ids = if config.calls.is_empty() {
        BTreeSet::from(["default"])
    } else {
        config.calls.iter().map(|call| call.id.as_str()).collect()
    };
    for (action_id, action) in &config.surface_actions {
        validate_identifier(action_id, "metadata.mcp.surfaceActions action id")?;
        if let Some(selected_calls) = &action.calls {
            if selected_calls.len() > 8 {
                return Err(format!(
                    "MCP Surface action `{action_id}` cannot select more than 8 calls"
                ));
            }
            let mut selected_ids = BTreeSet::new();
            for call_id in selected_calls {
                if !selected_ids.insert(call_id.as_str()) {
                    return Err(format!(
                        "MCP Surface action `{action_id}` selects call `{call_id}` more than once"
                    ));
                }
                if !call_ids.contains(call_id.as_str()) {
                    return Err(format!(
                        "MCP Surface action `{action_id}` selects unknown call `{call_id}`"
                    ));
                }
            }
        }
        if action.arguments.len() > 32 {
            return Err(format!(
                "MCP Surface action `{action_id}` cannot bind more than 32 arguments"
            ));
        }
        for (argument_name, binding) in &action.arguments {
            validate_argument_name(argument_name)?;
            if binding.from.is_empty() || binding.from.len() > 4 {
                return Err(format!(
                    "MCP Surface argument `{argument_name}` must declare 1 to 4 source paths"
                ));
            }
            for path in &binding.from {
                validate_binding_path(path)?;
            }
        }
    }
    Ok(())
}

fn validate_argument_aliases(
    aliases: &BTreeMap<String, BTreeMap<String, String>>,
) -> Result<(), String> {
    const MAX_ALIAS_TABLES: usize = 32;
    const MAX_ALIASES_PER_ARGUMENT: usize = 32;
    const MAX_ALIAS_BYTES: usize = 128;
    const MAX_ALIAS_TOTAL_BYTES: usize = 16 * 1024;
    if aliases.len() > MAX_ALIAS_TABLES {
        return Err(format!(
            "metadata.mcp.argumentAliases cannot contain more than {MAX_ALIAS_TABLES} argument tables"
        ));
    }
    let mut total_bytes = 0_usize;
    for (argument_name, values) in aliases {
        validate_argument_name(argument_name)?;
        if values.is_empty() || values.len() > MAX_ALIASES_PER_ARGUMENT {
            return Err(format!(
                "MCP argument alias table `{argument_name}` must contain 1 to {MAX_ALIASES_PER_ARGUMENT} aliases"
            ));
        }
        total_bytes = total_bytes.saturating_add(argument_name.len());
        for (alias, canonical) in values {
            for (label, value) in [("alias", alias), ("canonical value", canonical)] {
                if value.is_empty()
                    || value.len() > MAX_ALIAS_BYTES
                    || value.chars().any(char::is_control)
                {
                    return Err(format!(
                        "MCP argument {label} for `{argument_name}` must be non-empty, at most {MAX_ALIAS_BYTES} bytes, and contain no control characters"
                    ));
                }
            }
            total_bytes = total_bytes
                .saturating_add(alias.len())
                .saturating_add(canonical.len());
        }
    }
    if total_bytes > MAX_ALIAS_TOTAL_BYTES {
        return Err(format!(
            "metadata.mcp.argumentAliases exceeds the {MAX_ALIAS_TOTAL_BYTES} byte aggregate limit"
        ));
    }
    Ok(())
}

fn validate_argument_object(value: &Value, label: &str) -> Result<(), String> {
    if value.is_null() || value.is_object() {
        Ok(())
    } else {
        Err(format!("MCP Art {label} must be a JSON object"))
    }
}

fn validate_identifier(value: &str, label: &str) -> Result<(), String> {
    let value = value.trim();
    let valid = !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'));
    valid
        .then_some(())
        .ok_or_else(|| format!("invalid {label} `{value}`"))
}

fn validate_argument_name(value: &str) -> Result<(), String> {
    let mut bytes = value.bytes();
    let valid = value.len() <= 128
        && bytes
            .next()
            .is_some_and(|byte| byte == b'_' || byte.is_ascii_alphabetic())
        && bytes.all(|byte| byte == b'_' || byte.is_ascii_alphanumeric());
    valid
        .then_some(())
        .ok_or_else(|| format!("invalid MCP Surface argument name `{value}`"))
}

fn validate_binding_path(path: &str) -> Result<(), String> {
    let segments = path.split('.').collect::<Vec<_>>();
    let root_is_allowed = matches!(
        segments.first().copied(),
        Some("payload" | "authoritativeState")
    );
    let segments_are_safe = segments.len() >= 2
        && segments.len() <= 8
        && segments.iter().all(|segment| {
            !segment.is_empty()
                && segment.len() <= 64
                && segment
                    .bytes()
                    .all(|byte| byte == b'_' || byte == b'-' || byte.is_ascii_alphanumeric())
        });
    if root_is_allowed && segments_are_safe {
        Ok(())
    } else {
        Err(format!(
            "MCP Surface binding path `{path}` must be rooted at payload or authoritativeState"
        ))
    }
}
