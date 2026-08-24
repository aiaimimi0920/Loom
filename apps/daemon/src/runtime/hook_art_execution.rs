// Hook capabilities, protocol responses, Art preparation, execution, and cancellation.
fn hook_protocol_capabilities(
    tool_registry: &ToolRegistry,
    workflow_store: &WorkflowStore,
    surface: SurfaceHostCapabilities,
    enabled_only: bool,
) -> HookCapabilities {
    let tools = tool_registry.list_tools().unwrap_or_default();
    let art_definitions = tools
        .iter()
        .filter(|tool| !enabled_only || tool.enabled)
        .map(|tool| hook_protocol_art_capability(tool, &tools, workflow_store))
        .collect();
    HookCapabilities {
        art_definitions,
        surface,
        operations: loom_protocol::HOOK_REQUEST_METHODS
            .iter()
            .map(|method| (*method).to_owned())
            .collect(),
    }
}

fn hook_protocol_art_capability(
    tool: &ToolDefinition,
    tools: &[ToolDefinition],
    workflow_store: &WorkflowStore,
) -> HookArtCapability {
    let parameters = hook_capability_parameters(tool, tools, workflow_store);
    let mut metadata = tool.metadata.clone().unwrap_or_else(|| json!({}));
    if workflow_preview_tool(tool, tools, workflow_store).is_some_and(tool_supports_shader_preview)
    {
        if !metadata.is_object() {
            metadata = json!({});
        }
        let object = metadata.as_object_mut().expect("metadata normalized");
        let capabilities = object
            .entry("capabilities".to_owned())
            .or_insert_with(|| json!({}));
        if !capabilities.is_object() {
            *capabilities = json!({});
        }
        let capabilities = capabilities
            .as_object_mut()
            .expect("capabilities normalized");
        capabilities.insert("preview".to_owned(), json!("shader"));
        capabilities.insert("requiresFormalExecution".to_owned(), json!(true));
        let image_inputs = hook_image_input_names(tool);
        if let Some(input) = image_inputs.first() {
            capabilities.insert("shaderInput".to_owned(), json!(input));
        }
        if let Some(reference) = image_inputs.get(1) {
            capabilities.insert("shaderReferenceInput".to_owned(), json!(reference));
        }
    }
    HookArtCapability {
        id: tool.qualified_id(),
        label: tool.name.clone(),
        description: tool.description.clone(),
        enabled: tool.enabled,
        auto_process: metadata
            .get("autoProcess")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        supported_transports: vec![
            HookTransportMode::SharedMemory,
            HookTransportMode::Websocket,
        ],
        parameters,
        inputs: tool.inputs.clone(),
        outputs: tool.outputs.clone(),
        execution: serde_json::to_value(&tool.execution).unwrap_or_default(),
        defaults: tool_defaults_json(tool),
        default_visibility: metadata
            .get("defaultVisibility")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default(),
        metadata,
    }
}

fn hook_protocol_success(request_id: &str, data: Value) -> HookBridgeWebSocketTextResult {
    HookBridgeWebSocketTextResult::response(hook_protocol_response_json(
        request_id,
        HookRequestStatus::Succeeded,
        data,
        None,
    ))
}

fn hook_protocol_failure(
    request_id: &str,
    code: &str,
    error: impl std::fmt::Display,
) -> HookBridgeWebSocketTextResult {
    HookBridgeWebSocketTextResult::response(hook_protocol_failure_json(request_id, code, error))
}

fn hook_protocol_failure_json(
    request_id: &str,
    code: &str,
    error: impl std::fmt::Display,
) -> String {
    hook_protocol_response_json(
        request_id,
        HookRequestStatus::Failed,
        Value::Null,
        Some(loom_protocol::SurfaceExecutionError {
            code: code.to_owned(),
            message: error.to_string(),
            detail: None,
        }),
    )
}

