// MCP credentials, package lifecycle, registry refresh, and tool listing.
fn update_mcp_server_credentials(
    path_id: &str,
    body: &str,
    mcp_servers: &SharedMcpServerStore,
    control_plane_root: &Path,
    store_path: &Path,
) -> Result<(u16, String)> {
    let request: McpServerCredentialUpdateRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let previous = mcp_servers
        .lock()
        .map_err(|_| anyhow::anyhow!("lock MCP server store"))?
        .get(path_id)
        .cloned();
    let Some(previous) = previous else {
        return structured_error(
            404,
            json!({ "code": "mcp_server_not_found", "serverId": path_id }),
        );
    };
    let credential_ids = previous
        .credential_requirements
        .iter()
        .map(|requirement| requirement.id.as_str())
        .collect::<BTreeSet<_>>();
    if let Some(unknown) = request
        .values
        .keys()
        .map(String::as_str)
        .chain(request.clear.iter().map(String::as_str))
        .find(|id| !credential_ids.contains(id))
    {
        return invalid_request(format!("unknown MCP credential id `{unknown}`"));
    }
    let store = CredentialStore::new(control_plane_root);
    let scope = CredentialScope {
        framework_id: None,
        art_id: None,
        mcp_server_id: Some(path_id.to_owned()),
    };
    let mut updated = previous.clone();
    for id in request.clear {
        if let Some(name) = updated.credential_bindings.remove(&id) {
            let _ = store.delete(&name, &scope);
        }
    }
    for (id, value) in request.values {
        if value.is_empty() {
            return invalid_request(format!("MCP credential `{id}` must not be empty"));
        }
        let name = mcp_credential_name(path_id, &id);
        if let Err(error) = store.upsert(CredentialInput {
            name: name.clone(),
            value,
            value_type: CredentialValueType::String,
            scope: scope.clone(),
            expires_at: None,
        }) {
            return structured_error(
                400,
                json!({ "code": "credential_store_failed", "message": error.to_string() }),
            );
        }
        updated.credential_bindings.insert(id, name);
    }
    let mut guard = mcp_servers
        .lock()
        .map_err(|_| anyhow::anyhow!("lock MCP server store"))?;
    guard.insert(path_id.to_owned(), updated.clone());
    if let Err(error) = persist_mcp_servers_snapshot(store_path, &guard) {
        guard.insert(path_id.to_owned(), previous);
        return Err(error);
    }
    Ok((
        200,
        serde_json::to_string(&json!({
            "server": mcp_server_view(&updated, &[], control_plane_root),
        }))?,
    ))
}

fn mcp_credential_name(server_id: &str, credential_id: &str) -> String {
    let digest = format!("{:x}", Sha256::digest(server_id.as_bytes()));
    format!("mcp-{}-{credential_id}", &digest[..16])
}

fn materialize_mcp_server_credentials(
    server: &McpServerConfig,
    control_plane_root: &Path,
) -> std::result::Result<McpServerConfig, String> {
    let grants = CredentialStore::new(control_plane_root)
        .grants_for_mcp_bindings(&server.id, &server.credential_bindings)
        .map_err(|error| error.to_string())?;
    let values = grants
        .into_iter()
        .map(|grant| (grant.name, grant.value))
        .collect::<BTreeMap<_, _>>();
    let mut resolved = server.clone();
    for requirement in resolved
        .credential_requirements
        .iter()
        .filter(|requirement| requirement.required)
    {
        if !values.contains_key(&requirement.id) {
            return Err(format!(
                "MCP credential `{}` is not configured",
                requirement.label
            ));
        }
    }
    for (name, alias) in &resolved.credential_env {
        if let Some(value) = values.get(alias) {
            resolved.env.insert(name.clone(), value.clone());
        }
    }
    for (name, alias) in &resolved.credential_headers {
        if let Some(value) = values.get(alias) {
            resolved.headers.insert(name.clone(), value.clone());
        }
    }
    Ok(resolved)
}

fn test_installed_mcp_server(
    path_id: &str,
    mcp_servers: &SharedMcpServerStore,
    control_plane_root: &Path,
) -> Result<(u16, String)> {
    let server = mcp_servers
        .lock()
        .map_err(|_| anyhow::anyhow!("lock MCP server store"))?
        .get(path_id)
        .cloned();
    let Some(server) = server else {
        return structured_error(
            404,
            json!({ "code": "mcp_server_not_found", "serverId": path_id }),
        );
    };
    let resolved = match materialize_mcp_server_credentials(&server, control_plane_root) {
        Ok(server) => server,
        Err(message) => {
            return structured_error(
                409,
                json!({ "code": "mcp_credential_missing", "message": message }),
            )
        }
    };
    test_mcp_connection(&serde_json::to_string(&resolved)?)
}

