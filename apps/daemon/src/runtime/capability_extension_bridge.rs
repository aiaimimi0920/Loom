// Stateful loom.extension.v1 dispatch layered on an authenticated Hook WebSocket.
#[derive(Default)]
struct ExtensionConnectionState {
    hook_session_id: Option<String>,
    extension_session_id: Option<String>,
    negotiated_features: HashSet<String>,
    consumed_gestures: HashSet<String>,
}

struct ExtensionBridgeTextResult {
    response: String,
    subscribe_to_snapshots: bool,
}

impl ExtensionConnectionState {
    fn record_hook_handshake(&mut self, response: &str) {
        let Ok(response) = serde_json::from_str::<HookHandshakeResponse>(response) else {
            return;
        };
        self.hook_session_id = Some(response.session_id);
        self.extension_session_id = None;
        self.negotiated_features.clear();
        self.consumed_gestures.clear();
    }

    fn extension_session_matches(&self, session_id: &str) -> bool {
        self.extension_session_id.as_deref() == Some(session_id)
    }

    fn has_feature(&self, feature: &str) -> bool {
        self.negotiated_features.contains(feature)
    }

    fn client_gesture_is_available(&self, token: &str) -> bool {
        self.consumed_gestures.len() < 1_024 && !self.consumed_gestures.contains(token)
    }

    fn record_client_gesture(&mut self, token: &str) {
        self.consumed_gestures.insert(token.to_owned());
    }
}

fn is_extension_bridge_request(text: &str) -> bool {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|value| value.get("method").and_then(Value::as_str).map(str::to_owned))
        .is_some_and(|method| method.starts_with("loom.extension."))
}

fn handle_extension_bridge_text(
    text: &str,
    state: &mut ExtensionConnectionState,
    runtime: &SharedCapabilityRuntime,
    resources: &SharedCapabilityResourceBroker,
    surface_resources: &SharedSurfaceResourceStore,
) -> ExtensionBridgeTextResult {
    let request = match serde_json::from_str::<ExtensionBridgeRequest>(text) {
        Ok(request) => request,
        Err(error) => {
            return extension_bridge_failure(
                "invalid-request",
                "invalid_extension_request",
                error.to_string(),
                false,
            )
        }
    };
    match request {
        ExtensionBridgeRequest::Handshake(request) => {
            handle_extension_handshake(request, state, runtime)
        }
        ExtensionBridgeRequest::SnapshotGet(request) => {
            if !state.extension_session_matches(&request.session_id) {
                return extension_bridge_failure(
                    &request.request_id,
                    "stale_extension_session",
                    "extension session is stale or disconnected",
                    true,
                );
            }
            if !state.has_feature(loom_protocol::EXTENSION_FEATURE_SNAPSHOT) {
                return extension_feature_failure(&request.request_id, "contribution.snapshot");
            }
            match runtime.contribution_snapshot() {
                Ok(snapshot) => extension_bridge_success(
                    &request.request_id,
                    json!({ "snapshot": snapshot }),
                    false,
                ),
                Err(error) => extension_runtime_failure(&request.request_id, error),
            }
        }
        ExtensionBridgeRequest::CommandInvoke(request) => {
            handle_extension_invocation(request, state, runtime, resources, surface_resources)
        }
    }
}

fn handle_extension_handshake(
    request: ExtensionHandshakeRequest,
    state: &mut ExtensionConnectionState,
    runtime: &SharedCapabilityRuntime,
) -> ExtensionBridgeTextResult {
    if state.hook_session_id.as_deref() != Some(request.hook_session_id.as_str()) {
        return extension_bridge_failure(
            &request.request_id,
            "hook_session_required",
            "the same WebSocket must complete Hook authentication first",
            false,
        );
    }
    let features = match negotiate_extension_features(&request) {
        Ok(features) => features,
        Err(error) => {
            return extension_bridge_failure(
                &request.request_id,
                "extension_negotiation_failed",
                error.to_string(),
                false,
            )
        }
    };
    let session_id = format!("extension:{}", Uuid::new_v4());
    let subscribe_to_snapshots = features
        .iter()
        .any(|feature| feature == loom_protocol::EXTENSION_FEATURE_SNAPSHOT);
    let snapshot = if subscribe_to_snapshots {
        match runtime.contribution_snapshot() {
            Ok(snapshot) => Some(snapshot),
            Err(error) => return extension_runtime_failure(&request.request_id, error),
        }
    } else {
        None
    };
    state.extension_session_id = Some(session_id.clone());
    state.negotiated_features = features.iter().cloned().collect();
    state.consumed_gestures.clear();
    runtime.invalidate_user_gestures();
    let data = match snapshot {
        Some(snapshot) => json!({
            "sessionId": session_id,
            "features": features,
            "snapshot": snapshot,
        }),
        None => json!({
            "sessionId": session_id,
            "features": features,
        }),
    };
    extension_bridge_success(
        &request.request_id,
        data,
        subscribe_to_snapshots,
    )
}

