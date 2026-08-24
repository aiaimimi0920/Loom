//! Configuration, environment, header, package-state, and command validation.

use super::*;

pub(super) fn validate_server_metadata_and_limits(config: &McpServerConfig) -> McpResult<()> {
    validate_config_identity("server id", &config.id)?;
    validate_bounded_config_text("server name", &config.name, MAX_MCP_SERVER_NAME_BYTES, true)?;
    validate_bounded_config_text(
        "server description",
        &config.description,
        MAX_MCP_SERVER_DESCRIPTION_BYTES,
        false,
    )?;
    validate_string_list(
        "entry.args",
        &config.args,
        MAX_MCP_ARGUMENTS,
        MAX_MCP_ARGUMENT_BYTES,
        false,
    )?;
    if config.tools.len() > MAX_MCP_TOOLS {
        return Err(McpError::InvalidConfig(format!(
            "tools contains {} entries; limit is {MAX_MCP_TOOLS}",
            config.tools.len()
        )));
    }
    for (index, tool) in config.tools.iter().enumerate() {
        validate_mcp_tool_identifier(&format!("tools[{index}]"), tool)?;
    }
    if config.credential_requirements.len() > MAX_MCP_CREDENTIALS {
        return Err(McpError::InvalidConfig(format!(
            "credentialRequirements contains {} entries; limit is {MAX_MCP_CREDENTIALS}",
            config.credential_requirements.len()
        )));
    }
    let mut credential_ids = std::collections::BTreeSet::new();
    for (index, requirement) in config.credential_requirements.iter().enumerate() {
        validate_config_identity(
            &format!("credentialRequirements[{index}].id"),
            &requirement.id,
        )?;
        validate_bounded_config_text(
            &format!("credentialRequirements[{index}].label"),
            &requirement.label,
            MAX_MCP_CREDENTIAL_LABEL_BYTES,
            true,
        )?;
        if !credential_ids.insert(requirement.id.as_str()) {
            return Err(McpError::InvalidConfig(format!(
                "duplicate credential requirement id `{}`",
                requirement.id
            )));
        }
    }
    validate_mcp_environment(&config.env)?;
    validate_mcp_headers(&config.headers)?;
    validate_credential_target_names(config)?;
    if let Some(package) = &config.package {
        validate_installed_package_state(config, package)?;
    }
    Ok(())
}

pub(super) fn validate_bounded_config_text(
    field: &str,
    value: &str,
    max_bytes: usize,
    required: bool,
) -> McpResult<()> {
    if required && value.trim().is_empty() {
        return Err(McpError::InvalidConfig(format!("{field} is required")));
    }
    if value.len() > max_bytes {
        return Err(McpError::InvalidConfig(format!(
            "{field} is {} bytes; limit is {max_bytes}",
            value.len()
        )));
    }
    Ok(())
}

pub(super) fn validate_string_list(
    field: &str,
    values: &[String],
    max_entries: usize,
    max_value_bytes: usize,
    require_nonempty: bool,
) -> McpResult<()> {
    if values.len() > max_entries {
        return Err(McpError::InvalidConfig(format!(
            "{field} contains {} entries; limit is {max_entries}",
            values.len()
        )));
    }
    for (index, value) in values.iter().enumerate() {
        validate_bounded_config_text(
            &format!("{field}[{index}]"),
            value,
            max_value_bytes,
            require_nonempty,
        )?;
    }
    Ok(())
}

pub(super) fn validate_config_identity(field: &str, value: &str) -> McpResult<()> {
    let valid = !value.is_empty()
        && value.len() <= 128
        && !value.contains("..")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
    if valid {
        Ok(())
    } else {
        Err(McpError::InvalidConfig(format!(
            "{field} is not a safe identifier (limit 128 bytes)"
        )))
    }
}

pub fn validate_mcp_tool_identifier(field: &str, value: &str) -> McpResult<()> {
    let valid = !value.is_empty()
        && value.len() <= MAX_MCP_TOOL_ID_BYTES
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/')
        });
    if valid {
        Ok(())
    } else {
        Err(McpError::InvalidConfig(format!(
            "{field} must be a non-empty MCP tool identifier using at most {MAX_MCP_TOOL_ID_BYTES} bytes"
        )))
    }
}