fn delete_mcp_server(
    path_id: &str,
    mcp_servers: &SharedMcpServerStore,
    store_path: &Path,
    control_plane_root: &Path,
) -> Result<(u16, String)> {
    let removed = {
        let mut guard = mcp_servers
            .lock()
            .map_err(|_| anyhow::anyhow!("lock MCP server store"))?;
        let Some(removed) = guard.remove(path_id) else {
            return structured_error(
                404,
                json!({
                    "code": "mcp_server_not_found",
                    "message": format!("MCP server `{path_id}` was not found"),
                    "server_id": path_id,
                }),
            );
        };
        if let Err(error) = persist_mcp_servers_snapshot(store_path, &guard) {
            guard.insert(path_id.to_owned(), removed.clone());
            return Err(error);
        }
        removed
    };
    if let Err(error) = uninstall_server_package(control_plane_root, &removed) {
        let mut guard = mcp_servers
            .lock()
            .map_err(|_| anyhow::anyhow!("lock MCP server store"))?;
        guard.insert(path_id.to_owned(), removed.clone());
        let _ = persist_mcp_servers_snapshot(store_path, &guard);
        return structured_error(
            500,
            json!({ "code": "mcp_uninstall_failed", "message": error.to_string() }),
        );
    }
    let credential_store = CredentialStore::new(control_plane_root);
    let scope = CredentialScope {
        framework_id: None,
        art_id: None,
        mcp_server_id: Some(path_id.to_owned()),
    };
    for name in removed.credential_bindings.values() {
        let _ = credential_store.delete(name, &scope);
    }

    Ok((
        200,
        serde_json::to_string(&json!({ "serverId": path_id, "deleted": true }))?,
    ))
}

fn fetch_mcp_registry(path: &str, endpoint: &str, cache_path: &Path) -> Result<(u16, String)> {
    let _fetch_guard = MCP_REGISTRY_FETCH_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| anyhow::anyhow!("lock MCP Registry fetch state"))?;
    let search = query_value(path, "search");
    let cursor = query_value(path, "cursor");
    let limit = query_value(path, "limit").and_then(|value| value.parse::<u32>().ok());
    let updated_since =
        query_value(path, "updated_since").or_else(|| query_value(path, "updatedSince"));
    let version = query_value(path, "version");
    let include_deleted = query_value(path, "include_deleted")
        .or_else(|| query_value(path, "includeDeleted"))
        .is_some_and(|value| matches!(value.as_str(), "1" | "true"));
    let refresh =
        query_value(path, "refresh").is_some_and(|value| matches!(value.as_str(), "1" | "true"));
    let url = build_mcp_registry_url(
        endpoint,
        search.as_deref(),
        limit,
        cursor.as_deref(),
        updated_since.as_deref(),
        version.as_deref(),
        include_deleted,
    );
    let now = unix_time_millis();
    let cached = load_mcp_registry_cache(cache_path).entries.remove(&url);
    if !refresh {
        if let Some(entry) = cached.as_ref().filter(|entry| {
            now.saturating_sub(entry.fetched_at_ms) <= MCP_REGISTRY_CACHE_FRESH_MILLIS
        }) {
            let response = annotate_mcp_registry_response(
                entry.response.clone(),
                "cache",
                false,
                entry.fetched_at_ms,
            );
            return Ok((200, serde_json::to_string(&response)?));
        }
    }

    let policy = user_configured_outbound_policy();
    let client = secure_client(
        "Loom/0.1 MCP Registry Client",
        Duration::from_secs(20),
        policy.clone(),
    )
    .map_err(|error| anyhow::anyhow!("build MCP Registry client: {error}"))?;
    let mut bytes = None;
    let mut fetch_error = None;
    for attempt in 0..MCP_REGISTRY_FETCH_ATTEMPTS {
        match get_bounded(&client, &url, &policy, MAX_REGISTRY_RESPONSE_BYTES) {
            Ok(response_bytes) => {
                bytes = Some(response_bytes);
                break;
            }
            Err(error) => {
                fetch_error = Some(error.to_string());
                if attempt + 1 < MCP_REGISTRY_FETCH_ATTEMPTS {
                    std::thread::sleep(MCP_REGISTRY_RETRY_DELAY);
                }
            }
        }
    }
    let bytes = match bytes {
        Some(bytes) => bytes,
        None => {
            let error = fetch_error.unwrap_or_else(|| "unknown MCP Registry error".to_owned());
            if let Some(entry) = cached {
                let response = annotate_mcp_registry_response(
                    entry.response,
                    "cache",
                    true,
                    entry.fetched_at_ms,
                );
                return Ok((200, serde_json::to_string(&response)?));
            }
            return structured_error(
                502,
                json!({
                    "code": "mcp_registry_unavailable",
                    "message": format!("failed to fetch MCP Registry: {error}"),
                    "url": url,
                }),
            );
        }
    };
    let value = match serde_json::from_slice::<Value>(&bytes) {
        Ok(value) => value,
        Err(error) => {
            if let Some(entry) = cached {
                let response = annotate_mcp_registry_response(
                    entry.response,
                    "cache",
                    true,
                    entry.fetched_at_ms,
                );
                return Ok((200, serde_json::to_string(&response)?));
            }
            return structured_error(
                502,
                json!({
                    "code": "mcp_registry_invalid_json",
                    "message": format!("MCP Registry returned invalid JSON: {error}"),
                    "url": url,
                }),
            );
        }
    };

    cache_mcp_registry_response(cache_path, &url, &value, now);
    let response = annotate_mcp_registry_response(value, "network", false, now);
    Ok((200, serde_json::to_string(&response)?))
}
