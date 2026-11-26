// Top-level authenticated HTTP route selection and handler dispatch.
fn route_with_runtime(
    runtime: &DaemonRuntime,
    request: &ParsedHttpRequest,
) -> Result<(u16, String)> {
    if request.method == "GET" && request.path.split('?').next() == Some("/v1/naccount/callback") {
        return match naccount::callback(&runtime.control_plane_root, &request.path) {
            Ok(value) => Ok((200, serde_json::to_string(&value)?)),
            Err(error) => structured_error(
                error.status,
                json!({"code":error.code, "message":error.code}),
            ),
        };
    }
    runtime_log_debug(format!("{} {}", request.method, request.path));
    if let Some(response) = route_outbound_mcp_http(runtime, request) {
        return response;
    }
    if let Some(response) = route_cloud_relay_mcp_dispatch(runtime, request) {
        return response;
    }
    let path = request.path.split('?').next().unwrap_or_default();
    if offline_peers::OfflinePeers::handles(path) {
        // Peer proofs have their own pinned-key authentication; config remains admin-only.
        return match runtime.offline_peers.handle(
            request,
            request.has_admin_credential(&runtime.auth_token),
            &runtime.device_registry,
            &runtime.control_plane_root,
        ) {
            Ok(value) => Ok((200, serde_json::to_string(&value)?)),
            Err(error) => structured_error(
                error.status,
                json!({"code": error.code, "message": error.code}),
            ),
        };
    }
    let (status, body) = route(
        request,
        &runtime.run_store,
        runtime.run_store_status,
        &runtime.ai_gateway,
        &runtime.capability_runtime,
        &runtime.capability_dispatch,
        &runtime.capability_resources,
        &runtime.auth_token,
        runtime.config_registry.as_ref(),
        &runtime.config_store,
        &runtime.mcp_servers,
        &runtime.tool_registry,
        &runtime.workflow_store,
        &runtime.hook_bridge,
        &runtime.device_registry,
        &runtime.live_sessions,
        &runtime.walls,
        &runtime.surface_instances,
        &runtime.surface_actions,
        &runtime.surface_resources,
        &runtime.settings,
        &runtime.shared_images,
        &runtime.settings_base_url,
        &runtime.mcp_registry_endpoint,
        runtime.enable_management_ui,
        runtime.request_executor_status,
        &runtime.canvas_workflow_root,
        &runtime.framework_registry,
        &runtime.control_plane_root,
        &runtime.bundled_art_sha256_allowlist,
        &runtime.projection_owner,
        &runtime.offline_peers,
        &runtime.local_art_tasks,
        &runtime.shutting_down,
    )?;
    if status == 200
        && ((request.method == "POST" && path == "/v1/projections/targets")
            || (request.method == "GET" && path == "/v1/projection-settings"))
    {
        let mut value: Value = serde_json::from_str(&body)?;
        connected_users::initialize(&mut value, &runtime.control_plane_root)?;
        runtime.offline_peers.append_targets(&mut value);
        value["friends"]["status"] = value["peerDirectory"]["status"].clone();
        if path == "/v1/projections/targets" {
            // Peer I/O has finished before taking the registry lock. Directory membership
            // is advisory; create/offer still recheck live device and recipient authority.
            let registry = runtime
                .device_registry
                .lock()
                .map_err(|_| anyhow::anyhow!("device registry lock poisoned"))?;
            match registry.projection_settings.view() {
                Ok(document) => projection_settings::append_groups(&mut value, document),
                Err(error) => {
                    return structured_error(
                        error.status,
                        json!({"code": error.code, "message": error.code}),
                    )
                }
            }
        }
        return Ok((status, serde_json::to_string(&value)?));
    }
    Ok((status, body))
}