fn handle_extension_invocation(
    request: ExtensionCommandInvokeRequest,
    state: &mut ExtensionConnectionState,
    runtime: &SharedCapabilityRuntime,
    resources: &SharedCapabilityResourceBroker,
    surface_resources: &SharedSurfaceResourceStore,
) -> ExtensionBridgeTextResult {
    let request_id = request.invocation.request_id.clone();
    if !state.extension_session_matches(&request.session_id) {
        return extension_bridge_failure(
            &request_id,
            "stale_extension_session",
            "extension session is stale or disconnected",
            true,
        );
    }
    if !state.has_feature(loom_protocol::EXTENSION_FEATURE_COMMANDS) {
        return extension_feature_failure(&request_id, "command.invoke");
    }
    if let Err(error) = validate_extension_message(&ExtensionMessage::Invocation(
        request.invocation.clone(),
    )) {
        return extension_bridge_failure(
            &request_id,
            "invalid_extension_invocation",
            error.to_string(),
            false,
        );
    }
    let snapshot = match runtime.contribution_snapshot() {
        Ok(snapshot) => snapshot,
        Err(error) => return extension_runtime_failure(&request_id, error),
    };
    if request.invocation.snapshot_generation != snapshot.generation {
        return extension_result_failure(
            &request_id,
            CapabilityErrorCode::StaleGeneration,
            "extension contribution generation is stale",
        );
    }
    let owner = match runtime.command_owner(&request.invocation.command_id) {
        Ok(Some(owner)) if owner == request.invocation.plugin_id => owner,
        Ok(_) => {
            return extension_result_failure(
                &request_id,
                CapabilityErrorCode::PluginNotActive,
                "extension command owner is unavailable",
            )
        }
        Err(error) => return extension_runtime_failure(&request_id, error),
    };
    let scope_id = match runtime.plugin_scope(&owner) {
        Ok(Some(scope_id)) => scope_id,
        Ok(None) => return extension_result_failure(
            &request_id,
            CapabilityErrorCode::PluginNotActive,
            "extension command scope is unavailable",
        ),
        Err(error) => return extension_runtime_failure(&request_id, error),
    };
    let resource_lease = match resources.stage(
        surface_resources,
        &owner,
        &scope_id,
        &request_id,
        &request.invocation.resource_refs,
    ) {
        Ok(lease) => lease,
        Err(error) => return extension_resource_failure(&request_id, error),
    };
    let staged_resources = resource_lease.resources().to_vec();
    let gesture = match request.invocation.user_gesture_token.as_deref() {
        Some(token) if state.client_gesture_is_available(token) => {
            let gesture = match runtime.issue_user_gesture(
                &request.invocation.command_id,
                Some(loom_capability_runtime::UserGestureTarget {
                    unit_id: request.invocation.target.unit_id.clone(),
                    revision: request.invocation.target.revision,
                }),
            ) {
                Ok(gesture) => gesture,
                Err(error) => return extension_runtime_failure(&request_id, error),
            };
            state.record_client_gesture(token);
            Some(gesture)
        }
        Some(_) => {
            return extension_result_failure(
                &request_id,
                CapabilityErrorCode::PermissionDenied,
                "user gesture token is invalid or already consumed",
            )
        }
        None => None,
    };
    let output = runtime.invoke(CapabilityInvocation {
        request_id: request_id.clone(),
        command_id: request.invocation.command_id,
        input: request.invocation.input,
        target: Some(request.invocation.target),
        resource_refs: request.invocation.resource_refs,
        staged_resources,
        user_gesture_token: gesture,
        timeout: None,
    });
    match output {
        Ok(output) if output.plugin_id == owner => {
            let payload = output.payload.unwrap_or(Value::Null);
            let mut effects: Vec<loom_protocol::ExtensionEffect> = match payload.get("effects") {
                Some(effects) => match serde_json::from_value(effects.clone()) {
                    Ok(effects) => effects,
                    Err(_) => {
                        return extension_result_failure(
                            &request_id,
                            CapabilityErrorCode::InvalidInput,
                            "extension runtime returned invalid effects",
                        )
                    }
                },
                None => Vec::new(),
            };
            if !state.has_feature(loom_protocol::EXTENSION_FEATURE_NOTICES) {
                effects.retain(|effect: &loom_protocol::ExtensionEffect| {
                    effect.effect_type != loom_protocol::ExtensionEffectType::NoticeShow
                });
            }
            let result = ExtensionResult {
                protocol: EXTENSION_PROTOCOL.to_owned(),
                api_version: CAPABILITY_API_VERSION.to_owned(),
                request_id: request_id.clone(),
                status: ExtensionResultStatus::Succeeded,
                output: payload.get("output").cloned().unwrap_or(payload.clone()),
                effects,
                error: None,
            };
            if validate_extension_message(&ExtensionMessage::Result(result.clone())).is_err() {
                return extension_result_failure(
                    &request_id,
                    CapabilityErrorCode::InvalidInput,
                    "extension runtime returned invalid effects",
                );
            }
            extension_bridge_success(&request_id, serde_json::to_value(result).unwrap_or_default(), false)
        }
        Ok(_) => extension_result_failure(
            &request_id,
            CapabilityErrorCode::PluginNotActive,
            "extension command owner changed during invocation",
        ),
        Err(error) => extension_runtime_failure(&request_id, error),
    }
}

