// Unified multi-platform store catalog (`arts` + `capabilities` + `mcp_servers`) endpoint.

fn current_host_platform_tag() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "windows-x64",
        ("windows", "aarch64") => "windows-arm64",
        ("linux", "x86_64") => "linux-x64",
        ("linux", "aarch64") => "linux-arm64",
        ("macos", "x86_64") => "macos-x64",
        ("macos", "aarch64") => "macos-arm64",
        _ => "universal",
    }
}

fn route_unified_store(
    request: &ParsedHttpRequest,
    route_path: &str,
    tool_registry: &ToolRegistry,
    _capability_runtime: &SharedCapabilityRuntime,
    mcp_servers: &SharedMcpServerStore,
    control_plane_root: &Path,
) -> Option<Result<(u16, String)>> {
    if request.method != "GET" || route_path != "/v1/store/catalog" {
        return None;
    }
    let tools = match tool_registry.list_tools() {
        Ok(tools) => tools,
        Err(error) => return Some(Err(error.into())),
    };
    let installed_arts = tools
        .into_iter()
        .map(|tool| {
            json!({
                "id": tool.id,
                "qualifiedId": tool.qualified_id(),
                "kind": "art",
                "name": tool.name,
                "description": tool.description,
                "enabled": tool.enabled,
            })
        })
        .collect::<Vec<_>>();

    let installed_capabilities = match list_capability_plugins(control_plane_root) {
        Ok((200, body)) => match serde_json::from_str::<Value>(&body) {
            Ok(value) => match value.get("plugins").filter(|v| v.is_array()) {
                Some(plugins) => plugins.clone(),
                None => return Some(Err(anyhow::anyhow!("capability inventory omitted plugins"))),
            },
            Err(error) => return Some(Err(error.into())),
        },
        other => return Some(other),
    };

    let installed_mcp = match mcp_servers.lock() {
        Ok(servers) => servers
            .values()
            .map(|server| {
                json!({
                    "id": server.id,
                    "kind": "mcp_server",
                    "name": server.name,
                    "transport": server.transport,
                    "enabled": server.enabled,
                    "tools": server.tools,
                })
            })
            .collect::<Vec<_>>(),
        Err(_) => return Some(Err(anyhow::anyhow!("MCP server store lock poisoned"))),
    };

    let remote = resolve_art_store_url().and_then(|store_url| {
        let client = art_store_client().ok()?;
        let policy = user_configured_outbound_policy();
        let target = format!("{}/catalog", store_url.trim_end_matches('/'));
        let bytes = get_bounded(&client, &target, &policy, 8 * 1024 * 1024).ok()?;
        serde_json::from_slice::<Value>(&bytes).ok()
    });

    let payload = json!({
        "hostPlatform": current_host_platform_tag(),
        "supportedPlatforms": ["windows-x64", "windows-arm64", "linux-x64", "linux-arm64", "macos-x64", "macos-arm64", "universal"],
        "installed": {
            "arts": installed_arts,
            "capabilities": installed_capabilities,
            "mcpServers": installed_mcp,
        },
        "remote": remote,
    });
    Some(
        serde_json::to_string(&payload)
            .map(|body| (200, body))
            .map_err(Into::into),
    )
}
