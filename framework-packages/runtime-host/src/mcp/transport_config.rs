// Credential-aware environment/header construction and runtime path expansion.
fn build_environment(
    request: &FrameworkExecuteRequest,
    config: &FrameworkMcpServer,
) -> Result<BTreeMap<String, String>, String> {
    let declared_entries = config
        .env
        .len()
        .saturating_add(config.credential_env.len())
        .saturating_add(config.optional_credential_env.len());
    if declared_entries > MAX_MCP_ENVIRONMENT_ENTRIES {
        return Err(format!(
            "MCP environment declares {declared_entries} entries; limit is {MAX_MCP_ENVIRONMENT_ENTRIES}"
        ));
    }
    let mut environment = config
        .env
        .iter()
        .map(|(name, value)| {
            validate_environment_name(name)?;
            Ok((
                name.clone(),
                expand_runtime_paths(value, request, &request.art_dir),
            ))
        })
        .collect::<Result<BTreeMap<_, _>, String>>()?;

    for (environment_name, credential_name) in &config.credential_env {
        validate_environment_name(environment_name)?;
        let credential_name = credential_name.trim();
        if credential_name.is_empty() {
            return Err(format!(
                "MCP credential mapping for `{environment_name}` is empty"
            ));
        }
        let credential = request
            .context
            .credentials
            .iter()
            .find(|credential| credential.name == credential_name)
            .ok_or_else(|| {
                format!(
                    "MCP Art requires credential `{credential_name}` for `{environment_name}`; available aliases: {}",
                    available_credential_aliases(request)
                )
            })?;
        environment.insert(environment_name.clone(), credential.value.clone());
    }
    for (environment_name, credential_name) in &config.optional_credential_env {
        validate_environment_name(environment_name)?;
        // A name in both maps means the optional mapping would silently overwrite the required one
        // with a different credential, and only for operators who happen to hold that alias.
        if config.credential_env.contains_key(environment_name) {
            return Err(format!(
                "MCP credential mapping for `{environment_name}` is declared as both required and optional"
            ));
        }
        let credential_name = credential_name.trim();
        if credential_name.is_empty() {
            return Err(format!(
                "MCP optional credential mapping for `{environment_name}` is empty"
            ));
        }
        if let Some(credential) = request
            .context
            .credentials
            .iter()
            .find(|credential| credential.name == credential_name)
        {
            environment.insert(environment_name.clone(), credential.value.clone());
        }
    }
    validate_mcp_environment(&environment).map_err(|error| error.to_string())?;
    Ok(environment)
}

fn build_headers(
    request: &FrameworkExecuteRequest,
    config: &FrameworkMcpServer,
) -> Result<BTreeMap<String, String>, String> {
    let declared_entries = config
        .headers
        .len()
        .saturating_add(config.credential_headers.len())
        .saturating_add(config.optional_credential_headers.len());
    if declared_entries > MAX_MCP_HEADERS {
        return Err(format!(
            "MCP headers declare {declared_entries} entries; limit is {MAX_MCP_HEADERS}"
        ));
    }
    let mut headers = config
        .headers
        .iter()
        .map(|(name, value)| {
            validate_header_name(name)?;
            Ok((name.clone(), value.clone()))
        })
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    for (header_name, credential_name) in &config.credential_headers {
        validate_header_name(header_name)?;
        let credential_name = credential_name.trim();
        if credential_name.is_empty() {
            return Err(format!(
                "MCP credential mapping for header `{header_name}` is empty"
            ));
        }
        let credential = request
            .context
            .credentials
            .iter()
            .find(|credential| credential.name == credential_name)
            .ok_or_else(|| {
                format!(
                    "MCP Art requires credential `{credential_name}` for `{header_name}`; available aliases: {}",
                    available_credential_aliases(request)
                )
            })?;
        headers.insert(header_name.clone(), credential.value.clone());
    }
    for (header_name, credential_name) in &config.optional_credential_headers {
        validate_header_name(header_name)?;
        // Same collision as `build_environment`: the optional mapping must not be able to redirect
        // a required credential header to a weaker alias.
        if config.credential_headers.contains_key(header_name) {
            return Err(format!(
                "MCP credential mapping for header `{header_name}` is declared as both required and optional"
            ));
        }
        let credential_name = credential_name.trim();
        if credential_name.is_empty() {
            return Err(format!(
                "MCP optional credential mapping for header `{header_name}` is empty"
            ));
        }
        if let Some(credential) = request
            .context
            .credentials
            .iter()
            .find(|credential| credential.name == credential_name)
        {
            headers.insert(header_name.clone(), credential.value.clone());
        }
    }
    validate_mcp_headers(&headers).map_err(|error| error.to_string())?;
    Ok(headers)
}

fn available_credential_aliases(request: &FrameworkExecuteRequest) -> String {
    let aliases = request
        .context
        .credentials
        .iter()
        .map(|credential| credential.name.as_str())
        .collect::<Vec<_>>();
    if aliases.is_empty() {
        "<none>".to_owned()
    } else {
        aliases.join(", ")
    }
}

fn validate_header_name(name: &str) -> Result<(), String> {
    validate_mcp_header_name(name).map_err(|error| error.to_string())
}

fn validate_environment_name(name: &str) -> Result<(), String> {
    validate_mcp_environment_name(name).map_err(|error| error.to_string())
}

fn expand_stdio_command(
    command: &str,
    request: &FrameworkExecuteRequest,
    art_dir: &Path,
) -> Result<String, String> {
    let trimmed = command.trim();
    if trimmed.contains("{cacheDir}") || trimmed.contains("{tempDir}") {
        return Err(
            "resolved stdio MCP command may only use the anchored {artDir} path placeholder"
                .to_owned(),
        );
    }
    let uses_art_dir = trimmed.contains("{artDir}");
    if uses_art_dir
        && !(trimmed == "{artDir}"
            || trimmed.starts_with("{artDir}/")
            || trimmed.starts_with("{artDir}\\"))
    {
        return Err(
            "{artDir} in an MCP command must be the complete leading path segment".to_owned(),
        );
    }
    let expanded = expand_runtime_paths(trimmed, request, art_dir);
    if expanded.contains('{') || expanded.contains('}') {
        return Err("resolved stdio MCP command contains an unsupported placeholder".to_owned());
    }
    if uses_art_dir {
        if !art_dir.is_absolute() {
            return Err(
                "{artDir} MCP command expansion requires an absolute Art directory".to_owned(),
            );
        }
        let expanded_path = Path::new(&expanded);
        if !expanded_path.is_absolute()
            || !expanded_path.starts_with(art_dir)
            || expanded_path
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err("expanded {artDir} MCP command escapes the Art directory".to_owned());
        }
    }
    Ok(expanded)
}

fn expand_runtime_paths(value: &str, request: &FrameworkExecuteRequest, art_dir: &Path) -> String {
    value
        .replace("{artDir}", &art_dir.to_string_lossy())
        .replace("{cacheDir}", &request.context.cache_dir.to_string_lossy())
        .replace("{tempDir}", &request.context.temp_dir.to_string_lossy())
}
