// Art package installation, details, update checks, upgrade, and uninstall.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstallArtRequest {
    /// The art package zip, base64-encoded (data URL or raw base64).
    zip_base64: String,
    #[serde(default)]
    bundled_catalog: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateAuthoredArtRequest {
    tool: ToolDefinition,
    #[serde(default)]
    runtime: Option<ArtRuntimeManifest>,
    #[serde(default)]
    files: Vec<CreateAuthoredArtFile>,
    #[serde(default)]
    source_directory: Option<String>,
    #[serde(default)]
    source_directory_target: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateAuthoredArtFile {
    path: String,
    content: String,
}

fn collect_authored_art_files(
    request: &CreateAuthoredArtRequest,
) -> std::result::Result<Vec<(String, Vec<u8>)>, String> {
    const MAX_AUTHORED_FILES: usize = 4096;
    const MAX_AUTHORED_BYTES: u64 = 32 * 1024 * 1024;

    let mut files = request
        .files
        .iter()
        .map(|file| (file.path.clone(), file.content.as_bytes().to_vec()))
        .collect::<Vec<_>>();
    let mut total_bytes = files
        .iter()
        .map(|(_, content)| content.len() as u64)
        .sum::<u64>();
    let Some(source_directory) = request
        .source_directory
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        if files.len() > MAX_AUTHORED_FILES || total_bytes > MAX_AUTHORED_BYTES {
            return Err("authored Art resources exceed the package limits".to_owned());
        }
        return Ok(files);
    };

    let source_root = fs::canonicalize(source_directory)
        .map_err(|error| format!("cannot resolve authored Art source directory: {error}"))?;
    if !source_root.is_dir() {
        return Err("authored Art sourceDirectory must be a directory".to_owned());
    }
    let target = request
        .source_directory_target
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("runtime/plugin")
        .replace('\\', "/");
    if target.starts_with('/')
        || target.contains(':')
        || target
            .split('/')
            .any(|segment| segment.is_empty() || segment == "..")
    {
        return Err("authored Art sourceDirectoryTarget must be a safe relative path".to_owned());
    }

    let mut pending = vec![source_root.clone()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory)
            .map_err(|error| format!("cannot read authored Art source directory: {error}"))?
        {
            let entry =
                entry.map_err(|error| format!("cannot read authored Art source: {error}"))?;
            let file_type = entry
                .file_type()
                .map_err(|error| format!("cannot inspect authored Art source: {error}"))?;
            if file_type.is_symlink() {
                return Err(format!(
                    "authored Art source cannot contain symbolic links: {}",
                    entry.path().display()
                ));
            }
            if file_type.is_dir() {
                pending.push(entry.path());
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let relative = entry
                .path()
                .strip_prefix(&source_root)
                .map_err(|error| format!("cannot relativize authored Art source: {error}"))?
                .to_string_lossy()
                .replace('\\', "/");
            let content = fs::read(entry.path())
                .map_err(|error| format!("cannot read authored Art source file: {error}"))?;
            total_bytes = total_bytes.saturating_add(content.len() as u64);
            files.push((format!("{target}/{relative}"), content));
            if files.len() > MAX_AUTHORED_FILES || total_bytes > MAX_AUTHORED_BYTES {
                return Err("authored Art resources exceed the package limits".to_owned());
            }
        }
    }
    Ok(files)
}

fn create_authored_art(
    body: &str,
    tool_registry: &ToolRegistry,
    framework_registry: &FrameworkRegistry,
    control_plane_root: &Path,
    hook_bridge: &SharedHookBridgeRuntime,
) -> Result<(u16, String)> {
    let mut request: CreateAuthoredArtRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let (identity, _) = match ensure_local_publisher_identity(control_plane_root) {
        Ok(identity) => identity,
        Err(error) => {
            return structured_error(
                500,
                json!({ "code": "publisher_identity_failed", "message": error }),
            )
        }
    };
    let metadata = request
        .tool
        .metadata
        .get_or_insert_with(|| Value::Object(serde_json::Map::new()));
    if !metadata.is_object() {
        *metadata = Value::Object(serde_json::Map::new());
    }
    let package_security = metadata
        .as_object_mut()
        .expect("authored Art metadata was normalized")
        .entry("packageSecurity".to_owned())
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    if !package_security.is_object() {
        *package_security = Value::Object(serde_json::Map::new());
    }
    package_security
        .as_object_mut()
        .expect("authored Art package security was normalized")
        .insert(
            "publisher".to_owned(),
            json!({ "id": identity.user_id, "name": "Local Loom user" }),
        );
    if matches!(&request.tool.execution, ToolExecution::FrameworkArt { .. })
        && request.runtime.is_none()
    {
        return invalid_request("framework_art authoring requires an art runtime manifest");
    }
    let files = match collect_authored_art_files(&request) {
        Ok(files) => files,
        Err(error) => return invalid_request(error),
    };
    let qualified_id = request.tool.qualified_id();
    let zip = match loom_tool_registry::install::build_authored_art_package_zip(
        &request.tool,
        request.runtime.as_ref(),
        &files,
    ) {
        Ok(zip) => zip,
        Err(error) => return invalid_request(error.to_string()),
    };
    match loom_tool_registry::install::install_authored_art_from_zip(
        &zip,
        control_plane_root,
        framework_registry,
        tool_registry,
    ) {
        Ok(report) => {
            broadcast_tool_capabilities_updated(hook_bridge);
            let tool = tool_registry
                .get_tool(&qualified_id)
                .ok()
                .flatten()
                .unwrap_or(request.tool);
            Ok((
                200,
                serde_json::to_string(&json!({ "report": report, "tool": tool }))?,
            ))
        }
        Err(loom_tool_registry::install::ArtInstallError::FrameworkNotReady {
            art_id,
            framework,
            reason,
        }) => structured_error(
            409,
            json!({
                "code": "framework_not_ready",
                "message": format!("art `{art_id}` requires framework `{framework}` ({reason})"),
                "framework": framework,
            }),
        ),
        Err(error) => structured_error(
            400,
            json!({ "code": "art_create_failed", "message": error.to_string() }),
        ),
    }
}

