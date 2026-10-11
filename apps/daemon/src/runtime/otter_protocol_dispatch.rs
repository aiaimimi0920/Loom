// Otter subscription guards, protocol results, and request dispatch.
struct OtterBridgeSubscriptionGuard {
    id: usize,
    subscribers: Arc<Mutex<Vec<OtterBridgeSubscriber>>>,
}

impl Drop for OtterBridgeSubscriptionGuard {
    fn drop(&mut self) {
        if let Ok(mut subscribers) = self.subscribers.lock() {
            subscribers.retain(|subscriber| subscriber.id != self.id);
        }
    }
}

struct ConnectedClientGuard {
    connected_clients: Arc<AtomicUsize>,
}

struct ExtensionClientGuard {
    extension_clients: Arc<AtomicUsize>,
    ocr_text_clients: Arc<AtomicUsize>,
    has_ocr_text: bool,
}

impl Drop for ExtensionClientGuard {
    fn drop(&mut self) {
        decrement_client_count(&self.extension_clients);
        if self.has_ocr_text { decrement_client_count(&self.ocr_text_clients); }
    }
}

fn track_extension_client(
    guard: &mut Option<ExtensionClientGuard>,
    state: &ExtensionConnectionState,
    extension_clients: &Arc<AtomicUsize>,
    ocr_text_clients: &Arc<AtomicUsize>,
) {
    if state.extension_session_id.is_none() {
        *guard = None;
        return;
    }
    if guard.is_none() && state.extension_session_id.is_some() {
        extension_clients.fetch_add(1, Ordering::SeqCst);
        *guard = Some(ExtensionClientGuard {
            extension_clients: Arc::clone(extension_clients),
            ocr_text_clients: Arc::clone(ocr_text_clients),
            has_ocr_text: false,
        });
    }
    if let Some(guard) = guard {
        let supported = state.has_feature(loom_protocol::EXTENSION_FEATURE_OCR_TEXT);
        if supported != guard.has_ocr_text {
            if supported { ocr_text_clients.fetch_add(1, Ordering::SeqCst); }
            else { decrement_client_count(ocr_text_clients); }
            guard.has_ocr_text = supported;
        }
    }
}

impl Drop for ConnectedClientGuard {
    fn drop(&mut self) {
        decrement_client_count(&self.connected_clients);
    }
}

struct OtterBridgeWebSocketTextResult {
    response: String,
    broadcasts: Vec<String>,
    subscription_channels: Option<Vec<String>>,
}

impl OtterBridgeWebSocketTextResult {
    fn response(response: String) -> Self {
        Self {
            response,
            broadcasts: Vec::new(),
            subscription_channels: None,
        }
    }
}

#[cfg(test)]
fn handle_otter_bridge_websocket_text(
    text: &str,
    mcp_servers: &SharedMcpServerStore,
    tool_registry: &ToolRegistry,
    workflow_store: &WorkflowStore,
    settings: &SharedLoomSettingsStore,
    shared_images: &SharedImageStoreHandle,
    framework_registry: &FrameworkRegistry,
    control_plane_root: &Path,
    workflow_root: &Path,
    run_store: &SharedRunStore,
) -> OtterBridgeWebSocketTextResult {
    handle_otter_bridge_websocket_text_with_intermediate(
        text,
        mcp_servers,
        tool_registry,
        workflow_store,
        settings,
        shared_images,
        framework_registry,
        control_plane_root,
        workflow_root,
        run_store,
        None,
        &mut |_| {},
    )
}

