// Settings documents and MCP server list, create, delete, and enable operations.
fn settings_index(registry: &ConfigRegistry, store: &FileDocumentStore) -> Result<(u16, String)> {
    let managed = managed_app_set();
    let mut documents = Vec::new();
    for app in managed.managed_apps() {
        let (document, _) = match store.read_or_create(app, registry) {
            Ok(document) => document,
            Err(error) => return managed_config_error_response(error),
        };
        documents.push(document);
    }
    Ok((200, render_settings_index(registry, &managed, &documents)))
}

fn settings_app(
    app: ManagedAppId,
    registry: &ConfigRegistry,
    store: &FileDocumentStore,
) -> Result<(u16, String)> {
    if !managed_app_set().contains(app) {
        return managed_config_error_response(ManagedConfigError::new(
            ManagedConfigErrorCode::AppNotManaged,
            format!("{app} is not managed by Loom"),
        ));
    }
    let (document, _) = match store.read_or_create(app, registry) {
        Ok(document) => document,
        Err(error) => return managed_config_error_response(error),
    };
    Ok((200, render_app_settings_page(registry, app, &document)))
}

fn get_managed_config(
    app: ManagedAppId,
    registry: &ConfigRegistry,
    store: &FileDocumentStore,
) -> Result<(u16, String)> {
    if !managed_app_set().contains(app) {
        return managed_config_error_response(ManagedConfigError::new(
            ManagedConfigErrorCode::AppNotManaged,
            format!("{app} is not managed by Loom"),
        ));
    }
    let adapter = registry.get(app).expect("registered adapter");
    let (document, created) = match store.read_or_create(app, registry) {
        Ok(document) => document,
        Err(error) => return managed_config_error_response(error),
    };
    let config = document.config.clone();
    let ui_sections = adapter.ui_sections(&config);
    Ok((
        200,
        serde_json::to_string(&json!({
            "app": app,
            "owner": "loom",
            "source": "loom-managed",
            "writable": true,
            "created": created,
            "document": document.metadata(),
            "config": config,
            "ui": {
                "title": adapter.display_name(),
                "sections": ui_sections,
            }
        }))?,
    ))
}

fn put_managed_config(
    app: ManagedAppId,
    body: &str,
    registry: &ConfigRegistry,
    store: &FileDocumentStore,
) -> Result<(u16, String)> {
    if !managed_app_set().contains(app) {
        return managed_config_error_response(ManagedConfigError::new(
            ManagedConfigErrorCode::AppNotManaged,
            format!("{app} is not managed by Loom"),
        ));
    }
    let request = match serde_json::from_str::<PutManagedConfigRequest>(body) {
        Ok(request) => request,
        Err(error) => {
            return structured_error(
                400,
                json!({
                    "code": "invalid_request",
                    "message": error.to_string(),
                }),
            );
        }
    };
    let document =
        match store.write_validated(app, request.expected_revision, request.config, registry) {
            Ok(document) => document,
            Err(error) => return managed_config_error_response(error),
        };
    let config = document.config.clone();
    Ok((
        200,
        serde_json::to_string(&json!({
            "ok": true,
            "app": app,
            "owner": "loom",
            "source": "loom-managed",
            "writable": true,
            "created": false,
            "document": document.metadata(),
            "config": config,
            "validation": { "errors": [] },
        }))?,
    ))
}

fn managed_config_error_response(error: ManagedConfigError) -> Result<(u16, String)> {
    let status = match error.code() {
        ManagedConfigErrorCode::UnknownApp => 404,
        ManagedConfigErrorCode::AppNotManaged => 409,
        ManagedConfigErrorCode::InvalidConfiguration => 400,
        ManagedConfigErrorCode::RevisionConflict => 409,
        ManagedConfigErrorCode::StorageError => 500,
    };
    structured_error(
        status,
        json!({
            "code": managed_config_error_code(error.code()),
            "message": error.message(),
            "validation": { "errors": error.validation_errors() },
        }),
    )
}