fn hook_protocol_response_json(
    request_id: &str,
    status: HookRequestStatus,
    data: Value,
    error: Option<loom_protocol::SurfaceExecutionError>,
) -> String {
    serde_json::to_string(&HookResponse {
        protocol_version: loom_protocol::HOOK_PROTOCOL_VERSION.to_owned(),
        request_id: request_id.to_owned(),
        status,
        data,
        error,
    })
    .unwrap_or_else(|serialize_error| {
        json!({
            "protocolVersion": loom_protocol::HOOK_PROTOCOL_VERSION,
            "requestId": request_id,
            "status": "failed",
            "data": null,
            "error": {
                "code": "serialization_failed",
                "message": serialize_error.to_string(),
            }
        })
        .to_string()
    })
}

#[allow(clippy::too_many_arguments)]
fn handle_hook_art_execute(
    request: HookArtExecuteRequest,
    mcp_servers: &SharedMcpServerStore,
    tool_registry: &ToolRegistry,
    workflow_store: &WorkflowStore,
    shared_images: &SharedImageStoreHandle,
    framework_registry: &FrameworkRegistry,
    control_plane_root: &Path,
    run_store: &SharedRunStore,
    emit_intermediate: &mut dyn FnMut(String),
) -> HookBridgeWebSocketTextResult {
    if request.protocol_version != loom_protocol::HOOK_PROTOCOL_VERSION {
        return hook_protocol_failure(
            &request.request_id,
            "unsupported_protocol",
            format!("unsupported Hook protocol `{}`", request.protocol_version),
        );
    }
    if !loom_protocol::is_safe_hook_identifier(&request.request_id)
        || !loom_protocol::is_safe_hook_identifier(&request.node_id)
        || request.art_id.trim().is_empty()
    {
        return hook_protocol_failure(
            &request.request_id,
            "invalid_identity",
            "requestId, nodeId, and artId must be valid Hook identities",
        );
    }
    if request.output_transports.is_empty() {
        return hook_protocol_failure(
            &request.request_id,
            "invalid_output_transport",
            "outputTransports must contain at least one supported transport",
        );
    }
    let cancellation = match reserve_hook_art_request(&request, shared_images) {
        HookArtReservation::Execute(cancellation) => cancellation,
        HookArtReservation::Replay(response) | HookArtReservation::Reject(response) => {
            return HookBridgeWebSocketTextResult::response(response)
        }
    };

    let ack = HookArtAck {
        protocol_version: loom_protocol::HOOK_PROTOCOL_VERSION.to_owned(),
        request_id: request.request_id.clone(),
        node_id: request.node_id.clone(),
        generation: request.generation,
        accepted: true,
        status: HookRequestStatus::Running,
        error: None,
    };
    emit_intermediate(hook_protocol_event_json(
        loom_protocol::HOOK_EVENT_ART_ACK,
        &ack,
    ));
    let progress = HookArtProgress {
        protocol_version: loom_protocol::HOOK_PROTOCOL_VERSION.to_owned(),
        request_id: request.request_id.clone(),
        node_id: request.node_id.clone(),
        generation: request.generation,
        value: Some(0.0),
        stage: Some("running".to_owned()),
        message_key: None,
    };
    emit_intermediate(hook_protocol_event_json(
        loom_protocol::HOOK_EVENT_ART_PROGRESS,
        &progress,
    ));

    let started = Instant::now();
    let result = execute_hook_art_request(
        &request,
        cancellation.as_ref(),
        mcp_servers,
        tool_registry,
        workflow_store,
        shared_images,
        framework_registry,
        control_plane_root,
        emit_intermediate,
    );
    let (terminal_response, terminal_status) = match result {
        Ok((outputs, candidates)) => {
            let Some(result_revision) = next_hook_art_result_revision(
                &request.request_id,
                &request.node_id,
                request.generation,
                request.device_id.as_deref(),
            ) else {
                let response = hook_protocol_failure_json(
                    &request.request_id,
                    "cancelled",
                    "Art result was discarded after cancellation or replacement",
                );
                finish_hook_art_request(
                    &request.request_id,
                    &request.node_id,
                    request.generation,
                    request.device_id.as_deref(),
                    hook_art_request_fingerprint(&request),
                    HookRequestStatus::Cancelled,
                    response.clone(),
                    shared_images,
                );
                return HookBridgeWebSocketTextResult::response(response);
            };
            let commit = HookArtResultCommit {
                protocol_version: loom_protocol::HOOK_PROTOCOL_VERSION.to_owned(),
                request_id: request.request_id.clone(),
                node_id: request.node_id.clone(),
                generation: request.generation,
                result_revision,
                outputs,
                candidates,
            };
            emit_intermediate(hook_protocol_event_json(
                loom_protocol::HOOK_EVENT_ART_RESULT,
                &commit,
            ));
            let run_id =
                record_hook_art_run(run_store, tool_registry, &request, started, true, None);
            let mut data = serde_json::to_value(commit).unwrap_or_default();
            if let Some(object) = data.as_object_mut() {
                object.insert("executionId".to_owned(), json!(run_id));
            }
            let response = hook_protocol_response_json(
                &request.request_id,
                HookRequestStatus::Succeeded,
                data,
                None,
            );
            (response, HookRequestStatus::Succeeded)
        }
        Err(error) => {
            let cancelled = cancellation.load(Ordering::Acquire)
                || matches!(error, WorkflowRuntimeError::Cancelled);
            let code = if cancelled {
                "cancelled"
            } else {
                "art_execution_failed"
            };
            let failure = HookArtFailure {
                protocol_version: loom_protocol::HOOK_PROTOCOL_VERSION.to_owned(),
                request_id: request.request_id.clone(),
                node_id: request.node_id.clone(),
                generation: request.generation,
                error: loom_protocol::SurfaceExecutionError {
                    code: code.to_owned(),
                    message: error.to_string(),
                    detail: None,
                },
                last_successful_result_revision: hook_art_requests().lock().ok().and_then(
                    |state| {
                        state
                            .result_revision_by_node
                            .get(&HookArtNodeScope::new(
                                request.device_id.as_deref(),
                                &request.node_id,
                            ))
                            .copied()
                    },
                ),
            };
            let status = if cancelled {
                HookRequestStatus::Cancelled
            } else {
                HookRequestStatus::Failed
            };
            emit_intermediate(hook_protocol_event_json(
                loom_protocol::HOOK_EVENT_ART_FAILURE,
                &failure,
            ));
            let run_id = record_hook_art_run(
                run_store,
                tool_registry,
                &request,
                started,
                false,
                Some(&failure.error),
            );
            let mut data = serde_json::to_value(&failure).unwrap_or_default();
            if let Some(object) = data.as_object_mut() {
                object.insert("executionId".to_owned(), json!(run_id));
            }
            let response = hook_protocol_response_json(
                &request.request_id,
                status.clone(),
                data,
                Some(failure.error.clone()),
            );
            (response, status)
        }
    };
    finish_hook_art_request(
        &request.request_id,
        &request.node_id,
        request.generation,
        request.device_id.as_deref(),
        hook_art_request_fingerprint(&request),
        terminal_status,
        terminal_response.clone(),
        shared_images,
    );
    HookBridgeWebSocketTextResult::response(terminal_response)
}

