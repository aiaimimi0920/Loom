// Top-level MCP execution orchestration and resolved-server identity validation.
pub fn execute(request: &FrameworkExecuteRequest, art_dir: &Path) -> Result<McpExecution, String> {
    let config = load_config(art_dir)?;
    let calls = resolve_calls(request, &config)?;
    let mut warnings = dropped_argument_warnings(request, &config)?;
    let redactor = CredentialRedactor::new(&request.context.credentials);
    let resolved = request
        .context
        .mcp_server
        .as_ref()
        .ok_or_else(|| "MCP dependency was not resolved by the Loom host".to_owned())?;
    validate_resolved_server(&config, resolved)?;
    if calls.is_empty() {
        return Ok(McpExecution {
            server_id: resolved.id.clone(),
            tool_name: None,
            result: None,
            results: BTreeMap::new(),
            skipped: true,
            warnings,
        });
    }
    let transport = match resolved.transport.as_str() {
        "stdio" => McpTransport::Stdio,
        "streamable-http" => McpTransport::StreamableHttp,
        other => return Err(format!("resolved MCP transport `{other}` is unsupported")),
    };
    let environment = build_environment(request, resolved)?;
    let headers = build_headers(request, resolved)?;
    let mut server = match transport {
        McpTransport::Stdio => {
            if resolved.command.trim().is_empty() {
                return Err("resolved stdio MCP command is missing".to_owned());
            }
            McpServerConfig::new(
                resolved.id.clone(),
                format!("{} MCP server", resolved.id),
                expand_stdio_command(&resolved.command, request, art_dir)?,
            )
        }
        McpTransport::StreamableHttp => McpServerConfig::remote(
            resolved.id.clone(),
            format!("{} MCP server", resolved.id),
            resolved.url.clone(),
        ),
    };
    server.args = match transport {
        McpTransport::Stdio => resolved
            .args
            .iter()
            .map(|argument| expand_runtime_paths(argument, request, art_dir))
            .collect(),
        McpTransport::StreamableHttp if resolved.args.is_empty() => Vec::new(),
        McpTransport::StreamableHttp => {
            return Err(
                "resolved streamable-http MCP server must not declare process arguments or local path placeholders"
                    .to_owned(),
            )
        }
    };
    if transport == McpTransport::Stdio && !headers.is_empty() {
        return Err("resolved stdio MCP server must not declare HTTP headers".to_owned());
    }
    if transport == McpTransport::StreamableHttp && !environment.is_empty() {
        return Err(
            "resolved streamable-http MCP server must not declare process environment".to_owned(),
        );
    }
    server.env = environment;
    server.headers = headers;

    let mut batch = execute_tools(&server, &calls, &config.argument_aliases)
        .map_err(|error| redactor.redact_text(&error))?;
    if let Some(error) = batch.close_error.take() {
        warnings.push(McpExecutionWarning {
            code: "mcp_session_close_failed".to_owned(),
            message: redactor.redact_text(&error),
            dropped_argument_count: 0,
            dropped_argument_names: Vec::new(),
        });
    }
    for outcome in &mut batch.outcomes {
        match outcome {
            McpCallOutcome::Success(value) => redactor.redact_value(value),
            McpCallOutcome::Failure(error) => *error = redactor.redact_text(error),
        }
    }
    if config.calls.is_empty() {
        let call = calls
            .first()
            .ok_or_else(|| "legacy MCP Art did not resolve a tool call".to_owned())?;
        let outcome = batch
            .outcomes
            .into_iter()
            .next()
            .ok_or_else(|| "legacy MCP Art did not return a tool result".to_owned())?;
        let result = match outcome {
            McpCallOutcome::Success(result) => result,
            McpCallOutcome::Failure(error) => return Err(error),
        };
        return Ok(McpExecution {
            server_id: resolved.id.clone(),
            tool_name: Some(call.tool_name.clone()),
            result: Some(result),
            results: BTreeMap::new(),
            skipped: false,
            warnings,
        });
    }

    let results = calls
        .into_iter()
        .zip(batch.outcomes)
        .map(|(call, outcome)| {
            let (success, result, error) = match outcome {
                McpCallOutcome::Success(result) => (true, Some(result), None),
                McpCallOutcome::Failure(error) => (false, None, Some(error)),
            };
            (
                call.id,
                McpCallExecution {
                    tool_name: call.tool_name,
                    success,
                    result,
                    error,
                },
            )
        })
        .collect();
    Ok(McpExecution {
        server_id: resolved.id.clone(),
        tool_name: None,
        result: None,
        results,
        skipped: false,
        warnings,
    })
}

/// Hold the server the host resolved to what the Art declared: same server id, same package, and a
/// version the declared requirement admits. The version half is the part that used to be missing —
/// `metadata.mcp.version` was parsed and then ignored, so an Art pinned to `=2.9.0` ran happily
/// against whatever the host had installed.
fn validate_resolved_server(
    config: &McpArtConfig,
    resolved: &FrameworkMcpServer,
) -> Result<(), String> {
    if resolved.id != config.server_id {
        return Err(format!(
            "resolved MCP server `{}` does not match Art dependency `{}`",
            resolved.id, config.server_id
        ));
    }
    if resolved.package_id != config.package_id {
        return Err(format!(
            "resolved MCP package `{}` does not match Art dependency `{}`",
            resolved.package_id, config.package_id
        ));
    }
    if resolved.version.trim().is_empty() {
        return Err(format!(
            "resolved MCP package `{}` reported no version, so Art dependency `{}` cannot be checked",
            resolved.package_id, config.version
        ));
    }
    if let Some(bounds) = requirement_bounds(&config.version) {
        if !bounds.admits(&resolved.version) {
            return Err(format!(
                "resolved MCP package `{}` version `{}` does not satisfy Art dependency `{}` (needs >= {} and < {})",
                resolved.package_id,
                resolved.version.trim(),
                config.version.trim(),
                format_version(bounds.lower),
                format_version(bounds.upper)
            ));
        }
    }
    Ok(())
}