#[allow(clippy::too_many_arguments)]
fn handle_otter_bridge_websocket_text_with_intermediate(
    text: &str,
    mcp_servers: &SharedMcpServerStore,
    tool_registry: &ToolRegistry,
    workflow_store: &WorkflowStore,
    settings: &SharedLoomSettingsStore,
    shared_images: &SharedImageStoreHandle,
    framework_registry: &FrameworkRegistry,
    control_plane_root: &Path,
    workflow_root: &Path,
    run_store: &SharedRunStore,
    _surface_actions: Option<&SharedSurfaceActionExecutor>,
    emit_intermediate: &mut dyn FnMut(String),
) -> OtterBridgeWebSocketTextResult {
    let request = match serde_json::from_str::<OtterRequest>(text) {
        Ok(request) => request,
        Err(error) => {
            return OtterBridgeWebSocketTextResult::response(otter_protocol_failure_json(
                "invalid-request",
                "invalid_otter_request",
                error,
            ))
        }
    };
    match request {
        OtterRequest::ArtExecute(request) => handle_otter_art_execute(
            request,
            mcp_servers,
            tool_registry,
            workflow_store,
            shared_images,
            framework_registry,
            control_plane_root,
            run_store,
            emit_intermediate,
        ),
        OtterRequest::ArtCancel(request) => OtterBridgeWebSocketTextResult::response(
            cancel_otter_art_request(&request, shared_images),
        ),
        OtterRequest::ArtResourcesRelease(request) => OtterBridgeWebSocketTextResult::response(
            release_otter_art_resources(&request, shared_images),
        ),
        request => handle_otter_protocol_request(
            request,
            tool_registry,
            workflow_store,
            settings,
            workflow_root,
        ),
    }
}

