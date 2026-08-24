// Hook subscription guards, protocol results, and request dispatch.
struct HookBridgeSubscriptionGuard {
    id: usize,
    subscribers: Arc<Mutex<Vec<HookBridgeSubscriber>>>,
}

impl Drop for HookBridgeSubscriptionGuard {
    fn drop(&mut self) {
        if let Ok(mut subscribers) = self.subscribers.lock() {
            subscribers.retain(|subscriber| subscriber.id != self.id);
        }
    }
}

struct ConnectedClientGuard {
    connected_clients: Arc<AtomicUsize>,
}

impl Drop for ConnectedClientGuard {
    fn drop(&mut self) {
        self.connected_clients.fetch_sub(1, Ordering::SeqCst);
    }
}

struct HookBridgeWebSocketTextResult {
    response: String,
    broadcasts: Vec<String>,
    subscription_channels: Option<Vec<String>>,
}

impl HookBridgeWebSocketTextResult {
    fn response(response: String) -> Self {
        Self {
            response,
            broadcasts: Vec::new(),
            subscription_channels: None,
        }
    }
}

#[cfg(test)]
fn handle_hook_bridge_websocket_text(
    text: &str,
    mcp_servers: &SharedMcpServerStore,
    tool_registry: &ToolRegistry,
    workflow_store: &WorkflowStore,
    settings: &SharedLoomSettingsStore,
    shared_images: &SharedImageStoreHandle,
    ocr_provider: &OcrProviderHandle,
    framework_registry: &FrameworkRegistry,
    control_plane_root: &Path,
    workflow_root: &Path,
    run_store: &SharedRunStore,
) -> HookBridgeWebSocketTextResult {
    handle_hook_bridge_websocket_text_with_intermediate(
        text,
        mcp_servers,
        tool_registry,
        workflow_store,
        settings,
        shared_images,
        ocr_provider,
        framework_registry,
        control_plane_root,
        workflow_root,
        run_store,
        None,
        &mut |_| {},
    )
}