fn extension_resource_failure(
    request_id: &str,
    error: CapabilityResourceError,
) -> ExtensionBridgeTextResult {
    let (code, message) = match error {
        CapabilityResourceError::Invalid => (
            CapabilityErrorCode::InvalidInput,
            "extension resource reference is invalid",
        ),
        CapabilityResourceError::LeaseRejected => (
            CapabilityErrorCode::ResourceNotFound,
            "extension resource lease was rejected",
        ),
        CapabilityResourceError::Busy => (
            CapabilityErrorCode::Busy,
            "extension resource broker is busy",
        ),
        CapabilityResourceError::Io(_) | CapabilityResourceError::Json(_) => (
            CapabilityErrorCode::RuntimeFault,
            "extension resource broker is unavailable",
        ),
    };
    extension_result_failure(request_id, code, message)
}

fn extension_feature_failure(request_id: &str, feature: &str) -> ExtensionBridgeTextResult {
    extension_bridge_failure(
        request_id,
        "extension_feature_not_negotiated",
        format!("extension feature was not negotiated: {feature}"),
        false,
    )
}

fn extension_result_failure(
    request_id: &str,
    code: CapabilityErrorCode,
    message: &str,
) -> ExtensionBridgeTextResult {
    let result = ExtensionResult {
        protocol: EXTENSION_PROTOCOL.to_owned(),
        api_version: CAPABILITY_API_VERSION.to_owned(),
        request_id: request_id.to_owned(),
        status: ExtensionResultStatus::Failed,
        output: Value::Null,
        effects: Vec::new(),
        error: Some(ExtensionError { code, message: message.to_owned() }),
    };
    extension_bridge_success(request_id, serde_json::to_value(result).unwrap_or_default(), false)
}

fn extension_runtime_failure(
    request_id: &str,
    error: loom_capability_runtime::CapabilityHostError,
) -> ExtensionBridgeTextResult {
    use loom_capability_runtime::CapabilityHostError as Error;
    let (code, retryable) = match error {
        Error::NotFound(_) => ("extension_command_not_found", false),
        Error::InvalidPackage(_) | Error::Protocol(_) => ("extension_request_rejected", false),
        Error::Busy | Error::Timeout => ("extension_runtime_busy", true),
        Error::Unavailable(_) | Error::Io(_) | Error::Process(_) | Error::Json(_) => {
            ("extension_runtime_unavailable", true)
        }
    };
    extension_bridge_failure(request_id, code, "extension runtime request failed", retryable)
}

fn extension_bridge_success(
    request_id: &str,
    data: Value,
    subscribe_to_snapshots: bool,
) -> ExtensionBridgeTextResult {
    extension_bridge_response(request_id, ExtensionBridgeStatus::Succeeded, data, None, subscribe_to_snapshots)
}

fn extension_bridge_failure(
    request_id: &str,
    code: &str,
    message: impl Into<String>,
    retryable: bool,
) -> ExtensionBridgeTextResult {
    extension_bridge_response(
        request_id,
        ExtensionBridgeStatus::Failed,
        Value::Null,
        Some(ExtensionBridgeError { code: code.to_owned(), message: message.into(), retryable }),
        false,
    )
}

fn extension_bridge_response(
    request_id: &str,
    status: ExtensionBridgeStatus,
    data: Value,
    error: Option<ExtensionBridgeError>,
    subscribe_to_snapshots: bool,
) -> ExtensionBridgeTextResult {
    let response = ExtensionBridgeResponse {
        protocol: EXTENSION_PROTOCOL.to_owned(),
        api_version: CAPABILITY_API_VERSION.to_owned(),
        request_id: request_id.to_owned(),
        status,
        data,
        error,
    };
    ExtensionBridgeTextResult {
        response: serde_json::to_string(&response).unwrap_or_else(|_| "{}".to_owned()),
        subscribe_to_snapshots,
    }
}

fn extension_snapshot_event(snapshot: ContributionSnapshot) -> ExtensionSnapshotEvent {
    ExtensionSnapshotEvent {
        protocol: EXTENSION_PROTOCOL.to_owned(),
        api_version: CAPABILITY_API_VERSION.to_owned(),
        method: EXTENSION_EVENT_SNAPSHOT_UPDATED.to_owned(),
        params: ExtensionSnapshotEventParams { snapshot },
    }
}
