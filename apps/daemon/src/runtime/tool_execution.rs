// Run evidence, registered tool execution, and shared-image storage and conversion.
fn record_hook_art_run(
    run_store: &SharedRunStore,
    tool_registry: &ToolRegistry,
    request: &HookArtExecuteRequest,
    started_at: Instant,
    succeeded: bool,
    error: Option<&loom_protocol::SurfaceExecutionError>,
) -> String {
    let run_id = loom_core::RunId::new().to_string();
    let duration_ms = started_at.elapsed().as_millis().min(u64::MAX as u128) as u64;
    let tool = tool_registry.get_tool(&request.art_id).ok().flatten();
    let qualified_id = tool
        .as_ref()
        .map(ToolDefinition::qualified_id)
        .unwrap_or_else(|| request.art_id.clone());
    let framework_id = tool.as_ref().map(framework_id_for_tool);
    let package = tool
        .as_ref()
        .and_then(|tool| tool.metadata.as_ref())
        .and_then(|metadata| metadata.get("artPackage"));
    let status = if succeeded { "succeeded" } else { "failed" };
    let run = json!({
        "id": run_id,
        "capability": "art.execute",
        "status": status,
        "surface": "loom_hook_v1",
        "externalRequestId": request.request_id,
        "nodeId": request.node_id,
        "generation": request.generation,
        "toolId": request.art_id,
        "qualifiedId": qualified_id,
        "frameworkId": framework_id,
        "package": package.map(|package| json!({
            "version": package.get("version").cloned(),
            "digest": package.get("digest").cloned(),
            "trustStatus": package.get("trustStatus").cloned(),
        })),
        "durationMs": duration_ms,
        "error": error.map(|error| json!({ "code": error.code, "message": error.message })),
    });
    let started = RunEventDraft::new(
        "external_tool_started",
        json!({
            "surface": "loom_hook_v1",
            "externalRequestId": request.request_id,
            "toolId": request.art_id,
            "qualifiedId": qualified_id,
            "status": "running",
        }),
    );
    let finished = RunEventDraft::new(
        if succeeded {
            "external_tool_completed"
        } else {
            "external_tool_failed"
        },
        json!({
            "surface": "loom_hook_v1",
            "externalRequestId": request.request_id,
            "toolId": request.art_id,
            "qualifiedId": qualified_id,
            "status": status,
            "durationMs": duration_ms,
        }),
    );
    let (Ok(started), Ok(finished)) = (started, finished) else {
        return run_id;
    };
    if let Ok(mut store) = run_store.lock() {
        let _ = store.insert_run(run, vec![started, finished]);
    }
    run_id
}