fn managed_config_error_code(code: ManagedConfigErrorCode) -> &'static str {
    match code {
        ManagedConfigErrorCode::UnknownApp => "unknown_app",
        ManagedConfigErrorCode::AppNotManaged => "app_not_managed",
        ManagedConfigErrorCode::InvalidConfiguration => "invalid_configuration",
        ManagedConfigErrorCode::RevisionConflict => "revision_conflict",
        ManagedConfigErrorCode::StorageError => "storage_error",
    }
}

fn configuration_claim_app(path: &str) -> Option<&str> {
    let query = path.strip_prefix("/v1/configuration/claims?")?;
    query.split('&').find_map(|part| {
        let (name, value) = part.split_once('=')?;
        (name == "app" && !value.is_empty()).then_some(value)
    })
}

fn capabilities() -> Result<(u16, String)> {
    Ok((
        200,
        serde_json::to_string(&json!({
            "capabilities": [
                {
                    "id": CAPABILITY_BRAIN_PLAN,
                    "mode": "run",
                    "description": "Create a concise Loom-side execution plan from a goal and optional constraints.",
                    "input_schema": {
                        "type": "object",
                        "properties": {
                            "goal": { "type": "string" },
                            "constraints": {
                                "type": "array",
                                "items": { "type": "string" }
                            }
                        },
                        "required": ["goal"]
                    }
                },
                {
                    "id": CAPABILITY_TEA_TICKET_DECOMPOSE,
                    "mode": "run",
                    "description": "Use Loom reasoning to generate a Tea work-order decomposition proposal without mutating Tea ticket state.",
                    "input_schema": {
                        "type": "object",
                        "properties": {
                            "schema_version": { "type": "integer" },
                            "request_id": { "type": "string" },
                            "ticket": { "type": "object" },
                            "comments": { "type": "array" },
                            "policy": { "type": "object" },
                            "context": { "type": "object" }
                        },
                        "required": ["schema_version", "request_id", "ticket", "policy", "context"]
                    }
                },
                {
                    "id": CAPABILITY_TEA_TICKET_EXECUTE,
                    "mode": "run",
                    "description": "Execute an approved Tea plan through Loom runtime and return run evidence.",
                    "input_schema": {
                        "type": "object",
                        "properties": {
                            "schema_version": { "type": "integer" },
                            "request_id": { "type": "string" },
                            "ticket": { "type": "object" },
                            "approved_plan_id": { "type": "string" },
                            "plan": { "type": "object" },
                            "policy": { "type": "object" }
                        },
                        "required": ["schema_version", "request_id", "ticket", "approved_plan_id", "plan", "policy"]
                    }
                },
                {
                    "id": CAPABILITY_TEA_TICKET_REVIEW,
                    "mode": "run",
                    "description": "Review Tea execution evidence and return a review suggestion without changing Tea state.",
                    "input_schema": {
                        "type": "object",
                        "properties": {
                            "schema_version": { "type": "integer" },
                            "request_id": { "type": "string" },
                            "ticket": { "type": "object" },
                            "evidence": { "type": "object" }
                        },
                        "required": ["schema_version", "request_id", "ticket", "evidence"]
                    }
                }
            ]
        }))?,
    ))
}

fn list_mcp_servers(
    mcp_servers: &SharedMcpServerStore,
    tool_registry: &ToolRegistry,
    control_plane_root: &Path,
) -> Result<(u16, String)> {
    let configured_servers = mcp_servers
        .lock()
        .map_err(|_| anyhow::anyhow!("lock MCP server store"))?
        .values()
        .cloned()
        .collect::<Vec<_>>();
    let tools = tool_registry
        .list_tools()
        .map_err(|error| anyhow::anyhow!("list installed Arts for MCP usage: {error}"))?;
    let mut servers = configured_servers
        .into_iter()
        .map(|server| mcp_server_view(&server, &tools, control_plane_root))
        .collect::<Vec<_>>();
    servers.sort_by(|left, right| {
        left.get("id")
            .and_then(Value::as_str)
            .cmp(&right.get("id").and_then(Value::as_str))
    });
    Ok((200, serde_json::to_string(&json!({ "servers": servers }))?))
}