pub fn validate_mcp_environment(environment: &BTreeMap<String, String>) -> McpResult<()> {
    if environment.len() > MAX_MCP_ENVIRONMENT_ENTRIES {
        return Err(McpError::InvalidConfig(format!(
            "env contains {} entries; limit is {MAX_MCP_ENVIRONMENT_ENTRIES}",
            environment.len()
        )));
    }
    let mut total_bytes = 0usize;
    for (name, value) in environment {
        validate_mcp_environment_name(name)?;
        if value.len() > MAX_MCP_ENVIRONMENT_VALUE_BYTES {
            return Err(McpError::InvalidConfig(format!(
                "env.{name} is {} bytes; limit is {MAX_MCP_ENVIRONMENT_VALUE_BYTES}",
                value.len()
            )));
        }
        total_bytes = total_bytes.saturating_add(name.len() + value.len() + 2);
    }
    if total_bytes > MAX_MCP_ENVIRONMENT_TOTAL_BYTES {
        return Err(McpError::InvalidConfig(format!(
            "env is {total_bytes} aggregate bytes; limit is {MAX_MCP_ENVIRONMENT_TOTAL_BYTES}"
        )));
    }
    Ok(())
}

pub fn validate_mcp_environment_name(name: &str) -> McpResult<()> {
    let mut bytes = name.bytes();
    let valid = name.len() <= MAX_MCP_ENVIRONMENT_NAME_BYTES
        && bytes
            .next()
            .is_some_and(|byte| byte == b'_' || byte.is_ascii_alphabetic())
        && bytes.all(|byte| byte == b'_' || byte.is_ascii_alphanumeric());
    if !valid {
        return Err(McpError::InvalidConfig(format!(
            "invalid MCP environment variable name `{name}` (limit {MAX_MCP_ENVIRONMENT_NAME_BYTES} bytes)"
        )));
    }
    let normalized = name.to_ascii_uppercase();
    if matches!(
        normalized.as_str(),
        "PATH"
            | "PATHEXT"
            | "COMSPEC"
            | "SYSTEMROOT"
            | "WINDIR"
            | "TEMP"
            | "TMP"
            | "LD_PRELOAD"
            | "LD_LIBRARY_PATH"
            | "DYLD_INSERT_LIBRARIES"
            | "DYLD_LIBRARY_PATH"
            | "NODE_OPTIONS"
            | "PYTHONHOME"
            | "PYTHONPATH"
            | "RUBYOPT"
            | "PERL5OPT"
            | "BASH_ENV"
            | "ENV"
            | "SHELLOPTS"
            | "PS4"
            | "PROMPT_COMMAND"
    ) {
        return Err(McpError::InvalidConfig(format!(
            "MCP environment variable `{name}` is managed or process-influencing and may not be overridden"
        )));
    }
    Ok(())
}

pub fn is_managed_mcp_header_name(name: &str) -> bool {
    matches!(
        name.trim().to_ascii_lowercase().as_str(),
        "accept"
            | "connection"
            | "content-length"
            | "content-type"
            | "host"
            | "mcp-protocol-version"
            | "mcp-session-id"
            | "origin"
            | "transfer-encoding"
    )
}

pub fn validate_mcp_header_name(name: &str) -> McpResult<()> {
    let normalized = name.trim().to_ascii_lowercase();
    if normalized.is_empty() || name.len() > MAX_MCP_HEADER_NAME_BYTES {
        return Err(McpError::InvalidConfig(format!(
            "remote MCP header name `{name}` is empty or exceeds {MAX_MCP_HEADER_NAME_BYTES} bytes"
        )));
    }
    if is_managed_mcp_header_name(&normalized) {
        return Err(McpError::InvalidConfig(format!(
            "remote MCP header `{name}` is managed by Loom"
        )));
    }
    HeaderName::from_bytes(normalized.as_bytes()).map_err(|error| {
        McpError::InvalidConfig(format!("invalid remote MCP header `{name}`: {error}"))
    })?;
    Ok(())
}

pub fn validate_mcp_headers(headers: &BTreeMap<String, String>) -> McpResult<()> {
    if headers.len() > MAX_MCP_HEADERS {
        return Err(McpError::InvalidConfig(format!(
            "headers contains {} entries; limit is {MAX_MCP_HEADERS}",
            headers.len()
        )));
    }
    let mut total_bytes = 0usize;
    for (name, value) in headers {
        validate_mcp_header_name(name)?;
        if value.len() > MAX_MCP_HEADER_VALUE_BYTES {
            return Err(McpError::InvalidConfig(format!(
                "header `{name}` value is {} bytes; limit is {MAX_MCP_HEADER_VALUE_BYTES}",
                value.len()
            )));
        }
        HeaderValue::from_str(value).map_err(|error| {
            McpError::InvalidConfig(format!(
                "invalid value for remote MCP header `{name}`: {error}"
            ))
        })?;
        total_bytes = total_bytes.saturating_add(name.len() + value.len() + 4);
    }
    if total_bytes > MAX_MCP_HEADER_TOTAL_BYTES {
        return Err(McpError::InvalidConfig(format!(
            "headers are {total_bytes} aggregate bytes; limit is {MAX_MCP_HEADER_TOTAL_BYTES}"
        )));
    }
    Ok(())
}