fn execute_registered_tool(
    tool_id: &str,
    body: &str,
    mcp_servers: &SharedMcpServerStore,
    tool_registry: &ToolRegistry,
    workflow_store: &WorkflowStore,
    framework_registry: &FrameworkRegistry,
    run_store: &SharedRunStore,
    control_plane_root: &Path,
) -> Result<(u16, String)> {
    let request_body = if body.trim().is_empty() { "{}" } else { body };
    let request = match serde_json::from_str::<ExecuteToolRequest>(request_body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let tool = match tool_registry.get_tool(tool_id) {
        Ok(Some(tool)) => tool,
        Ok(None) => {
            return structured_error(
                404,
                json!({
                    "code": "tool_not_found",
                    "message": format!("tool `{tool_id}` was not found"),
                    "tool_id": tool_id,
                }),
            );
        }
        Err(error) => return tool_registry_error_response(error),
    };
    let tool = match resolve_registered_tool_package(
        &tool,
        tool_registry,
        framework_registry,
        control_plane_root,
    ) {
        Ok(tool) => tool,
        Err(error) => {
            return structured_error(
                409,
                json!({
                    "code": "art_package_integrity_failed",
                    "message": error.to_string(),
                    "artId": tool.qualified_id(),
                }),
            );
        }
    };
    if let ToolExecution::FrameworkArt { framework } = &tool.execution {
        if !tool.enabled {
            return structured_error(
                409,
                json!({
                    "code": "art_disabled",
                    "message": format!("Art {tool_id} 已禁用"),
                    "artId": tool_id,
                }),
            );
        }
        let (ready, detail) = framework_registry.readiness(framework);
        if !ready {
            return structured_error(
                409,
                json!({
                    "code": "framework_not_ready",
                    "message": format!("Art {tool_id} 的框架 {framework} 不可运行：{detail}"),
                    "framework": framework,
                    "artId": tool_id,
                }),
            );
        }
    }
    let servers = mcp_servers
        .lock()
        .map_err(|_| anyhow::anyhow!("lock MCP server store"))?
        .values()
        .cloned()
        .collect::<Vec<_>>();
    let run_id = loom_core::RunId::new().to_string();
    let started_at = Instant::now();
    let mut run = external_tool_run(&run_id, &tool, &request.arguments);
    let started_event = RunEventDraft::new(
        "external_tool_started",
        json!({
            "toolId": &tool.id,
            "qualifiedId": tool.qualified_id(),
            "frameworkId": framework_id_for_tool(&tool),
            "status": "running",
        }),
    )
    .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    {
        let mut store =
            lock_run_store(run_store).map_err(|error| anyhow::anyhow!(error.to_string()))?;
        store
            .insert_run(run.clone(), vec![started_event])
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    }
    let result = match execute_tool_with_workflows(
        &tool,
        &servers,
        workflow_store,
        tool_registry,
        request.arguments,
    ) {
        Ok(result) => result,
        Err(error) => {
            run["status"] = json!("failed");
            run["durationMs"] =
                json!(started_at.elapsed().as_millis().min(u64::MAX as u128) as u64);
            run["error"] = json!({
                "code": "tool_execution_failed",
                "message": truncate_diagnostic(error.to_string(), 512),
            });
            let failed = RunEventDraft::new(
                "external_tool_failed",
                json!({
                    "toolId": &tool.id,
                    "qualifiedId": tool.qualified_id(),
                    "status": "failed",
                    "durationMs": run["durationMs"],
                    "error": { "code": "tool_execution_failed" },
                }),
            )
            .map_err(|event_error| anyhow::anyhow!(event_error.to_string()))?;
            let mut store = lock_run_store(run_store)
                .map_err(|store_error| anyhow::anyhow!(store_error.to_string()))?;
            store
                .transition_run(run, failed)
                .map_err(|store_error| anyhow::anyhow!(store_error.to_string()))?;
            return workflow_runtime_error_response(error);
        }
    };
    let duration_ms = started_at.elapsed().as_millis().min(u64::MAX as u128) as u64;
    run["status"] = json!("succeeded");
    run["durationMs"] = json!(duration_ms);
    run["outputSummary"] = json!({
        "jsonBytes": serde_json::to_vec(&result).map(|bytes| bytes.len()).unwrap_or_default(),
        "diagnostics": result.pointer("/_loomExecution/diagnostics").cloned(),
        "events": result.pointer("/_loomExecution/events").cloned(),
    });
    let completed = RunEventDraft::new(
        "external_tool_completed",
        json!({
            "toolId": &tool.id,
            "qualifiedId": tool.qualified_id(),
            "status": "succeeded",
            "durationMs": duration_ms,
            "diagnostics": result.pointer("/_loomExecution/diagnostics").cloned(),
        }),
    )
    .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    {
        let mut store =
            lock_run_store(run_store).map_err(|error| anyhow::anyhow!(error.to_string()))?;
        store
            .transition_run(run, completed)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    }

    Ok((
        200,
        serde_json::to_string(&json!({
            "toolId": tool_id,
            "executionId": run_id,
            "status": "succeeded",
            "result": result,
        }))?,
    ))
}
