use super::*;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct McpArtDependency {
    server_id: String,
    package_id: String,
    version: String,
}

pub(super) fn framework_packages_root() -> Option<PathBuf> {
    std::env::var("LOOM_FRAMEWORK_PACKAGES_DIR")
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("LOOM_CONTROL_PLANE_ROOT")
                .ok()
                .map(|value| PathBuf::from(value).join("frameworks"))
        })
}

pub(super) fn art_directory(tool: &ToolDefinition) -> Option<PathBuf> {
    let metadata = tool.metadata.as_ref()?.as_object()?;
    let package = metadata
        .get("artPackage")
        .and_then(Value::as_object)
        .and_then(|value| value.get("dir"))
        .and_then(Value::as_str);
    package.map(PathBuf::from)
}

pub(super) fn art_package_path(tool: &ToolDefinition, key: &str) -> Option<PathBuf> {
    art_package_string(tool, key).map(PathBuf::from)
}

pub(super) fn art_package_string(tool: &ToolDefinition, key: &str) -> Option<String> {
    tool.metadata
        .as_ref()
        .and_then(|metadata| metadata.get("artPackage"))
        .and_then(|package| package.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub(super) fn art_credential_bindings(tool: &ToolDefinition) -> BTreeMap<String, String> {
    let settings = tool
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("artUserSettings"))
        .and_then(|settings| settings.get("credentialBindings"));
    let authoring = tool
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("authoring"))
        .and_then(|authoring| authoring.get("credentialBindings"));
    settings
        .or(authoring)
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

pub(super) fn resolve_mcp_server(
    tool: &ToolDefinition,
    control_plane_root: &Path,
    credential_store: Option<&crate::credentials::CredentialStore>,
) -> ToolRegistryResult<(FrameworkMcpServer, Vec<loom_protocol::CredentialGrant>)> {
    let dependency: McpArtDependency = tool
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("mcp"))
        .cloned()
        .ok_or_else(|| {
            mcp_dependency_error(
                tool,
                "<missing>",
                "mcp_dependency_invalid",
                "MCP Art metadata.mcp is required",
            )
        })
        .and_then(|value| {
            serde_json::from_value(value).map_err(|error| {
                mcp_dependency_error(
                    tool,
                    "<invalid>",
                    "mcp_dependency_invalid",
                    format!("invalid MCP Art dependency: {error}"),
                )
            })
        })?;
    if dependency.server_id.trim().is_empty()
        || dependency.package_id.trim().is_empty()
        || dependency.version.trim().is_empty()
    {
        return Err(mcp_dependency_error(
            tool,
            dependency.server_id.as_str(),
            "mcp_dependency_invalid",
            "metadata.mcp.serverId, packageId, and version are required",
        ));
    }
    let store_path = control_plane_root.join("mcp").join("servers.json");
    let servers = fs::read(&store_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Vec<loom_mcp::McpServerConfig>>(&bytes).ok())
        .unwrap_or_default();
    let server = servers
        .into_iter()
        .find(|server| server.id == dependency.server_id)
        .ok_or_else(|| {
            mcp_dependency_error(
                tool,
                &dependency.server_id,
                "mcp_dependency_missing",
                format!(
                    "independent MCP server `{}` is not installed",
                    dependency.server_id
                ),
            )
        })?;
    if !server.enabled {
        return Err(mcp_dependency_error(
            tool,
            &dependency.server_id,
            "mcp_dependency_disabled",
            format!("MCP server `{}` is disabled", dependency.server_id),
        ));
    }
    let package = server.package.as_ref().ok_or_else(|| {
        mcp_dependency_error(
            tool,
            &dependency.server_id,
            "mcp_dependency_package_mismatch",
            "the selected MCP server is not installed from a package",
        )
    })?;
    if package.qualified_id != dependency.package_id {
        return Err(mcp_dependency_error(
            tool,
            &dependency.server_id,
            "mcp_dependency_package_mismatch",
            format!(
                "Art requires MCP package `{}`, but server `{}` is package `{}`",
                dependency.package_id, dependency.server_id, package.qualified_id
            ),
        ));
    }
    let version_requirement = semver::VersionReq::parse(&dependency.version).map_err(|error| {
        mcp_dependency_error(
            tool,
            &dependency.server_id,
            "mcp_dependency_invalid",
            format!(
                "invalid MCP version requirement `{}`: {error}",
                dependency.version
            ),
        )
    })?;
    let package_version = semver::Version::parse(&package.version).map_err(|error| {
        mcp_dependency_error(
            tool,
            &dependency.server_id,
            "mcp_dependency_version_mismatch",
            format!("installed MCP package version is invalid: {error}"),
        )
    })?;
    if !version_requirement.matches(&package_version) {
        return Err(mcp_dependency_error(
            tool,
            &dependency.server_id,
            "mcp_dependency_version_mismatch",
            format!(
                "Art requires MCP package version `{}`, but `{}` is installed",
                dependency.version, package.version
            ),
        ));
    }
    server.validate().map_err(|error| {
        mcp_dependency_error(
            tool,
            &dependency.server_id,
            "mcp_dependency_invalid",
            error.to_string(),
        )
    })?;
    for requirement in server
        .credential_requirements
        .iter()
        .filter(|requirement| requirement.required)
    {
        if !server.credential_bindings.contains_key(&requirement.id) {
            return Err(mcp_dependency_error(
                tool,
                &dependency.server_id,
                "mcp_credential_missing",
                format!("MCP credential `{}` is not configured", requirement.label),
            ));
        }
    }
    let credentials = credential_store
        .map(|store| store.grants_for_mcp_bindings(&server.id, &server.credential_bindings))
        .transpose()
        .map_err(|error| {
            mcp_dependency_error(
                tool,
                &dependency.server_id,
                "mcp_credential_missing",
                format!("MCP credential resolution failed: {error}"),
            )
        })?
        .unwrap_or_default();
    let optional_credential_ids = server
        .credential_requirements
        .iter()
        .filter(|requirement| !requirement.required)
        .map(|requirement| requirement.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let (credential_env, optional_credential_env) =
        server
            .credential_env
            .into_iter()
            .partition(|(_, credential_name)| {
                !optional_credential_ids.contains(credential_name.as_str())
            });
    let (credential_headers, optional_credential_headers) = server
        .credential_headers
        .into_iter()
        .partition(|(_, credential_name)| {
            !optional_credential_ids.contains(credential_name.as_str())
        });
    let resolved = FrameworkMcpServer {
        id: server.id,
        package_id: package.qualified_id.clone(),
        version: package.version.clone(),
        transport: server.transport.label().to_owned(),
        command: server.command,
        args: server.args,
        env: server.env,
        url: server.url,
        headers: server.headers,
        credential_env,
        credential_headers,
        optional_credential_env,
        optional_credential_headers,
    };
    Ok((resolved, credentials))
}

fn mcp_dependency_error(
    tool: &ToolDefinition,
    server_id: &str,
    code: &str,
    reason: impl Into<String>,
) -> ToolRegistryError {
    ToolRegistryError::McpDependency {
        tool_id: tool.qualified_id(),
        server_id: server_id.to_owned(),
        code: code.to_owned(),
        reason: reason.into(),
    }
}