// Install an art package (zip) into the registry: extracts to <root>/arts/<id>/,
// checks the framework is ready, and registers the ToolDefinition.
fn sync_installed_workflow_definition(
    tool: &ToolDefinition,
    workflow_store: &WorkflowStore,
) -> std::result::Result<(), String> {
    let ToolExecution::Workflow { workflow_id, .. } = &tool.execution else {
        return Ok(());
    };
    let art_dir = tool
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.pointer("/artPackage/dir"))
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| {
            format!(
                "workflow Art `{}` has no installed package directory",
                tool.id
            )
        })?;
    let canonical_art_dir = fs::canonicalize(&art_dir)
        .map_err(|error| format!("cannot resolve workflow Art package: {error}"))?;
    let workflow_path = fs::canonicalize(art_dir.join("workflow.yaml"))
        .map_err(|error| format!("workflow Art `{}` has no workflow.yaml: {error}", tool.id))?;
    if !workflow_path.starts_with(&canonical_art_dir) || !workflow_path.is_file() {
        return Err(format!(
            "workflow Art `{}` resolves workflow.yaml outside its package",
            tool.id
        ));
    }
    let yaml = fs::read_to_string(&workflow_path)
        .map_err(|error| format!("cannot read workflow Art definition: {error}"))?;
    workflow_store
        .save_workflow(workflow_id, &yaml)
        .map_err(|error| format!("cannot register workflow Art definition: {error}"))?;
    Ok(())
}

fn sync_registered_workflow_definition(
    art_id: &str,
    tool_registry: &ToolRegistry,
    workflow_store: &WorkflowStore,
) -> std::result::Result<(), String> {
    let tool = tool_registry
        .get_tool(art_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("installed Art `{art_id}` was not registered"))?;
    sync_installed_workflow_definition(&tool, workflow_store)
}