fn route(
    request: &ParsedHttpRequest,
    run_store: &SharedRunStore,
    run_store_status: RunStoreStatus,
    ai_gateway: &SharedAiGateway,
    capability_runtime: &SharedCapabilityRuntime,
    capability_dispatch: &SharedCapabilityDispatchRegistry,
    capability_resources: &SharedCapabilityResourceBroker,
    auth_token: &str,
    config_registry: &ConfigRegistry,
    config_store: &FileDocumentStore,
    mcp_servers: &SharedMcpServerStore,
    tool_registry: &ToolRegistry,
    workflow_store: &WorkflowStore,
    hook_bridge: &SharedOtterBridgeRuntime,
    device_registry: &SharedDeviceRegistryStore,
    live_sessions: &SharedLiveSessionStore,
    walls: &SharedWallStore,
    surface_instances: &SharedSurfaceInstanceStore,
    surface_actions: &SharedSurfaceActionExecutor,
    surface_resources: &SharedSurfaceResourceStore,
    settings: &SharedLoomSettingsStore,
    shared_images: &SharedImageStoreHandle,
    settings_base_url: &str,
    mcp_registry_endpoint: &str,
    enable_management_ui: bool,
    request_executor: RequestExecutorStatus,
    canvas_workflow_root: &Path,
    framework_registry: &FrameworkRegistry,
    control_plane_root: &Path,
    bundled_art_sha256_allowlist: &BTreeSet<String>,
    projection_owner: &ProjectionOwner,
    offline_peers: &offline_peers::OfflinePeers,
    local_art_tasks: &local_art::Tasks,
    shutting_down: &AtomicBool,
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

    if let Some(response) = route_ai_gateway(
        request,
        route_path,
        admin_authenticated,
        ai_gateway,
        control_plane_root,
    ) {
        return response;
    }
    if let Some(response) = terminal_relay::route(
        request,
        route_path,
        authenticated_device_id.as_deref(),
        admin_authenticated,
        control_plane_root,
    ) {
        return response;
    }
    if let Some(response) = cloud_relay::route(
        request,
        route_path,
        authenticated_device_id.as_deref(),
        admin_authenticated,
        control_plane_root,
    ) {
        return response;
    }
    if let Some(response) = otter_sync::route(
        request,
        route_path,
        authenticated_device_id.as_deref(),
        admin_authenticated,
        control_plane_root,
        workflow_store,
        hook_bridge,
    ) {
        return response;
    }
    if admin_authenticated {
        if request.method == "GET" && route_path == "/v1/devices/hierarchy" {
            return build_devices_hierarchy_response(
                device_registry,
                hook_bridge,
                control_plane_root,
            );
        }
        if let Some(response) = mcp_share_tokens::route_admin(
            request,
            route_path,
            control_plane_root,
            settings_base_url,
        ) {
            return response;
        }
        if let Some(response) = companion_config::route(request, route_path, config_store.root()) {
            return response;
        }
        if let Some(response) = route_unified_store(
            request,
            route_path,
            tool_registry,
            capability_runtime,
            mcp_servers,
            control_plane_root,
        ) {
            return response;
        }
    }
    if let Some(response) = otter_locale::handle(
        request,
        route_path,
        authenticated_device_id.as_deref(),
        admin_authenticated,
        device_registry,
    ) {
        return response;
    }
    if let Some(response) = local_art::route(
        request,
        authenticated_device_id.as_deref(),
        admin_authenticated,
        &local_art::Services {
            root: control_plane_root,
            devices: device_registry,
            tools: tool_registry,
            frameworks: framework_registry,
            issuer: offline_peers,
            tasks: local_art_tasks,
            runtime: capability_runtime,
            resources: capability_resources,
            surface_resources,
            live_sessions,
        },
    ) {
        return response;
    }
    if matches!(
        route_path,
        "/v1/otter/identity" | "/v1/otter/connection-info"
    ) || route_path.starts_with("/v1/otter/connections")
    {
        return route_otter_connections(
            request,
            route_path,
            authenticated_device_id.as_deref(),
            admin_authenticated,
            device_registry,
            hook_bridge,
            control_plane_root,
        );
    }
    if request.method == "POST"
        && route_path == "/v1/projections/edit"
        && authenticated_device_id.is_some()
        && projection_edit::needs_prune(&request.body)
    {
        if let Err(error) = offline_peers.prune_edit_sessions(device_registry) {
            return structured_error(
                error.status,
                json!({"code": error.code, "message": error.code}),
            );
        }
    }
    if route_path == "/v1/projection-settings" {
        if !admin_authenticated {
            return structured_error(403, json!({"code":"projection_settings_admin_required"}));
        }
        return route_projection_settings(request, device_registry);
    }
    if request.method == "POST"
        && (route_path.starts_with("/v1/naccount/") || route_path.starts_with("/v1/asset-library/"))
    {
        if !admin_authenticated {
            return structured_error(403, json!({"code":"cloud_account_admin_required"}));
        }
        if route_path == "/v1/asset-library/install" {
            let package = match asset_library::prepare_install(control_plane_root, &request.body) {
                Ok(package) => package,
                Err(error) => {
                    return structured_error(
                        error.status,
                        json!({"code":error.code, "message":error.code}),
                    )
                }
            };
            return install_asset_library_package(
                package,
                control_plane_root,
                capability_runtime,
                capability_resources,
                hook_bridge,
                tool_registry,
                framework_registry,
                workflow_store,
                mcp_servers,
            );
        }
        let result = if let Some(action) = route_path.strip_prefix("/v1/naccount/") {
            if action == "start"
                && reqwest::Url::parse(settings_base_url)
                    .ok()
                    .is_none_or(|url| url.origin().ascii_serialization() != "http://127.0.0.1:8765")
            {
                return structured_error(
                    409,
                    json!({"code":"naccount_callback_port_required", "message":"naccount_callback_port_required"}),
                );
            }
            naccount::handle(control_plane_root, action)
        } else {
            asset_library::handle(control_plane_root, &route_path[18..], &request.body)
        };
        return match result {
            Ok(value) => Ok((200, serde_json::to_string(&value)?)),
            Err(error) => structured_error(
                error.status,
                json!({"code":error.code, "message":error.code}),
            ),
        };
    }
    // Account credentials remain admin-only; Otter device sessions cannot operate this API.
    if request.method == "POST" && route_path.starts_with("/v1/account/") {
        return match projection_owner.account(control_plane_root, &route_path[12..], &request.body)
        {
            Ok(value) => Ok((200, serde_json::to_string(&value)?)),
            Err(error) => structured_error(
                error.status,
                json!({"code":error.code, "message":error.code}),
            ),
        };
    }
    if request.method == "POST" && PROJECTION_V2_ROUTES.contains(&route_path) {
        return handle_projection_v2_route(
            route_path,
            &request.body,
            authenticated_device_id.as_deref(),
            control_plane_root,
            projection_owner,
        );
    }
    if let Some(response) = route_capability_plugins(
        request,
        route_path,
        control_plane_root,
        capability_runtime,
        capability_resources,
        hook_bridge,
    ) {
        return response;
    }
    if request.method == "POST" && route_path == "/v1/invoke/cancel" {
        return cancel_capability_invocation(&request.body, capability_dispatch.plugin_runtime());
    }
    if request.method == "POST" && route_path == "/v1/invoke" {
        return invoke_capability(
            &request.body,
            run_store,
            ai_gateway,
            capability_dispatch,
            capability_resources,
            surface_resources,
            control_plane_root,
        );
    }

    if let Some(response) = route_wall_surfaces(
        request,
        route_path,
        &WallSurfaceServices {
            walls,
            instances: surface_instances,
            actions: surface_actions,
            resources: surface_resources,
            shared_images,
            bridge: hook_bridge,
            tools: tool_registry,
            frameworks: framework_registry,
            root: control_plane_root,
        },
        authenticated_device_id.as_deref(),
    ) {
        return response;
    }

    if let Some(response) = route_walls(
        request,
        route_path,
        walls,
        device_registry,
        surface_resources,
        live_sessions,
        authenticated_device_id.as_deref(),
    ) {
        prune_wall_surfaces(
            walls,
            device_registry,
            surface_instances,
            surface_resources,
            shared_images,
        );
        return response;
    }

    if let Some(response) = route_live_sessions(
        request,
        route_path,
        live_sessions,
        surface_instances,
        surface_actions,
        authenticated_device_id.as_deref(),
    ) {
        return response;
    }

    if request.method == "POST" {
        if let Some(tool_id) = tool_execute_path_id(route_path) {
            return execute_registered_tool(
                &tool_id,
                &request.body,
                mcp_servers,
                tool_registry,
                workflow_store,
                framework_registry,
                run_store,
                control_plane_root,
                shutting_down,
                request_executor.mode != "inline",
            );
        }
    }
    // Ordered route groups preserve the former match-table precedence and final fallback.
    route_surfaces_devices(
        request,
        run_store,
        run_store_status,
        ai_gateway,
        config_registry,
        config_store,
        mcp_servers,
        tool_registry,
        workflow_store,
        hook_bridge,
        capability_runtime,
        capability_resources,
        device_registry,
        live_sessions,
        surface_instances,
        surface_actions,
        surface_resources,
        settings,
        shared_images,
        settings_base_url,
        mcp_registry_endpoint,
        enable_management_ui,
        request_executor,
        canvas_workflow_root,
        framework_registry,
        control_plane_root,
        bundled_art_sha256_allowlist,
        &authenticated_device_id,
        route_path,
        walls,
    )
}
