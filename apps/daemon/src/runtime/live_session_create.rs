// Live publication creation and source ownership at the HTTP boundary.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LiveSessionCreateRequest {
    surface_instance_id: Option<String>,
    source_attachment_id: Option<String>,
    direct_viewer_device_id: Option<String>,
    envelope: LiveControlEnvelope,
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
    validate_live_source_binding(
        surface_instances,
        request.surface_instance_id.as_deref(),
        request.source_attachment_id.as_deref(),
        &start.session.source_device_id,
        Some(&start.session.source_hook_id),
        authenticated_device_id,
        true,
    )?;
    validate_direct_publication(&request, authenticated_device_id)?;
    let (created, snapshot) = live_sessions.create_with_direct_viewer(
        &actor,
        request.envelope,
        request.direct_viewer_device_id,
    )?;
    let status = if created { 201 } else { 200 };
    serde_json::to_string(&snapshot)
        .map(|body| (status, body))
        .map_err(|_| unavailable())
}
