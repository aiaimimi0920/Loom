// Top-level authenticated HTTP route selection and handler dispatch.
fn route(
    request: &ParsedHttpRequest,
    hook_settings: &HookSettings,
    run_store: &SharedRunStore,
    run_store_status: RunStoreStatus,
    brain_planner: &SharedBrainPlanner,
    auth_token: &str,
    config_registry: &ConfigRegistry,
    config_store: &FileDocumentStore,
    mcp_servers: &SharedMcpServerStore,
    tool_registry: &ToolRegistry,
    workflow_store: &WorkflowStore,
    hook_bridge: &SharedHookBridgeRuntime,
    device_registry: &SharedDeviceRegistryStore,
    surface_instances: &SharedSurfaceInstanceStore,
    surface_actions: &SharedSurfaceActionExecutor,
    surface_resources: &SharedSurfaceResourceStore,
    settings: &SharedLoomSettingsStore,
    shared_images: &SharedImageStoreHandle,
    ocr_provider: &OcrProviderHandle,
    settings_base_url: &str,
    mcp_registry_endpoint: &str,
    request_executor: RequestExecutorStatus,
    canvas_workflow_root: &Path,
    framework_registry: &FrameworkRegistry,
    control_plane_root: &Path,
    bundled_art_sha256_allowlist: &BTreeSet<String>,
) -> Result<(u16, String)> {
    let route_path = request
        .path
        .split('?')
        .next()
        .unwrap_or(request.path.as_str());
    let public_device_auth_route = is_public_device_auth_route(request.method.as_str(), route_path);
    let admin_authenticated = request.has_admin_credential(auth_token);
    // The administrator bearer is decided first on purpose. A desktop client that has just been
    // re-paired still sends its previous `Authorization: Device …` credential, and surfacing that
    // credential's error ahead of a valid administrator bearer would reject a request the caller
    // was already entitled to make. A stale device credential alongside a valid admin bearer is
    // therefore ignored rather than fatal; with no admin bearer it is still reported in full.
    let authenticated_device_id = match authenticate_http_device_session(request, device_registry) {
        Ok(device_id) => device_id,
        Err(_) if admin_authenticated => None,
        Err(error) => return device_auth_error_response(error),
    };
    if !public_device_auth_route && !admin_authenticated && authenticated_device_id.is_none() {
        return structured_error(
            401,
            json!({
                "code": "unauthorized",
                "message": "missing or invalid Loom administrator or device session credential",
            }),
        );
    }
    if authenticated_device_id.is_some()
        && !device_session_route_allowed(request.method.as_str(), route_path)
        && !public_device_auth_route
    {
        return structured_error(
            403,
            json!({
                "code": "device_session_scope_denied",
                "message": "device session is not permitted to access this Loom route",
            }),
        );
    }

    // Ordered route groups preserve the former match-table precedence and final fallback.
    route_surfaces_devices(
        request,
        hook_settings,
        run_store,
        run_store_status,
        brain_planner,
        config_registry,
        config_store,
        mcp_servers,
        tool_registry,
        workflow_store,
        hook_bridge,
        device_registry,
        surface_instances,
        surface_actions,
        surface_resources,
        settings,
        shared_images,
        ocr_provider,
        settings_base_url,
        mcp_registry_endpoint,
        request_executor,
        canvas_workflow_root,
        framework_registry,
        control_plane_root,
        bundled_art_sha256_allowlist,
        &authenticated_device_id,
        route_path,
    )
}