#[allow(clippy::too_many_arguments)]
fn handle_hook_bridge_websocket_text_with_intermediate(
    text: &str,
    mcp_servers: &SharedMcpServerStore,
    tool_registry: &ToolRegistry,
    workflow_store: &WorkflowStore,
    settings: &SharedLoomSettingsStore,
    shared_images: &SharedImageStoreHandle,
    ocr_provider: &OcrProviderHandle,
    framework_registry: &FrameworkRegistry,
    control_plane_root: &Path,
    workflow_root: &Path,
    run_store: &SharedRunStore,
    _surface_actions: Option<&SharedSurfaceActionExecutor>,
    emit_intermediate: &mut dyn FnMut(String),
) -> HookBridgeWebSocketTextResult {
    let request = match serde_json::from_str::<HookRequest>(text) {
        Ok(request) => request,
        Err(error) => {
            return HookBridgeWebSocketTextResult::response(hook_protocol_failure_json(
                "invalid-request",
                "invalid_hook_request",
                error,
            ))
        }
    };
    match request {
        HookRequest::ArtExecute(request) => handle_hook_art_execute(
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
        HookRequest::ArtCancel(request) => HookBridgeWebSocketTextResult::response(
            cancel_hook_art_request(&request, shared_images),
        ),
        HookRequest::ArtResourcesRelease(request) => HookBridgeWebSocketTextResult::response(
            release_hook_art_resources(&request, shared_images),
        ),
        request => handle_hook_protocol_request(
            request,
            tool_registry,
            workflow_store,
            settings,
            ocr_provider,
            workflow_root,
        ),
    }
}

fn handle_hook_protocol_request(
    request: HookRequest,
    tool_registry: &ToolRegistry,
    workflow_store: &WorkflowStore,
    settings: &SharedLoomSettingsStore,
    ocr_provider: &OcrProviderHandle,
    workflow_root: &Path,
) -> HookBridgeWebSocketTextResult {
    match request {
        HookRequest::Handshake(request) => {
            if let Err(error) = loom_protocol::validate_hook_handshake(&request) {
                return hook_protocol_failure("handshake", "protocol_negotiation_failed", error);
            }
            let transport = if request
                .transports
                .contains(&HookTransportMode::SharedMemory)
            {
                HookTransportMode::SharedMemory
            } else if request.transports.contains(&HookTransportMode::Websocket) {
                HookTransportMode::Websocket
            } else {
                HookTransportMode::CloudflareRelay
            };
            let capabilities = hook_protocol_capabilities(
                tool_registry,
                workflow_store,
                request
                    .surface
                    .unwrap_or_else(default_declarative_surface_host_capabilities),
                true,
            );
            let response = HookHandshakeResponse {
                protocol_version: loom_protocol::HOOK_PROTOCOL_VERSION.to_owned(),
                server_name: "loom-daemon".to_owned(),
                server_version: env!("CARGO_PKG_VERSION").to_owned(),
                session_id: format!("hook:{}", Uuid::new_v4()),
                transport,
                capabilities,
            };
            HookBridgeWebSocketTextResult::response(
                serde_json::to_string(&response).unwrap_or_else(|error| {
                    hook_protocol_failure_json("handshake", "serialization_failed", error)
                }),
            )
        }
        HookRequest::CapabilitiesList(request) => {
            let capabilities = hook_protocol_capabilities(
                tool_registry,
                workflow_store,
                default_declarative_surface_host_capabilities(),
                request.enabled_only,
            );
            hook_protocol_success(&request.request_id, serde_json::json!(capabilities))
        }
        HookRequest::Subscribe(request) => {
            let invalid = request
                .events
                .iter()
                .find(|event| !loom_protocol::hook_subscription_event_supported(event));
            if let Some(invalid) = invalid {
                return hook_protocol_failure(
                    &request.request_id,
                    "unsupported_event",
                    format!("unsupported Hook event `{invalid}`"),
                );
            }
            let mut result =
                hook_protocol_success(&request.request_id, json!({ "events": request.events }));
            result.subscription_channels = Some(
                serde_json::from_str::<HookResponse>(&result.response)
                    .ok()
                    .and_then(|response| response.data.get("events").cloned())
                    .and_then(|events| serde_json::from_value(events).ok())
                    .unwrap_or_default(),
            );
            result
        }
        HookRequest::WorkflowSync(request) => {
            if is_hook_live_workflow_id(&request.workflow_id) {
                if let Err(error) = hook_session_document_revision(&request.snapshot) {
                    return hook_protocol_failure(
                        &request.request_id,
                        "hook_session_schema_unsupported",
                        format!("invalid Hook live workflow revision metadata: {error}"),
                    );
                }
            }
            match sync_workflow(workflow_root, &request.workflow_id, &request.snapshot) {
                Ok(event) => {
                    if is_hook_live_workflow_id(&request.workflow_id) {
                        if let Err(error) = store_hook_live_workflow_snapshot(
                            &hook_session_path(),
                            &request.workflow_id,
                            &request.snapshot,
                        ) {
                            return hook_protocol_failure(
                                &request.request_id,
                                "hook_live_snapshot_store_failed",
                                error,
                            );
                        }
                    }
                    HookBridgeWebSocketTextResult {
                        response: hook_protocol_response_json(
                            &request.request_id,
                            HookRequestStatus::Succeeded,
                            json!({ "workflowId": request.workflow_id }),
                            None,
                        ),
                        broadcasts: serde_json::to_string(&event).into_iter().collect(),
                        subscription_channels: None,
                    }
                }
                Err(error) => {
                    hook_protocol_failure(&request.request_id, "workflow_sync_failed", error)
                }
            }
        }
        HookRequest::WorkflowNodeUpdate(request) => {
            if is_hook_live_workflow_id(&request.workflow_id) {
                let mut patch = HookCanvasPersistPatch::default();
                patch
                    .param_updates
                    .push((request.parameter_id.clone(), request.value.clone()));
                if let Err(error) = persist_hook_canvas_live_node_patch(&request.node_id, &patch) {
                    return hook_protocol_failure(
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
                Ok(event) => HookBridgeWebSocketTextResult {
                    response: hook_protocol_response_json(
                        &request.request_id,
                        HookRequestStatus::Succeeded,
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
                    hook_protocol_failure(&request.request_id, "workflow_update_failed", error)
                }
            }
        }
        HookRequest::WorkflowInstantiate(request) => {
            match instantiate_workflow(
                workflow_root,
                request.nodes,
                request.edges,
                &request.mode,
                request.workflow_id,
            ) {
                Ok(event) => HookBridgeWebSocketTextResult {
                    response: hook_protocol_response_json(
                        &request.request_id,
                        HookRequestStatus::Succeeded,
                        json!({}),
                        None,
                    ),
                    broadcasts: serde_json::to_string(&event).into_iter().collect(),
                    subscription_channels: None,
                },
                Err(error) => {
                    hook_protocol_failure(&request.request_id, "workflow_instantiate_failed", error)
                }
            }
        }
        HookRequest::SettingsGet(request) => match settings.lock() {
            Ok(store) => hook_protocol_success(
                &request.request_id,
                hook_settings_protocol_value(&store.settings),
            ),
            Err(_) => hook_protocol_failure(
                &request.request_id,
                "settings_unavailable",
                "lock Loom settings",
            ),
        },
        HookRequest::EnhancementsGet(request) => {
            let ocr = ocr_provider
                .lock()
                .map(|provider| provider.is_available())
                .unwrap_or(false);
            hook_protocol_success(
                &request.request_id,
                json!({ "ocr": ocr, "translation": true }),
            )
        }
        HookRequest::OcrExecute(request) => {
            match execute_hook_ocr(&request.image_base64, ocr_provider) {
                Ok(result) => hook_protocol_success(&request.request_id, result),
                Err(error) => hook_protocol_failure(&request.request_id, "ocr_failed", error),
            }
        }
        HookRequest::TranslationExecute(request) => {
            match translate_text_via_provider(&request.text, &request.target_language) {
                Ok(translated_text) => hook_protocol_success(
                    &request.request_id,
                    json!({
                        "translatedText": translated_text.unwrap_or(request.text),
                        "targetLanguage": request.target_language,
                    }),
                ),
                Err(error) => {
                    hook_protocol_failure(&request.request_id, "translation_failed", error)
                }
            }
        }
        HookRequest::ArtExecute(_)
        | HookRequest::ArtCancel(_)
        | HookRequest::ArtResourcesRelease(_) => hook_protocol_failure(
            "hook.art",
            "internal_routing_error",
            "Art requests must be handled by the execution-aware dispatcher",
        ),
    }
}
