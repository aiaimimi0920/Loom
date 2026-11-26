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
    let installed_arts = tool_registry
        .list_tools()
        .unwrap_or_default()
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

    let installed_capabilities = list_capability_plugins(control_plane_root)
        .ok()
        .and_then(|(status, body)| (status == 200).then(|| serde_json::from_str::<Value>(&body).ok()).flatten())
        .and_then(|v| v.get("plugins").cloned())
        .unwrap_or_else(|| json!([]));

    let installed_mcp = mcp_servers
        .lock()
        .ok()
        .map(|servers| {
            servers
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
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let remote = resolve_art_store_url().and_then(|store_url| {
        let client = art_store_client().ok()?;
        let policy = user_configured_outbound_policy();
        let target = format!("{}/catalog", store_url.trim_end_matches('/'));
        let bytes = get_bounded(&client, &target, &policy, 8 * 1024 * 1024).ok()?;
        serde_json::from_slice::<Value>(&bytes).ok()
    });

    let payload = json!({
        "hostPlatform": current_host_platform_tag(),
        "supportedPlatforms": ["windows-x64", "linux-x64", "linux-arm64", "universal"],
        "installed": {
            "arts": installed_arts,
            "capabilities": installed_capabilities,
            "mcpServers": installed_mcp,
        },
        "remote": remote,
    });
    Some(serde_json::to_string(&payload).map(|body| (200, body)).map_err(Into::into))
}
