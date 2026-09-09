// Authenticated loom.live.v1 HTTP control plane; binary media remains on its own WebSocket route.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LiveSessionCreateRequest {
    surface_instance_id: String,
    source_attachment_id: String,
    envelope: LiveControlEnvelope,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LiveViewerAttachRequest {
    surface_instance_id: String,
    attachment_id: String,
    envelope: LiveControlEnvelope,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LiveEnvelopeRequest {
    envelope: LiveControlEnvelope,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LiveInputForwardRequest {
    surface_instance_id: String,
    attachment_id: String,
    envelope: LiveControlEnvelope,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LiveObservationPublishRequest {
    surface_instance_id: String,
    attachment_id: String,
    envelope: LiveControlEnvelope,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LiveTriggerBindingRequest {
    binding_id: String,
    enabled: bool,
    target: LiveTriggerTarget,
    envelope: LiveControlEnvelope,
}

fn route_live_sessions(
    request: &ParsedHttpRequest,
    route_path: &str,
    live_sessions: &SharedLiveSessionStore,
    surface_instances: &SharedSurfaceInstanceStore,
    surface_actions: &SharedSurfaceActionExecutor,
    authenticated_device_id: Option<&str>,
) -> Option<Result<(u16, String)>> {
    let response = match (request.method.as_str(), route_path) {
        ("GET", "/v1/live/status") => live_status(live_sessions),
        ("GET", "/v1/live/sessions") => list_live_sessions(live_sessions),
        ("POST", "/v1/live/sessions") => create_live_session(
            &request.body,
            live_sessions,
            surface_instances,
            authenticated_device_id,
        ),
        ("GET", path) if live_session_suffix_id(path, "/events").is_some() => live_session_events(
            live_session_suffix_id(path, "/events").expect("checked live event path"),
            &request.path,
            live_sessions,
            authenticated_device_id,
        ),
        ("POST", path) if live_session_suffix_id(path, "/viewers").is_some() => attach_live_viewer(
            live_session_suffix_id(path, "/viewers").expect("checked live viewer path"),
            &request.body,
            live_sessions,
            surface_instances,
            authenticated_device_id,
        ),
        ("POST", path) if live_session_suffix_id(path, "/resume").is_some() => resume_live_session(
            live_session_suffix_id(path, "/resume").expect("checked live resume path"),
            &request.body,
            live_sessions,
            authenticated_device_id,
        ),
        ("POST", path) if live_session_suffix_id(path, "/control").is_some() => {
            change_live_controller(
                live_session_suffix_id(path, "/control").expect("checked live control path"),
                &request.body,
                live_sessions,
                surface_instances,
                authenticated_device_id,
            )
        }
        ("POST", path) if live_session_suffix_id(path, "/input").is_some() => forward_live_input(
            live_session_suffix_id(path, "/input").expect("checked live input path"),
            &request.body,
            live_sessions,
            surface_instances,
            authenticated_device_id,
        ),
        ("POST", path) if live_session_suffix_id(path, "/observations").is_some() => {
            publish_live_observation(
                live_session_suffix_id(path, "/observations")
                    .expect("checked live observation path"),
                &request.body,
                live_sessions,
                surface_instances,
                surface_actions,
                authenticated_device_id,
            )
        }
        ("POST", path) if live_session_suffix_id(path, "/triggers").is_some() => {
            configure_live_trigger(
                live_session_suffix_id(path, "/triggers").expect("checked live trigger path"),
                &request.body,
                live_sessions,
                surface_instances,
                authenticated_device_id,
            )
        }
        ("POST", path) if live_session_suffix_id(path, "/close").is_some() => close_live_session(
            live_session_suffix_id(path, "/close").expect("checked live close path"),
            &request.body,
            live_sessions,
            authenticated_device_id,
        ),
        ("GET", path) if live_session_path_id(path).is_some() => read_live_session(
            live_session_path_id(path).expect("checked live session path"),
            live_sessions,
            authenticated_device_id,
        ),
        _ if route_path == "/v1/live/media" => Err(LiveRuntimeError::new(
            426,
            "live_websocket_required",
            "the live media endpoint requires a WebSocket upgrade",
        )),
        _ if route_path.starts_with("/v1/live/") => Err(LiveRuntimeError::new(
            404,
            "live_route_not_found",
            "the requested live route does not exist",
        )),
        _ => return None,
    };
    Some(response.or_else(live_error_response))
}

fn read_live_session(
    session_id: &str,
    live_sessions: &SharedLiveSessionStore,
    authenticated_device_id: Option<&str>,
) -> std::result::Result<(u16, String), LiveRuntimeError> {
    match authenticated_device_id {
        Some(actor) => live_sessions.get_for_member(session_id, actor),
        None => live_sessions.get(session_id),
    }
    .and_then(json_snapshot)
}

fn create_live_session(
    body: &str,
    live_sessions: &SharedLiveSessionStore,
    surface_instances: &SharedSurfaceInstanceStore,
    authenticated_device_id: Option<&str>,
) -> std::result::Result<(u16, String), LiveRuntimeError> {
    let mut request: LiveSessionCreateRequest = parse_live_body(body)?;
    loom_protocol::validate_control_envelope(&request.envelope)
        .map_err(|error| invalid_live_protocol(error.to_string()))?;
    let LiveControlMessage::SessionStart(start) = &mut request.envelope.message else {
        return Err(invalid_live_protocol(
            "live session creation requires session_start",
        ));
    };
    if start.session.frame_stream.transport != loom_protocol::LiveMediaTransport::WebsocketBinary {
        return Err(invalid_live_protocol(
            "Phase 4 live sessions require websocket_binary media",
        ));
    }
    start.session.frame_stream.endpoint = Some("/v1/live/media".to_owned());
    let actor = authenticated_device_id
        .unwrap_or(start.requested_by_device_id.as_str())
        .to_owned();
    validate_live_attachment(
        surface_instances,
        &request.surface_instance_id,
        &request.source_attachment_id,
        &start.session.source_device_id,
        Some(&start.session.source_hook_id),
        authenticated_device_id,
    )?;
    let (created, snapshot) = live_sessions.create(&actor, request.envelope)?;
    let status = if created { 201 } else { 200 };
    serde_json::to_string(&snapshot)
        .map(|body| (status, body))
        .map_err(|_| unavailable())
}

fn attach_live_viewer(
    session_id: &str,
    body: &str,
    live_sessions: &SharedLiveSessionStore,
    surface_instances: &SharedSurfaceInstanceStore,
    authenticated_device_id: Option<&str>,
) -> std::result::Result<(u16, String), LiveRuntimeError> {
    let request: LiveViewerAttachRequest = parse_live_body(body)?;
    loom_protocol::validate_control_envelope(&request.envelope)
        .map_err(|error| invalid_live_protocol(error.to_string()))?;
    ensure_route_session(session_id, &request.envelope)?;
    let LiveControlMessage::SessionAck(ack) = &request.envelope.message else {
        return Err(invalid_live_protocol(
            "viewer attachment requires session_ack",
        ));
    };
    let actor = authenticated_device_id
        .unwrap_or(ack.responder_device_id.as_str())
        .to_owned();
    validate_live_attachment(
        surface_instances,
        &request.surface_instance_id,
        &request.attachment_id,
        &actor,
        None,
        authenticated_device_id,
    )?;
    live_sessions
        .attach_viewer(&actor, request.envelope)
        .and_then(json_snapshot)
}

fn resume_live_session(
    session_id: &str,
    body: &str,
    live_sessions: &SharedLiveSessionStore,
    authenticated_device_id: Option<&str>,
) -> std::result::Result<(u16, String), LiveRuntimeError> {
    let request: LiveEnvelopeRequest = parse_live_body(body)?;
    loom_protocol::validate_control_envelope(&request.envelope)
        .map_err(|error| invalid_live_protocol(error.to_string()))?;
    ensure_route_session(session_id, &request.envelope)?;
    let LiveControlMessage::ResumeRequest(resume) = &request.envelope.message else {
        return Err(invalid_live_protocol("live resume requires resume_request"));
    };
    let actor = authenticated_device_id
        .unwrap_or(resume.requester_device_id.as_str())
        .to_owned();
    live_sessions
        .resume(&actor, request.envelope)
        .and_then(json_snapshot)
}

fn change_live_controller(
    session_id: &str,
    body: &str,
    live_sessions: &SharedLiveSessionStore,
    surface_instances: &SharedSurfaceInstanceStore,
    authenticated_device_id: Option<&str>,
) -> std::result::Result<(u16, String), LiveRuntimeError> {
    let request: LiveControlLeaseRequest = parse_live_body(body)?;
    let actor = authenticated_device_id.ok_or_else(|| {
        LiveRuntimeError::new(
            400,
            "live_controller_device_required",
            "administrator controller requests must use a paired device session",
        )
    })?;
    validate_live_attachment(
        surface_instances,
        &request.surface_instance_id,
        &request.attachment_id,
        actor,
        None,
        authenticated_device_id,
    )?;
    live_sessions
        .change_controller(actor, session_id, &request)
        .and_then(json_snapshot)
}

fn close_live_session(
    session_id: &str,
    body: &str,
    live_sessions: &SharedLiveSessionStore,
    authenticated_device_id: Option<&str>,
) -> std::result::Result<(u16, String), LiveRuntimeError> {
    let request: LiveEnvelopeRequest = parse_live_body(body)?;
    loom_protocol::validate_control_envelope(&request.envelope)
        .map_err(|error| invalid_live_protocol(error.to_string()))?;
    ensure_route_session(session_id, &request.envelope)?;
    let LiveControlMessage::SessionEnd(end) = &request.envelope.message else {
        return Err(invalid_live_protocol("live close requires session_end"));
    };
    let actor = authenticated_device_id
        .unwrap_or(end.ended_by_device_id.as_str())
        .to_owned();
    live_sessions
        .close(&actor, request.envelope)
        .and_then(json_snapshot)
}

fn forward_live_input(
    session_id: &str,
    body: &str,
    live_sessions: &SharedLiveSessionStore,
    surface_instances: &SharedSurfaceInstanceStore,
    authenticated_device_id: Option<&str>,
) -> std::result::Result<(u16, String), LiveRuntimeError> {
    let request: LiveInputForwardRequest = parse_live_body(body)?;
    loom_protocol::validate_control_envelope(&request.envelope)
        .map_err(|error| invalid_live_protocol(error.to_string()))?;
    ensure_route_session(session_id, &request.envelope)?;
    let LiveControlMessage::InputEvent(input) = &request.envelope.message else {
        return Err(invalid_live_protocol(
            "live input forwarding requires input_event",
        ));
    };
    let actor = authenticated_device_id.ok_or_else(|| {
        LiveRuntimeError::new(
            401,
            "live_input_device_required",
            "remote input requires a paired device session",
        )
    })?;
    validate_live_attachment(
        surface_instances,
        &request.surface_instance_id,
        &request.attachment_id,
        actor,
        None,
        authenticated_device_id,
    )?;
    if input.source_device_id != actor {
        return Err(LiveRuntimeError::new(
            403,
            "live_input_identity_mismatch",
            "the input source must match the authenticated device",
        ));
    }
    let event = live_sessions.forward_input(actor, session_id, request.envelope)?;
    serde_json::to_string(&json!({
        "protocolVersion": loom_protocol::LIVE_PROTOCOL_VERSION,
        "acceptedSequence": event.sequence,
    }))
    .map(|body| (202, body))
    .map_err(|_| unavailable())
}

fn publish_live_observation(
    session_id: &str,
    body: &str,
    live_sessions: &SharedLiveSessionStore,
    surface_instances: &SharedSurfaceInstanceStore,
    surface_actions: &SharedSurfaceActionExecutor,
    authenticated_device_id: Option<&str>,
) -> std::result::Result<(u16, String), LiveRuntimeError> {
    let request: LiveObservationPublishRequest = parse_live_body(body)?;
    loom_protocol::validate_control_envelope(&request.envelope)
        .map_err(|error| invalid_live_protocol(error.to_string()))?;
    ensure_route_session(session_id, &request.envelope)?;
    if !matches!(
        &request.envelope.message,
        LiveControlMessage::Observation(_)
    ) {
        return Err(invalid_live_protocol(
            "live observation publishing requires observation",
        ));
    }
    let actor = authenticated_device_id.ok_or_else(|| {
        LiveRuntimeError::new(
            401,
            "live_observation_device_required",
            "live observations require a paired source device session",
        )
    })?;
    validate_live_attachment(
        surface_instances,
        &request.surface_instance_id,
        &request.attachment_id,
        actor,
        None,
        authenticated_device_id,
    )?;
    let accepted_sequence = request.envelope.sequence;
    let accepted_observation_sequence = match &request.envelope.message {
        LiveControlMessage::Observation(observation) => observation.sequence,
        _ => unreachable!("validated live observation preserves its message type"),
    };
    let outcome = live_sessions.publish_observation(actor, session_id, request.envelope)?;
    let LiveControlMessage::Observation(observation) = outcome.event.message else {
        unreachable!("accepted live observation preserves its message type");
    };
    debug_assert_eq!(observation.sequence, accepted_observation_sequence);
    for dispatch in outcome.dispatches {
        let epoch = dispatch.epoch;
        let audit = dispatch_live_trigger(
            dispatch,
            live_sessions,
            surface_instances,
            surface_actions,
        );
        if let Err(error) = live_sessions.finalize_trigger_dispatch(session_id, epoch, audit) {
            eprintln!(
                "live trigger audit finalization failed after reserved dispatch: {}",
                error.message
            );
        }
    }
    serde_json::to_string(&json!({
        "protocolVersion": loom_protocol::LIVE_PROTOCOL_VERSION,
        "acceptedSequence": accepted_sequence,
        "acceptedObservationSequence": accepted_observation_sequence,
    }))
    .map(|body| (202, body))
    .map_err(|_| unavailable())
}

fn configure_live_trigger(
    session_id: &str,
    body: &str,
    live_sessions: &SharedLiveSessionStore,
    surface_instances: &SharedSurfaceInstanceStore,
    authenticated_device_id: Option<&str>,
) -> std::result::Result<(u16, String), LiveRuntimeError> {
    let request: LiveTriggerBindingRequest = parse_live_body(body)?;
    loom_protocol::validate_control_envelope(&request.envelope)
        .map_err(|error| invalid_live_protocol(error.to_string()))?;
    ensure_route_session(session_id, &request.envelope)?;
    if !matches!(
        &request.envelope.message,
        LiveControlMessage::TriggerCondition(_)
    ) {
        return Err(invalid_live_protocol(
            "live trigger registration requires trigger_condition",
        ));
    }
    let actor = authenticated_device_id.ok_or_else(|| {
        LiveRuntimeError::new(
            401,
            "live_trigger_device_required",
            "live triggers require a paired viewer device session",
        )
    })?;
    validate_live_attachment(
        surface_instances,
        &request.target.surface_instance_id,
        &request.target.surface_attachment_id,
        actor,
        None,
        authenticated_device_id,
    )?;
    live_sessions
        .upsert_trigger_binding(
            actor,
            &request.binding_id,
            request.enabled,
            request.target,
            request.envelope,
        )
        .and_then(json_snapshot)
}

fn list_live_sessions(
    live_sessions: &SharedLiveSessionStore,
) -> std::result::Result<(u16, String), LiveRuntimeError> {
    let sessions = live_sessions.list()?;
    serde_json::to_string(&json!({
        "protocolVersion": loom_protocol::LIVE_PROTOCOL_VERSION,
        "sessions": sessions,
    }))
    .map(|body| (200, body))
    .map_err(|_| unavailable())
}

fn live_status(
    live_sessions: &SharedLiveSessionStore,
) -> std::result::Result<(u16, String), LiveRuntimeError> {
    serde_json::to_string(&live_sessions.status())
        .map(|body| (200, body))
        .map_err(|_| unavailable())
}

fn live_session_events(
    session_id: &str,
    path: &str,
    live_sessions: &SharedLiveSessionStore,
    authenticated_device_id: Option<&str>,
) -> std::result::Result<(u16, String), LiveRuntimeError> {
    let actor = authenticated_device_id.ok_or_else(|| {
        LiveRuntimeError::new(
            401,
            "live_events_device_required",
            "live events require a paired device session",
        )
    })?;
    let after = query_value(path, "after")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or_default();
    let timeout_ms = query_value(path, "timeoutMs")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or_default()
        .min(1_000);
    let (reset, events) = live_sessions.wait_events_after(
        session_id,
        actor,
        after,
        Duration::from_millis(timeout_ms),
    )?;
    let next = events.last().map(|event| event.sequence).unwrap_or(after);
    serde_json::to_string(&json!({
        "protocolVersion": loom_protocol::LIVE_PROTOCOL_VERSION,
        "next": next,
        "reset": reset,
        "events": events,
    }))
    .map(|body| (200, body))
    .map_err(|_| unavailable())
}