pub(super) fn validate_credential_target_names(config: &McpServerConfig) -> McpResult<()> {
    if config.credential_env.len() > MAX_MCP_ENVIRONMENT_ENTRIES {
        return Err(McpError::InvalidConfig(format!(
            "credentialEnv contains {} entries; limit is {MAX_MCP_ENVIRONMENT_ENTRIES}",
            config.credential_env.len()
        )));
    }
    for (name, credential_id) in &config.credential_env {
        validate_mcp_environment_name(name)?;
        validate_config_identity("credentialEnv credential id", credential_id)?;
    }
    if config.credential_headers.len() > MAX_MCP_HEADERS {
        return Err(McpError::InvalidConfig(format!(
            "credentialHeaders contains {} entries; limit is {MAX_MCP_HEADERS}",
            config.credential_headers.len()
        )));
    }
    for (name, credential_id) in &config.credential_headers {
        validate_mcp_header_name(name)?;
        validate_config_identity("credentialHeaders credential id", credential_id)?;
    }
    Ok(())
}

pub(super) fn validate_installed_package_state(
    config: &McpServerConfig,
    package: &McpServerPackageState,
) -> McpResult<()> {
    validate_config_identity("package.publisherId", &package.publisher_id)?;
    let expected_qualified = format!("{}/{}", package.publisher_id, config.id);
    if package.qualified_id != expected_qualified {
        return Err(McpError::InvalidConfig(format!(
            "package.qualifiedId `{}` must equal `{expected_qualified}`",
            package.qualified_id
        )));
    }
    semver::Version::parse(&package.version).map_err(|error| {
        McpError::InvalidConfig(format!("package.version must be SemVer: {error}"))
    })?;
    if package.digest.len() != 64 || !package.digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(McpError::InvalidConfig(
            "package.digest must be a 64-character SHA-256 hex digest".to_owned(),
        ));
    }
    if !package.package_dir.is_absolute() {
        return Err(McpError::InvalidConfig(
            "package.packageDir must be absolute".to_owned(),
        ));
    }
    if package.files.len() > 4096 {
        return Err(McpError::InvalidConfig(format!(
            "package.files contains {} entries; limit is 4096",
            package.files.len()
        )));
    }
    for (path, digest) in &package.files {
        if path.is_empty()
            || Path::new(path).is_absolute()
            || Path::new(path)
                .components()
                .any(|component| !matches!(component, std::path::Component::Normal(_)))
        {
            return Err(McpError::InvalidConfig(format!(
                "package.files path `{path}` is unsafe"
            )));
        }
        if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(McpError::InvalidConfig(format!(
                "package.files digest for `{path}` must be SHA-256 hex"
            )));
        }
    }
    Ok(())
}

/// Reject a stdio command whose meaning depends on where the daemon happens to have been started.
///
/// A path with a parent but no root — `runtime/server.exe`, `./server`, `..\server.exe` — is completed
/// by the process's current directory, so which binary runs is decided by the daemon's working
/// directory rather than by the configuration. A packaged server never needs one, since the installer
/// records an absolute path under the package directory, and an operator writing the row by hand can
/// say what they mean.
///
/// A bare program name is still allowed: that is a `PATH` lookup, which is how servers are normally
/// launched (`npx`, `node`, `python`), and taking it away would rule out most of the ecosystem.
pub(super) fn validate_stdio_command(command: &str) -> McpResult<()> {
    let path = Path::new(command.trim());
    let is_bare_name = !path
        .parent()
        .is_some_and(|parent| !parent.as_os_str().is_empty());
    if path.is_absolute() || is_bare_name {
        return Ok(());
    }
    Err(McpError::InvalidConfig(format!(
        "stdio command `{command}` is a relative path, so which file runs depends on the daemon's \
         working directory; use an absolute path, or a bare program name to look up on PATH"
    )))
}