#[allow(clippy::too_many_arguments)]
fn execute_hook_art_request(
    request: &HookArtExecuteRequest,
    cancellation: &AtomicBool,
    mcp_servers: &SharedMcpServerStore,
    tool_registry: &ToolRegistry,
    workflow_store: &WorkflowStore,
    shared_images: &SharedImageStoreHandle,
    framework_registry: &FrameworkRegistry,
    control_plane_root: &Path,
    emit_intermediate: &mut dyn FnMut(String),
) -> std::result::Result<(BTreeMap<String, HookArtPortValue>, Option<Value>), WorkflowRuntimeError>
{
    let tool = tool_registry
        .get_tool(&request.art_id)?
        .or_else(|| {
            tool_registry
                .list_tools()
                .ok()?
                .into_iter()
                .find(|tool| tool.qualified_id() == request.art_id)
        })
        .ok_or_else(|| ToolRegistryError::ExecutionRejected {
            id: request.art_id.clone(),
        })?;
    let tool = resolve_registered_tool_package(
        &tool,
        tool_registry,
        framework_registry,
        control_plane_root,
    )
    .map_err(|error| ToolRegistryError::InvalidToolDefinition {
        id: request.art_id.clone(),
        reason: format!("Art package integrity verification failed: {error}"),
    })?;
    let servers = mcp_servers
        .lock()
        .map_err(|_| ToolRegistryError::ExecutionRejected {
            id: request.art_id.clone(),
        })?
        .values()
        .cloned()
        .collect::<Vec<_>>();
    let inputs =
        materialize_hook_art_inputs(&request.inputs, shared_images).map_err(|message| {
            ToolRegistryError::InvalidToolDefinition {
                id: request.art_id.clone(),
                reason: message,
            }
        })?;
    let timeout = request
        .deadline_at_ms
        .map(|deadline| deadline.saturating_sub(unix_time_millis()))
        .map(Duration::from_millis)
        .unwrap_or_else(|| Duration::from_secs(120))
        .max(Duration::from_millis(1));
    let mut arguments = inputs.as_object().cloned().unwrap_or_default();
    for (name, value) in &request.parameters {
        if arguments.insert(name.clone(), value.clone()).is_some() {
            return Err(ToolRegistryError::InvalidToolDefinition {
                id: request.art_id.clone(),
                reason: format!("Hook input and parameter names collide at `{name}`"),
            }
            .into());
        }
    }
    arguments.insert(
        "disabledParams".to_owned(),
        serde_json::to_value(&request.disabled_parameters).unwrap_or_else(|_| json!([])),
    );
    let arguments = Value::Object(arguments);
    let result = execute_tool_with_workflows_and_preview_timeout_and_cancellation(
        &tool,
        &servers,
        workflow_store,
        tool_registry,
        arguments,
        timeout,
        cancellation,
        |preview| {
            if !hook_art_request_is_current(
                &request.request_id,
                &request.node_id,
                request.generation,
                request.device_id.as_deref(),
            ) {
                return;
            }
            let Some(value) = hook_art_primary_output_value(
                &preview,
                shared_images,
                request
                    .output_transports
                    .contains(&HookTransportMode::SharedMemory),
            ) else {
                return;
            };
            let handles = hook_art_port_resource_handles(&value);
            if !register_hook_art_resource_handles(request, &handles, false) {
                release_shared_image_handles(shared_images, handles);
                return;
            }
            let Some(preview_revision) = next_hook_art_preview_revision(
                &request.request_id,
                &request.node_id,
                request.generation,
                request.device_id.as_deref(),
            ) else {
                return;
            };
            let commit = HookArtPreviewCommit {
                protocol_version: loom_protocol::HOOK_PROTOCOL_VERSION.to_owned(),
                request_id: request.request_id.clone(),
                node_id: request.node_id.clone(),
                generation: request.generation,
                preview_revision,
                port_id: hook_art_primary_output_id(&tool),
                value,
            };
            emit_intermediate(hook_protocol_event_json(
                loom_protocol::HOOK_EVENT_ART_PREVIEW,
                &commit,
            ));
        },
    )?;
    if cancellation.load(Ordering::Acquire) {
        return Err(WorkflowRuntimeError::Cancelled);
    }
    let candidates = result.pointer("/loomMetadata/candidates").cloned();
    let outputs = hook_art_result_outputs(
        &tool,
        &result,
        shared_images,
        request
            .output_transports
            .contains(&HookTransportMode::SharedMemory),
    );
    let handles = hook_art_output_resource_handles(&outputs);
    if !register_hook_art_resource_handles(request, &handles, true) {
        release_shared_image_handles(shared_images, handles);
        return Err(WorkflowRuntimeError::Cancelled);
    }
    Ok((outputs, candidates))
}
