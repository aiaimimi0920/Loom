// A standalone projection is an immutable, media-only grant for one paired device.
fn validate_direct_publication(
    request: &LiveSessionCreateRequest,
    actor: Option<&str>,
) -> std::result::Result<(), LiveRuntimeError> {
    let Some(target) = request.direct_viewer_device_id.as_deref() else {
        return Ok(());
    };
    let LiveControlMessage::SessionStart(start) = &request.envelope.message else {
        return Err(invalid_live_protocol(
            "direct publication requires session_start",
        ));
    };
    let session = &start.session;
    if actor != Some(session.source_device_id.as_str())
        || request.surface_instance_id.is_some()
        || request.source_attachment_id.is_some()
        || target == session.source_device_id
        || loom_protocol::validate_live_identifier(target, "direct_viewer_device_id").is_err()
        || !session.interaction_capabilities.is_empty()
        || !session.observation_capabilities.is_empty()
    {
        return Err(invalid_live_protocol(
            "direct publication requires a paired standalone source, distinct target and media-only capabilities",
        ));
    }
    Ok(())
}

fn attach_direct_viewer(
    session_id: &str,
    body: &str,
    sessions: &SharedLiveSessionStore,
    actor: Option<&str>,
) -> std::result::Result<(u16, String), LiveRuntimeError> {
    let actor = actor.ok_or_else(|| {
        LiveRuntimeError::new(
            401,
            "live_direct_device_required",
            "paired device session required",
        )
    })?;
    let request: LiveEnvelopeRequest = parse_live_body(body)?;
    loom_protocol::validate_control_envelope(&request.envelope)
        .map_err(|error| invalid_live_protocol(error.to_string()))?;
    ensure_route_session(session_id, &request.envelope)?;
    sessions
        .attach_viewer_authorized(actor, request.envelope, false, true)
        .and_then(json_snapshot)
}

fn authorize_direct_viewer(
    record: &LiveSessionRecord,
    actor: &str,
    direct: bool,
) -> std::result::Result<(), LiveRuntimeError> {
    // Apply before accepting any sequence or adding membership, regardless of entry route.
    match (&record.direct_viewer, direct) {
        (Some(target), true) if target == actor => Ok(()),
        (None, false) => Ok(()),
        _ => Err(LiveRuntimeError::new(
            403,
            "live_direct_viewer_denied",
            "viewer is not authorized for this publication",
        )),
    }
}

fn ensure_live_collaboration(
    record: &LiveSessionRecord,
) -> std::result::Result<(), LiveRuntimeError> {
    if record.direct_viewer.is_some() {
        return Err(LiveRuntimeError::new(
            403,
            "live_direct_media_only",
            "direct viewers may only receive media",
        ));
    }
    Ok(())
}
