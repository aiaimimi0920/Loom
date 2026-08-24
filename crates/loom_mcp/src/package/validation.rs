//! Manifest field, credential, entry-point, and transport validation.

use super::*;

pub(super) fn validate_manifest(
    manifest: &McpServerPackageManifest,
    staging_root: &Path,
) -> Result<(), McpPackageError> {
    if manifest.schema_version != 1 {
        return Err(McpPackageError::InvalidManifest(
            "schemaVersion must be 1".to_owned(),
        ));
    }
    for (label, value) in [
        ("id", manifest.id.as_str()),
        ("publisher.id", manifest.publisher.id.as_str()),
    ] {
        if !is_safe_identity(value) {
            return Err(McpPackageError::InvalidManifest(format!(
                "{label} is not a safe package identity"
            )));
        }
    }
    Version::parse(&manifest.version).map_err(|error| {
        McpPackageError::InvalidManifest(format!("version must be SemVer: {error}"))
    })?;
    if manifest.name.trim().is_empty() {
        return Err(McpPackageError::InvalidManifest(
            "name is required".to_owned(),
        ));
    }
    validate_manifest_text("name", &manifest.name, MAX_MCP_SERVER_NAME_BYTES, true)?;
    validate_manifest_text(
        "description",
        &manifest.description,
        MAX_MCP_SERVER_DESCRIPTION_BYTES,
        false,
    )?;
    validate_manifest_text(
        "publisher.name",
        &manifest.publisher.name,
        MAX_MCP_SERVER_NAME_BYTES,
        true,
    )?;
    if manifest.tools.len() > MAX_MCP_TOOLS {
        return Err(McpPackageError::InvalidManifest(format!(
            "tools contains {} entries; limit is {MAX_MCP_TOOLS}",
            manifest.tools.len()
        )));
    }
    for (index, tool) in manifest.tools.iter().enumerate() {
        validate_mcp_tool_identifier(&format!("tools[{index}]"), tool)
            .map_err(|error| McpPackageError::InvalidManifest(error.to_string()))?;
    }
    if manifest.entry.args.len() > MAX_MCP_ARGUMENTS {
        return Err(McpPackageError::InvalidManifest(format!(
            "entry.args contains {} entries; limit is {MAX_MCP_ARGUMENTS}",
            manifest.entry.args.len()
        )));
    }
    for (index, argument) in manifest.entry.args.iter().enumerate() {
        validate_manifest_text(
            &format!("entry.args[{index}]"),
            argument,
            MAX_MCP_ARGUMENT_BYTES,
            false,
        )?;
    }
    if manifest.credentials.len() > MAX_MCP_CREDENTIALS {
        return Err(McpPackageError::InvalidManifest(format!(
            "credentials contains {} entries; limit is {MAX_MCP_CREDENTIALS}",
            manifest.credentials.len()
        )));
    }
    let mut credential_ids = BTreeMap::new();
    for (index, credential) in manifest.credentials.iter().enumerate() {
        if !is_safe_identity(&credential.id) || credential.target.name.trim().is_empty() {
            return Err(McpPackageError::InvalidManifest(
                "credential id and target name are required".to_owned(),
            ));
        }
        if credential_ids.insert(&credential.id, ()).is_some() {
            return Err(McpPackageError::InvalidManifest(format!(
                "duplicate credential id `{}`",
                credential.id
            )));
        }
        validate_manifest_text(
            &format!("credentials[{index}].label"),
            &credential.label,
            MAX_MCP_CREDENTIAL_LABEL_BYTES,
            true,
        )?;
        match credential.target.kind {
            McpPackageCredentialTargetKind::Env => {
                validate_mcp_environment_name(&credential.target.name)
            }
            McpPackageCredentialTargetKind::Header => {
                validate_mcp_header_name(&credential.target.name)
            }
        }
        .map_err(|error| McpPackageError::InvalidManifest(error.to_string()))?;
    }
    match manifest.transport {
        McpTransport::Stdio => {
            let command = safe_relative_path(&manifest.entry.command)?;
            if names_a_batch_file(&command) {
                return Err(McpPackageError::InvalidManifest(format!(
                    "entry.command `{}` is a batch file, which Windows runs through `cmd.exe`; \
                     name an executable or a `.ps1` script instead",
                    command.display()
                )));
            }
            if !staging_root.join(&command).is_file() {
                return Err(McpPackageError::MissingEntry(command.display().to_string()));
            }
        }
        McpTransport::StreamableHttp if manifest.entry.url.trim().is_empty() => {
            return Err(McpPackageError::InvalidManifest(
                "streamable-http entry.url is required".to_owned(),
            ));
        }
        McpTransport::StreamableHttp => {}
    }
    Ok(())
}

pub(super) fn validate_manifest_text(
    field: &str,
    value: &str,
    max_bytes: usize,
    required: bool,
) -> Result<(), McpPackageError> {
    if required && value.trim().is_empty() {
        return Err(McpPackageError::InvalidManifest(format!(
            "{field} is required"
        )));
    }
    if value.len() > max_bytes {
        return Err(McpPackageError::InvalidManifest(format!(
            "{field} is {} bytes; limit is {max_bytes}",
            value.len()
        )));
    }
    Ok(())
}
