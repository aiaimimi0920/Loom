// MCP connection tests, tool calls, package plans, and safe Python package names.
fn test_mcp_connection(body: &str) -> Result<(u16, String)> {
    let config: McpServerConfig = match serde_json::from_str(body) {
        Ok(config) => config,
        Err(error) => {
            return structured_error(
                400,
                json!({
                    "code": "invalid_mcp_server",
                    "message": format!("invalid MCP server config: {error}"),
                }),
            );
        }
    };

    let mut client = match McpClient::connect(&config) {
        Ok(client) => client,
        Err(error) => {
            return Ok((
                200,
                serde_json::to_string(&json!({
                    "success": false,
                    "tools": [],
                    "error": error.to_string(),
                }))?,
            ));
        }
    };
    let server_info = match client.initialize() {
        Ok(server_info) => server_info,
        Err(error) => {
            return Ok((
                200,
                serde_json::to_string(&json!({
                    "success": false,
                    "tools": [],
                    "error": error.to_string(),
                }))?,
            ));
        }
    };
    let tools_result = match client.list_tools() {
        Ok(tools) => tools,
        Err(error) => {
            return Ok((
                200,
                serde_json::to_string(&json!({
                    "success": false,
                    "tools": [],
                    "server_info": server_info,
                    "serverInfo": server_info,
                    "error": error.to_string(),
                }))?,
            ));
        }
    };
    let tools = tools_result
        .get("tools")
        .cloned()
        .unwrap_or_else(|| json!([]));

    Ok((
        200,
        serde_json::to_string(&json!({
            "success": true,
            "tools": tools,
            "server_info": server_info,
            "serverInfo": server_info,
        }))?,
    ))
}

fn call_mcp_tool(body: &str) -> Result<(u16, String)> {
    let request: McpToolCallRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let tool_name = request.tool_name.trim();
    if tool_name.is_empty() {
        return invalid_request("toolName is required");
    }

    let config = McpServerConfig {
        id: "loom-direct".to_owned(),
        name: "Loom Direct MCP".to_owned(),
        description: "One-shot Loom MCP tool call".to_owned(),
        command: request.command.trim().to_owned(),
        args: request.args,
        env: request.env,
        transport: request.transport,
        url: request.url.trim().to_owned(),
        headers: request.headers,
        credential_env: BTreeMap::new(),
        credential_headers: BTreeMap::new(),
        credential_bindings: BTreeMap::new(),
        credential_requirements: Vec::new(),
        tools: Vec::new(),
        package: None,
        enabled: true,
    };
    if let Err(error) = config.validate() {
        return invalid_request(error.to_string());
    }
    let mut client = match McpClient::connect(&config) {
        Ok(client) => client,
        Err(error) => {
            return structured_error(
                502,
                json!({
                    "code": "mcp_spawn_failed",
                    "message": error.to_string(),
                }),
            );
        }
    };
    if let Err(error) = client.initialize() {
        return structured_error(
            502,
            json!({
                "code": "mcp_initialize_failed",
                "message": error.to_string(),
            }),
        );
    }
    let result = match client.call_tool(tool_name, request.tool_args) {
        Ok(result) => result,
        Err(error) => {
            return structured_error(
                502,
                json!({
                    "code": "mcp_tool_call_failed",
                    "message": error.to_string(),
                }),
            );
        }
    };

    Ok((
        200,
        serde_json::to_string(&json!({
            "status": "succeeded",
            "jsonrpc": "2.0",
            "id": 3,
            "result": result,
        }))?,
    ))
}

fn check_mcp_package_installed(body: &str) -> Result<(u16, String)> {
    let request: McpPackageCheckRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let module_name = request.module_name.trim();
    if !is_safe_python_module_name(module_name) {
        return invalid_request("moduleName must be a dotted Python module identifier");
    }

    let python = resolve_mcp_package_python();
    let output = Command::new(&python)
        .args(["-c", &format!("import {module_name}")])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    let value = match output {
        Ok(output) => json!({
            "installed": output.status.success(),
            "module": module_name,
            "python": python.to_string_lossy(),
            "stdout": String::from_utf8_lossy(&output.stdout).to_string(),
            "stderr": String::from_utf8_lossy(&output.stderr).to_string(),
        }),
        Err(error) => json!({
            "installed": false,
            "module": module_name,
            "python": python.to_string_lossy(),
            "error": error.to_string(),
        }),
    };

    Ok((200, serde_json::to_string(&value)?))
}

fn build_mcp_package_install_plan(body: &str) -> Result<(u16, String)> {
    let request: McpPackageInstallPlanRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let package_name = request.package_name.trim();
    if !is_safe_python_package_name(package_name) {
        return invalid_request("packageName must be a safe pip package specifier");
    }
    let python = resolve_mcp_package_python();
    let command = vec![
        python.to_string_lossy().to_string(),
        "-m".to_owned(),
        "pip".to_owned(),
        "install".to_owned(),
        "--upgrade".to_owned(),
        package_name.to_owned(),
    ];

    Ok((
        200,
        serde_json::to_string(&json!({
            "package": package_name,
            "sideEffect": false,
            "mode": "safe-preview",
            "command": command,
            "message": "Install plan prepared. Loom does not run arbitrary package installation from this preview.",
        }))?,
    ))
}

fn resolve_mcp_package_python() -> PathBuf {
    if let Some(path) = std::env::var_os("LOOM_PYTHON").map(PathBuf::from) {
        return path;
    }
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(base) = current_exe.parent() {
            let packaged = base.join("bin").join("python-embed").join("python.exe");
            if packaged.exists() {
                return packaged;
            }
        }
    }
    PathBuf::from(if cfg!(windows) {
        "python.exe"
    } else {
        "python3"
    })
}

fn is_safe_python_module_name(value: &str) -> bool {
    !value.is_empty()
        && value.split('.').all(|segment| {
            let mut chars = segment.chars();
            let Some(first) = chars.next() else {
                return false;
            };
            (first == '_' || first.is_ascii_alphabetic())
                && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
        })
}

fn is_safe_python_package_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '[' | ']' | '='))
}

fn list_tools(tool_registry: &ToolRegistry) -> Result<(u16, String)> {
    let tools = match tool_registry.list_tools() {
        Ok(tools) => tools,
        Err(error) => return tool_registry_error_response(error),
    };
    Ok((200, serde_json::to_string(&json!({ "tools": tools }))?))
}