fn mcp_server_view(
    server: &McpServerConfig,
    tools: &[ToolDefinition],
    control_plane_root: &Path,
) -> Value {
    let mut value = serde_json::to_value(server).unwrap_or_else(|_| json!({}));
    let Some(object) = value.as_object_mut() else {
        return value;
    };
    object.remove("credentialBindings");
    let package_id = server
        .package
        .as_ref()
        .map(|package| package.qualified_id.clone());
    let used_by_art_ids = tools
        .iter()
        .filter(|tool| {
            let dependencies = read_dependencies(tool);
            dependencies
                .mcp_servers
                .iter()
                .any(|dependency| Some(dependency.id.as_str()) == package_id.as_deref())
                || tool
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.pointer("/mcp/serverId"))
                    .and_then(Value::as_str)
                    == Some(server.id.as_str())
        })
        .map(ToolDefinition::qualified_id)
        .collect::<Vec<_>>();
    let credential_required = server
        .credential_requirements
        .iter()
        .any(|requirement| requirement.required);
    let required_bindings = server
        .credential_requirements
        .iter()
        .filter(|requirement| requirement.required)
        .filter_map(|requirement| {
            server
                .credential_bindings
                .get(&requirement.id)
                .map(|name| (requirement.id.clone(), name.clone()))
        })
        .collect::<BTreeMap<_, _>>();
    let credential_bound = !credential_required
        || (required_bindings.len()
            == server
                .credential_requirements
                .iter()
                .filter(|requirement| requirement.required)
                .count()
            && CredentialStore::new(control_plane_root)
                .grants_for_mcp_bindings(&server.id, &required_bindings)
                .is_ok());
    object.insert(
        "source".to_owned(),
        Value::String(if server.package.is_some() {
            "package".to_owned()
        } else {
            "manual".to_owned()
        }),
    );
    object.insert("credentialRequired".to_owned(), json!(credential_required));
    object.insert("credentialBound".to_owned(), json!(credential_bound));
    object.insert("usageCount".to_owned(), json!(used_by_art_ids.len()));
    object.insert("usedByArtIds".to_owned(), json!(used_by_art_ids));
    value
}

fn put_mcp_server(
    path_id: &str,
    body: &str,
    mcp_servers: &SharedMcpServerStore,
    store_path: &Path,
) -> Result<(u16, String)> {
    let server = match serde_json::from_str::<McpServerConfig>(body) {
        Ok(server) => server,
        Err(error) => return invalid_request(error.to_string()),
    };
    if server.id != path_id {
        return id_mismatch("server", path_id, &server.id);
    }
    if server.package.is_some() {
        return structured_error(
            400,
            json!({
                "code": "mcp_package_state_read_only",
                "message": "package metadata can only be changed through MCP package install or upgrade",
            }),
        );
    }
    if let Err(error) = server.validate() {
        return invalid_request(error.to_string());
    }

    {
        let mut guard = mcp_servers
            .lock()
            .map_err(|_| anyhow::anyhow!("lock MCP server store"))?;
        let previous = guard.insert(server.id.clone(), server.clone());
        if let Err(error) = persist_mcp_servers_snapshot(store_path, &guard) {
            match previous {
                Some(previous_server) => {
                    guard.insert(server.id.clone(), previous_server);
                }
                None => {
                    guard.remove(&server.id);
                }
            }
            return Err(error);
        }
    }

    Ok((200, serde_json::to_string(&json!({ "server": server }))?))
}

#[derive(Debug)]
enum McpServerPackageBase64Error {
    Invalid(base64::DecodeError),
    TooLarge,
}

