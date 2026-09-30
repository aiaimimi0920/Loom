// Shared parsing, attachment authorization, and response helpers for live HTTP routes.
fn validate_live_attachment(
    surface_instances: &SharedSurfaceInstanceStore,
    instance_id: &str,
    attachment_id: &str,
    expected_device_id: &str,
    expected_hook_node_id: Option<&str>,
    authenticated_device_id: Option<&str>,
) -> std::result::Result<(), LiveRuntimeError> {
    if authenticated_device_id.is_some_and(|device_id| device_id != expected_device_id) {
        return Err(LiveRuntimeError::new(
            403,
            "live_attachment_identity_mismatch",
            "the authenticated device cannot use another device's attachment",
        ));
    }
    let store = surface_instances.lock().map_err(|_| unavailable())?;
    let instance = store.get(instance_id).ok_or_else(|| {
        LiveRuntimeError::new(
            404,
            "live_surface_not_found",
            "the bound Surface instance was not found",
        )
    })?;
    let attachment = instance.attachments.get(attachment_id).ok_or_else(|| {
        LiveRuntimeError::new(
            404,
            "live_attachment_not_found",
            "the bound Surface attachment was not found",
        )
    })?;
    if attachment.descriptor.device_id != expected_device_id
        || attachment.descriptor.instance_id != instance_id
        || expected_hook_node_id.is_some_and(|node| attachment.descriptor.hook_node_id != node)
    {
        return Err(LiveRuntimeError::new(
            403,
            "live_attachment_identity_mismatch",
            "the Surface attachment does not match the live device and Hook identity",
        ));
    }
    if attachment.lifecycle == loom_protocol::SurfaceLifecycleState::Disposed {
        return Err(LiveRuntimeError::new(
            409,
            "live_attachment_disposed",
            "a disposed Surface attachment cannot bind a live session",
        ));
    }
    Ok(())
}

fn parse_live_body<T: for<'de> Deserialize<'de>>(
    body: &str,
) -> std::result::Result<T, LiveRuntimeError> {
    serde_json::from_str(body).map_err(|error| {
        LiveRuntimeError::new(
            400,
            "live_request_invalid",
            format!("invalid live request: {error}"),
        )
    })
}

fn ensure_route_session(
    session_id: &str,
    envelope: &LiveControlEnvelope,
) -> std::result::Result<(), LiveRuntimeError> {
    if envelope.session_id == session_id {
        Ok(())
    } else {
        Err(invalid_live_protocol(
            "the route and envelope session ids differ",
        ))
    }
}

fn json_snapshot(
    snapshot: LiveSessionRuntimeSnapshot,
) -> std::result::Result<(u16, String), LiveRuntimeError> {
    serde_json::to_string(&snapshot)
        .map(|body| (200, body))
        .map_err(|_| unavailable())
}

fn live_error_response(error: LiveRuntimeError) -> Result<(u16, String)> {
    structured_error(
        error.status,
        json!({ "code": error.code, "message": error.message }),
    )
}

fn invalid_live_protocol(message: impl Into<String>) -> LiveRuntimeError {
    LiveRuntimeError::new(400, "live_protocol_invalid", message)
}

fn live_session_path_id(path: &str) -> Option<&str> {
    let id = path.strip_prefix("/v1/live/sessions/")?;
    (!id.is_empty() && !id.contains('/')).then_some(id)
}

fn live_session_suffix_id<'a>(path: &'a str, suffix: &str) -> Option<&'a str> {
    let id = path
        .strip_prefix("/v1/live/sessions/")?
        .strip_suffix(suffix)?;
    (!id.is_empty() && !id.contains('/')).then_some(id)
}
