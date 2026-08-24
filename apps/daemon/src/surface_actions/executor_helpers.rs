// Wire confirmation broadcasting and test-only package validation helpers.
fn broadcast_confirmation(
    hook_bridge: &SharedHookBridgeRuntime,
    confirmation: &SurfaceConfirmationRequest,
) {
    broadcast_hook_bridge_json(
        hook_bridge,
        json!({
            "method": SURFACE_EVENT_CONFIRMATION_REQUEST,
            "params": confirmation,
        }),
    );
}

#[cfg(test)]
fn validate_locked_tool(
    descriptor: &loom_protocol::SurfaceInstanceDescriptor,
    tool: &ToolDefinition,
) -> Result<(), SurfaceStoreError> {
    if super::art_version_from_tool(tool) != descriptor.art_version {
        return Err(SurfaceStoreError::Conflict(
            "Surface instance Art version is no longer active".into(),
        ));
    }
    let digest = tool
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("artPackage"))
        .and_then(|package| package.get("digest"))
        .and_then(Value::as_str)
        .map(|digest| digest.strip_prefix("sha256:").unwrap_or(digest))
        .unwrap_or_default();
    if !digest.eq_ignore_ascii_case(&descriptor.package_digest) {
        return Err(SurfaceStoreError::Conflict(
            "Surface instance package digest is no longer active".into(),
        ));
    }
    Ok(())
}