/// Rejects an oversized encoded archive before Base64 decoding allocates its output buffer.
fn decode_mcp_server_package_base64(
    encoded: &str,
    max_decoded_bytes: usize,
) -> Result<Vec<u8>, McpServerPackageBase64Error> {
    let max_encoded_bytes = max_decoded_bytes
        .checked_add(2)
        .and_then(|value| value.checked_div(3))
        .and_then(|value| value.checked_mul(4))
        .unwrap_or(usize::MAX);
    if encoded.len() > max_encoded_bytes {
        return Err(McpServerPackageBase64Error::TooLarge);
    }
    let bytes = BASE64
        .decode(encoded)
        .map_err(McpServerPackageBase64Error::Invalid)?;
    if bytes.len() > max_decoded_bytes {
        return Err(McpServerPackageBase64Error::TooLarge);
    }
    Ok(bytes)
}

fn install_mcp_server_package(
    body: &str,
    mcp_servers: &SharedMcpServerStore,
    control_plane_root: &Path,
    store_path: &Path,
) -> Result<(u16, String)> {
    let request: McpServerPackageInstallRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    if request.zip_base64.trim().is_empty() {
        return invalid_request("zipBase64 is required");
    }
    let package_bytes = match decode_mcp_server_package_base64(
        request.zip_base64.trim(),
        MAX_MCP_SERVER_PACKAGE_BYTES,
    ) {
        Ok(bytes) => bytes,
        Err(McpServerPackageBase64Error::Invalid(error)) => {
            return invalid_request(format!("zipBase64 is invalid: {error}"))
        }
        Err(McpServerPackageBase64Error::TooLarge) => {
            return structured_error(
                400,
                json!({
                    "code": "invalid_mcp_server_package",
                    "message": McpPackageError::PackageTooLarge.to_string(),
                }),
            )
        }
    };
    let mut installed = match install_server_package(control_plane_root, &package_bytes) {
        Ok(server) => server,
        Err(error) => {
            return structured_error(
                400,
                json!({
                    "code": "invalid_mcp_server_package",
                    "message": error.to_string(),
                }),
            )
        }
    };
    let mut guard = mcp_servers
        .lock()
        .map_err(|_| anyhow::anyhow!("lock MCP server store"))?;
    let previous = guard.get(&installed.id).cloned();
    if let Some(existing) = previous.as_ref() {
        let existing_package = existing
            .package
            .as_ref()
            .map(|package| package.qualified_id.as_str());
        let installed_package = installed
            .package
            .as_ref()
            .map(|package| package.qualified_id.as_str());
        if existing_package != installed_package {
            let _ = uninstall_server_package(control_plane_root, &installed);
            return structured_error(
                409,
                json!({
                    "code": "mcp_server_id_conflict",
                    "message": format!("MCP server id `{}` is already used by another configuration", installed.id),
                    "serverId": installed.id,
                }),
            );
        }
        installed.credential_bindings = existing.credential_bindings.clone();
        installed.enabled = existing.enabled;
    }
    guard.insert(installed.id.clone(), installed.clone());
    if let Err(error) = persist_mcp_servers_snapshot(store_path, &guard) {
        match previous {
            Some(previous) => {
                guard.insert(installed.id.clone(), previous);
            }
            None => {
                guard.remove(&installed.id);
            }
        }
        return Err(error);
    }
    drop(guard);
    Ok((
        200,
        serde_json::to_string(&json!({
            "server": mcp_server_view(&installed, &[], control_plane_root),
        }))?,
    ))
}

fn set_mcp_server_enabled(
    path_id: &str,
    body: &str,
    mcp_servers: &SharedMcpServerStore,
    store_path: &Path,
) -> Result<(u16, String)> {
    let request: ToggleRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let mut guard = mcp_servers
        .lock()
        .map_err(|_| anyhow::anyhow!("lock MCP server store"))?;
    let Some(previous) = guard.get(path_id).cloned() else {
        return structured_error(
            404,
            json!({ "code": "mcp_server_not_found", "serverId": path_id }),
        );
    };
    let mut updated = previous.clone();
    updated.enabled = request.enabled;
    guard.insert(path_id.to_owned(), updated.clone());
    if let Err(error) = persist_mcp_servers_snapshot(store_path, &guard) {
        guard.insert(path_id.to_owned(), previous);
        return Err(error);
    }
    Ok((200, serde_json::to_string(&json!({ "server": updated }))?))
}
