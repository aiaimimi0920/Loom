// Surface resources, health/configuration, device sessions, and Surface instance routes.
#[allow(clippy::too_many_arguments, unused_variables)]
fn route_surfaces_devices(
    request: &ParsedHttpRequest,
    hook_settings: &HookSettings,
    run_store: &SharedRunStore,
    run_store_status: RunStoreStatus,
    brain_planner: &SharedBrainPlanner,
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
    authenticated_device_id: &Option<String>,
    route_path: &str,
) -> Result<(u16, String)> {
    match (request.method.as_str(), route_path) {
        ("POST", "/v1/surfaces/resources") => {
            create_surface_resource(&request.body, surface_resources, shared_images)
        }
        ("DELETE", path) if path_id(path, "/v1/surfaces/resource-leases/").is_some() => {
            release_surface_resource_lease(
                path_id(path, "/v1/surfaces/resource-leases/").expect("checked path"),
                surface_resources,
                shared_images,
            )
        }
        ("GET", "/health") => Ok((
            200,
            serde_json::to_string(&HealthResponse {
                status: "ok",
                version: env!("CARGO_PKG_VERSION"),
            })?,
        )),
        ("GET", "/status") => Ok((
            200,
            serde_json::to_string(&StatusResponse {
                status: "ready",
                pid: std::process::id(),
                executable_path: std::env::current_exe()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default(),
                modules: module_statuses(),
                hooks: hook_settings.summary(),
                brain_planner: brain_planner.status(),
                run_store: run_store_status,
                request_executor,
            })?,
        )),
        ("GET", "/v1/configuration/claims") if configuration_claim_app(&request.path).is_some() => {
            configuration_claim(
                configuration_claim_app(&request.path).expect("checked path"),
                config_registry,
                settings_base_url,
            )
        }
        ("GET", "/settings") => settings_index(config_registry, config_store),
        ("GET", path) if app_from_path(path, "/settings/").is_some() => settings_app(
            app_from_path(path, "/settings/").expect("checked app path"),
            config_registry,
            config_store,
        ),
        ("GET", path) if app_from_path(path, "/v1/configuration/apps/").is_some() => {
            get_managed_config(
                app_from_path(path, "/v1/configuration/apps/").expect("checked app path"),
                config_registry,
                config_store,
            )
        }
        ("PUT", path) if app_from_path(path, "/v1/configuration/apps/").is_some() => {
            put_managed_config(
                app_from_path(path, "/v1/configuration/apps/").expect("checked app path"),
                &request.body,
                config_registry,
                config_store,
            )
        }
        ("GET", "/v1/capabilities") => capabilities(),
        ("GET", "/v1/surfaces/stream") => poll_surface_stream(
            &request.path,
            hook_bridge,
            surface_instances,
            authenticated_device_id.as_deref(),
        ),
        ("POST", "/v1/device-sessions/challenges") => {
            create_device_session_challenge(&request.body, device_registry)
        }
        ("POST", "/v1/device-sessions") => issue_device_session(&request.body, device_registry),
        ("POST", "/v1/surfaces/actions/cancel") => cancel_surface_action(
            &request.body,
            surface_actions,
            device_registry,
            authenticated_device_id.as_deref(),
        ),
        ("POST", "/v1/surfaces/confirmations/decision") => decide_surface_confirmation(
            &request.body,
            surface_actions,
            device_registry,
            authenticated_device_id.as_deref(),
        ),
        ("GET", "/v1/surfaces/instances") => list_surface_instances(surface_instances),
        ("POST", "/v1/surfaces/attach") => attach_and_mount_surface(
            &request.body,
            surface_instances,
            device_registry,
            tool_registry,
            framework_registry,
            control_plane_root,
            hook_bridge,
            surface_resources,
            shared_images,
            authenticated_device_id.as_deref(),
        ),
        ("POST", "/v1/surfaces/instances") => create_surface_instance(
            &request.body,
            surface_instances,
            tool_registry,
            framework_registry,
            control_plane_root,
        ),
        ("POST", path)
            if path_id_with_suffix(path, "/v1/surfaces/instances/", "/attachments").is_some() =>
        {
            attach_surface_instance(
                path_id_with_suffix(path, "/v1/surfaces/instances/", "/attachments")
                    .expect("checked path"),
                &request.body,
                surface_instances,
                device_registry,
                authenticated_device_id.as_deref(),
            )
        }
        ("PUT", path)
            if path_id_with_suffix(path, "/v1/surfaces/instances/", "/snapshot").is_some() =>
        {
            put_surface_snapshot(
                path_id_with_suffix(path, "/v1/surfaces/instances/", "/snapshot")
                    .expect("checked path"),
                &request.body,
                surface_instances,
                surface_resources,
                hook_bridge,
            )
        }
        ("POST", path)
            if path_id_with_suffix(path, "/v1/surfaces/instances/", "/patch").is_some() =>
        {
            apply_surface_patch(
                path_id_with_suffix(path, "/v1/surfaces/instances/", "/patch")
                    .expect("checked path"),
                &request.body,
                surface_instances,
                surface_resources,
                hook_bridge,
            )
        }
        ("POST", path)
            if path_id_with_suffix(path, "/v1/surfaces/instances/", "/generation").is_some() =>
        {
            begin_surface_generation(
                path_id_with_suffix(path, "/v1/surfaces/instances/", "/generation")
                    .expect("checked path"),
                &request.body,
                surface_instances,
                hook_bridge,
            )
        }
        ("POST", path)
            if path_id_with_suffix(path, "/v1/surfaces/instances/", "/lifecycle").is_some() =>
        {
            transition_surface_lifecycle(
                path_id_with_suffix(path, "/v1/surfaces/instances/", "/lifecycle")
                    .expect("checked path"),
                &request.body,
                surface_instances,
                hook_bridge,
                surface_resources,
                shared_images,
                authenticated_device_id.as_deref(),
            )
        }
        ("POST", path)
            if path_id_with_suffix(path, "/v1/surfaces/instances/", "/preview").is_some() =>
        {
            commit_surface_preview(
                path_id_with_suffix(path, "/v1/surfaces/instances/", "/preview")
                    .expect("checked path"),
                &request.body,
                surface_instances,
                surface_resources,
            )
        }
        ("POST", path)
            if path_id_with_suffix(path, "/v1/surfaces/instances/", "/result").is_some() =>
        {
            commit_surface_result(
                path_id_with_suffix(path, "/v1/surfaces/instances/", "/result")
                    .expect("checked path"),
                &request.body,
                surface_instances,
                surface_resources,
            )
        }
        ("POST", path)
            if path_id_with_suffix(path, "/v1/surfaces/instances/", "/failure").is_some() =>
        {
            record_surface_failure(
                path_id_with_suffix(path, "/v1/surfaces/instances/", "/failure")
                    .expect("checked path"),
                &request.body,
                surface_instances,
            )
        }
        ("POST", path)
            if path_id_with_suffix(path, "/v1/surfaces/instances/", "/events").is_some() =>
        {
            accept_surface_event(
                path_id_with_suffix(path, "/v1/surfaces/instances/", "/events")
                    .expect("checked path"),
                &request.body,
                surface_actions,
                surface_instances,
                authenticated_device_id.as_deref(),
            )
        }
        ("POST", path)
            if path_id_with_suffix(path, "/v1/surfaces/instances/", "/migrate").is_some() =>
        {
            migrate_surface_instance(
                path_id_with_suffix(path, "/v1/surfaces/instances/", "/migrate")
                    .expect("checked path"),
                &request.body,
                surface_instances,
                tool_registry,
                framework_registry,
                control_plane_root,
                hook_bridge,
                surface_resources,
                shared_images,
            )
        }
        ("POST", path)
            if path_id_with_suffix(path, "/v1/surfaces/instances/", "/mount").is_some() =>
        {
            mount_surface_instance(
                path_id_with_suffix(path, "/v1/surfaces/instances/", "/mount")
                    .expect("checked path"),
                &request.body,
                surface_instances,
                tool_registry,
                framework_registry,
                control_plane_root,
                hook_bridge,
                surface_resources,
                shared_images,
            )
        }
        ("GET", path) if path_id(path, "/v1/surfaces/instances/").is_some() => {
            get_surface_instance(
                path_id(path, "/v1/surfaces/instances/").expect("checked path"),
                surface_instances,
            )
        }
        ("DELETE", path) if path_id(path, "/v1/surfaces/instances/").is_some() => {
            delete_surface_instance(
                path_id(path, "/v1/surfaces/instances/").expect("checked path"),
                surface_instances,
                hook_bridge,
                surface_resources,
                shared_images,
            )
        }
        _ => route_mcp_art(
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
            authenticated_device_id,
            route_path,
        ),
    }
}