fn handle_otter_protocol_request(
    request: OtterRequest,
    tool_registry: &ToolRegistry,
    workflow_store: &WorkflowStore,
    settings: &SharedLoomSettingsStore,
    workflow_root: &Path,
) -> OtterBridgeWebSocketTextResult {
    match request {
        OtterRequest::Handshake(request) => {
            if let Err(error) = loom_protocol::validate_otter_handshake(&request) {
                return otter_protocol_failure("handshake", "protocol_negotiation_failed", error);
            }
            let transport = if request
                .transports
                .contains(&OtterTransportMode::SharedMemory)
            {
                OtterTransportMode::SharedMemory
            } else if request.transports.contains(&OtterTransportMode::Websocket) {
                OtterTransportMode::Websocket
            } else {
                OtterTransportMode::CloudflareRelay
            };
            let capabilities = otter_protocol_capabilities(
                tool_registry,
                workflow_store,
                request
                    .surface
                    .unwrap_or_else(default_declarative_surface_host_capabilities),
                true,
            );
            let response = OtterHandshakeResponse {
                protocol_version: loom_protocol::HOOK_PROTOCOL_VERSION.to_owned(),
                server_name: "loom-daemon".to_owned(),
                server_version: env!("CARGO_PKG_VERSION").to_owned(),
                session_id: format!("hook:{}", Uuid::new_v4()),
                transport,
                capabilities,
            };
            OtterBridgeWebSocketTextResult::response(
                serde_json::to_string(&response).unwrap_or_else(|error| {
                    otter_protocol_failure_json("handshake", "serialization_failed", error)
                }),
            )
        }
        OtterRequest::CapabilitiesList(request) => {
            let capabilities = otter_protocol_capabilities(
                tool_registry,
                workflow_store,
                default_declarative_surface_host_capabilities(),
                request.enabled_only,
            );
            otter_protocol_success(&request.request_id, serde_json::json!(capabilities))
        }
        OtterRequest::Subscribe(request) => {
            let invalid = request
                .events
                .iter()
                .find(|event| !loom_protocol::otter_subscription_event_supported(event));
            if let Some(invalid) = invalid {
                return otter_protocol_failure(
                    &request.request_id,
                    "unsupported_event",
                    format!("unsupported Otter event `{invalid}`"),
                );
            }
            let mut result =
                otter_protocol_success(&request.request_id, json!({ "events": request.events }));
            result.subscription_channels = Some(
                serde_json::from_str::<OtterResponse>(&result.response)
                    .ok()
                    .and_then(|response| response.data.get("events").cloned())
                    .and_then(|events| serde_json::from_value(events).ok())
                    .unwrap_or_default(),
            );
            result
        }
        OtterRequest::WorkflowSync(request) => {
            if is_otter_live_workflow_id(&request.workflow_id) {
                if let Err(error) = otter_session_document_revision(&request.snapshot) {
                    return otter_protocol_failure(
                        &request.request_id,
                        "otter_session_schema_unsupported",
                        format!("invalid Otter live workflow revision metadata: {error}"),
                    );
                }
            }
            match sync_workflow(workflow_root, &request.workflow_id, &request.snapshot) {
                Ok(event) => {
                    if is_otter_live_workflow_id(&request.workflow_id) {
                        if let Err(error) = store_otter_live_workflow_snapshot(
                            &otter_session_path(),
                            &request.workflow_id,
                            &request.snapshot,
                        ) {
                            return otter_protocol_failure(
                                &request.request_id,
                                "otter_live_snapshot_store_failed",
                                error,
                            );
                        }
                    }
                    OtterBridgeWebSocketTextResult {
                        response: otter_protocol_response_json(
                            &request.request_id,
                            OtterRequestStatus::Succeeded,
                            json!({ "workflowId": request.workflow_id }),
                            None,
                        ),
                        broadcasts: serde_json::to_string(&event).into_iter().collect(),
                        subscription_channels: None,
                    }
                }
                Err(error) => {
                    otter_protocol_failure(&request.request_id, "workflow_sync_failed", error)
                }
            }
        }
        OtterRequest::WorkflowNodeUpdate(request) => {
            if is_otter_live_workflow_id(&request.workflow_id) {
                let mut patch = OtterCanvasPersistPatch::default();
                patch
                    .param_updates
                    .push((request.parameter_id.clone(), request.value.clone()));
                if let Err(error) = persist_otter_canvas_live_node_patch(&request.node_id, &patch) {
                    return otter_protocol_failure(
                        &request.request_id,
                        error.code(),
                        error.message(),
                    );
                }
            }
            match update_workflow_node(
                workflow_root,
                &request.workflow_id,
                &request.node_id,
                &request.parameter_id,
                request.value.clone(),
            ) {
                Ok(event) => OtterBridgeWebSocketTextResult {
                    response: otter_protocol_response_json(
                        &request.request_id,
                        OtterRequestStatus::Succeeded,
                        json!({
                            "workflowId": request.workflow_id,
                            "nodeId": request.node_id,
                            "parameterId": request.parameter_id,
                        }),
                        None,
                    ),
                    broadcasts: serde_json::to_string(&event).into_iter().collect(),
                    subscription_channels: None,
                },
                Err(error) => {
                    otter_protocol_failure(&request.request_id, "workflow_update_failed", error)
                }
            }
        }
        OtterRequest::WorkflowInstantiate(request) => {
            match instantiate_workflow(
                workflow_root,
                request.nodes,
                request.edges,
                &request.mode,
                request.workflow_id,
            ) {
                Ok(event) => OtterBridgeWebSocketTextResult {
                    response: otter_protocol_response_json(
                        &request.request_id,
                        OtterRequestStatus::Succeeded,
                        json!({}),
                        None,
                    ),
                    broadcasts: serde_json::to_string(&event).into_iter().collect(),
                    subscription_channels: None,
                },
                Err(error) => {
                    otter_protocol_failure(&request.request_id, "workflow_instantiate_failed", error)
                }
            }
        }
        OtterRequest::SettingsGet(request) => match settings.lock() {
            Ok(store) => otter_protocol_success(
                &request.request_id,
                otter_settings_protocol_value(&store.settings),
            ),
            Err(_) => otter_protocol_failure(
                &request.request_id,
                "settings_unavailable",
                "lock Loom settings",
            ),
        },
        OtterRequest::ArtExecute(_)
        | OtterRequest::ArtCancel(_)
        | OtterRequest::ArtResourcesRelease(_) => otter_protocol_failure(
            "hook.art",
            "internal_routing_error",
            "Art requests must be handled by the execution-aware dispatcher",
        ),
    }
}