fn install_art(
    body: &str,
    tool_registry: &ToolRegistry,
    framework_registry: &FrameworkRegistry,
    workflow_store: &WorkflowStore,
    control_plane_root: &Path,
    hook_bridge: &SharedHookBridgeRuntime,
    bundled_art_sha256_allowlist: &BTreeSet<String>,
) -> Result<(u16, String)> {
    let request = match serde_json::from_str::<InstallArtRequest>(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let zip_bytes = match loom_image_io::decode_data_url_bytes(&request.zip_base64) {
        Ok(bytes) => bytes,
        Err(error) => return invalid_request(format!("decode art package: {error}")),
    };
    let result = if request.bundled_catalog {
        let package_sha256 = sha256_bytes(&zip_bytes);
        if !bundled_art_sha256_allowlist.contains(&package_sha256) {
            return structured_error(
                403,
                json!({
                    "code": "bundled_art_not_allowlisted",
                    "message": "bundled Art package digest is not authorized by the daemon launch catalog",
                }),
            );
        }
        loom_tool_registry::install::install_bundled_art_from_zip(
            &zip_bytes,
            control_plane_root,
            framework_registry,
            tool_registry,
        )
    } else {
        loom_tool_registry::install::install_art_from_zip(
            &zip_bytes,
            control_plane_root,
            framework_registry,
            tool_registry,
        )
    };
    match result {
        Ok(report) => {
            if let Err(message) =
                sync_registered_workflow_definition(&report.tool_id, tool_registry, workflow_store)
            {
                return structured_error(
                    400,
                    json!({ "code": "workflow_art_install_failed", "message": message }),
                );
            }
            broadcast_tool_capabilities_updated(hook_bridge);
            Ok((200, serde_json::to_string(&json!({ "report": report }))?))
        }
        Err(loom_tool_registry::install::ArtInstallError::FrameworkNotReady {
            art_id,
            framework,
            reason,
        }) => structured_error(
            409,
            json!({
                "code": "framework_not_ready",
                "message": format!("art `{art_id}` 需要框架 `{framework}`（未{reason}），请先安装该框架"),
                "framework": framework,
            }),
        ),
        Err(loom_tool_registry::install::ArtInstallError::InvalidArtId(id)) => {
            invalid_request(format!("invalid art id `{id}`"))
        }
        Err(error) => structured_error(
            400,
            json!({ "code": "art_install_failed", "message": error.to_string() }),
        ),
    }
}

fn rollback_art(
    art_id: &str,
    tool_registry: &ToolRegistry,
    framework_registry: &FrameworkRegistry,
    workflow_store: &WorkflowStore,
    control_plane_root: &Path,
    hook_bridge: &SharedHookBridgeRuntime,
) -> Result<(u16, String)> {
    match loom_tool_registry::install::rollback_art_package(
        control_plane_root,
        art_id,
        tool_registry,
        framework_registry,
    ) {
        Ok(tool) => {
            if let Err(message) = sync_installed_workflow_definition(&tool, workflow_store) {
                return structured_error(
                    409,
                    json!({ "code": "workflow_art_rollback_failed", "message": message }),
                );
            }
            broadcast_tool_capabilities_updated(hook_bridge);
            Ok((200, serde_json::to_string(&json!({ "tool": tool }))?))
        }
        Err(error) => structured_error(
            409,
            json!({ "code": "art_rollback_failed", "message": error.to_string() }),
        ),
    }
}

fn uninstall_art(
    art_id: &str,
    body: &str,
    tool_registry: &ToolRegistry,
    control_plane_root: &Path,
    hook_bridge: &SharedHookBridgeRuntime,
    mcp_servers: &SharedMcpServerStore,
    mcp_store_path: &Path,
) -> Result<(u16, String)> {
    let request: ArtUninstallRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let target = match tool_registry.get_tool(art_id) {
        Ok(Some(tool)) => tool,
        Ok(None) => {
            return structured_error(404, json!({ "code": "art_not_found", "artId": art_id }))
        }
        Err(error) => return tool_registry_error_response(error),
    };
    let target_identity = target.qualified_id();
    let mcp_dependencies = read_dependencies(&target).mcp_servers;
    let other_tools = tool_registry
        .list_tools()
        .map_err(|error| anyhow::anyhow!("list Arts before uninstall: {error}"))?;
    let mut retained_mcp_servers = Vec::new();
    let removable_mcp_packages = if request.remove_unused_mcp_servers {
        mcp_dependencies
            .iter()
            .filter_map(|dependency| {
                let used_by = other_tools
                    .iter()
                    .filter(|tool| tool.qualified_id() != target_identity)
                    .filter(|tool| {
                        read_dependencies(tool)
                            .mcp_servers
                            .iter()
                            .any(|candidate| candidate.id == dependency.id)
                    })
                    .map(ToolDefinition::qualified_id)
                    .collect::<Vec<_>>();
                if used_by.is_empty() {
                    Some(dependency.id.clone())
                } else {
                    retained_mcp_servers.push(json!({
                        "packageId": dependency.id,
                        "usedByArtIds": used_by,
                    }));
                    None
                }
            })
            .collect::<BTreeSet<_>>()
    } else {
        BTreeSet::new()
    };
    match loom_tool_registry::install::uninstall_art_package(
        control_plane_root,
        art_id,
        tool_registry,
    ) {
        Ok(()) => {
            let _ = ArtSettingsStore::new(control_plane_root).delete(art_id);
            let mut removed_mcp_servers = Vec::new();
            if request.remove_unused_mcp_servers {
                let candidates = mcp_servers
                    .lock()
                    .map_err(|_| anyhow::anyhow!("lock MCP server store"))?
                    .values()
                    .filter(|server| {
                        server.package.as_ref().is_some_and(|package| {
                            removable_mcp_packages.contains(&package.qualified_id)
                        })
                    })
                    .map(|server| server.id.clone())
                    .collect::<Vec<_>>();
                for server_id in candidates {
                    let (status, _) = delete_mcp_server(
                        &server_id,
                        mcp_servers,
                        mcp_store_path,
                        control_plane_root,
                    )?;
                    if status == 200 {
                        removed_mcp_servers.push(server_id);
                    }
                }
            }
            broadcast_tool_capabilities_updated(hook_bridge);
            Ok((
                200,
                serde_json::to_string(&json!({
                    "artId": art_id,
                    "uninstalled": true,
                    "removedMcpServers": removed_mcp_servers,
                    "retainedMcpServers": retained_mcp_servers,
                }))?,
            ))
        }
        Err(error) => structured_error(
            400,
            json!({ "code": "art_uninstall_failed", "message": error.to_string() }),
        ),
    }
}
