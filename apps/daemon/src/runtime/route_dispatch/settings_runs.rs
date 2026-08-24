// Settings, workflows, devices, Hook bridge, run routes, and the final not-found fallback.
#[allow(clippy::too_many_arguments, unused_variables)]
fn route_settings_runs(
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
        ("GET", "/v1/settings/shortcuts") => get_shortcuts(settings),
        ("PUT", path) if path_id(path, "/v1/settings/shortcuts/").is_some() => put_shortcut(
            path_id(path, "/v1/settings/shortcuts/").expect("checked path"),
            &request.body,
            settings,
            hook_bridge,
        ),
        ("GET", "/v1/runtime/app-paths") => get_app_paths(control_plane_root),
        ("GET", "/v1/runtime/autostart") => get_autostart(settings),
        ("POST", "/v1/runtime/autostart") => set_autostart(&request.body, settings),
        ("POST", "/v1/runtime/minimize-to-tray") => set_minimize_to_tray(&request.body, settings),
        ("GET", "/v1/workflows") => list_workflows(workflow_store),
        ("GET", path) if path_id(path, "/v1/workflows/").is_some() => get_workflow(
            path_id(path, "/v1/workflows/").expect("checked path"),
            workflow_store,
        ),
        ("PUT", path) if path_id(path, "/v1/workflows/").is_some() => put_workflow(
            path_id(path, "/v1/workflows/").expect("checked path"),
            &request.body,
            workflow_store,
        ),
        ("DELETE", path) if path_id(path, "/v1/workflows/").is_some() => delete_workflow(
            path_id(path, "/v1/workflows/").expect("checked path"),
            workflow_store,
        ),
        ("GET", "/v1/hook-bridge/status") => hook_bridge_status(hook_bridge),
        ("POST", "/v1/hook-bridge/workflows/instantiate") => {
            instantiate_hook_workflow(&request.body, hook_bridge)
        }
        ("POST", "/v1/hook-bridge/workflows/nodes/update") => {
            update_hook_workflow_node(&request.body, hook_bridge)
        }
        ("GET", "/v1/devices") => list_managed_devices(device_registry, hook_bridge),
        ("POST", "/v1/devices") => {
            add_managed_device(&request.body, "approved", device_registry, hook_bridge)
        }
        ("POST", "/v1/devices/requests") => {
            add_managed_device(&request.body, "pending", device_registry, hook_bridge)
        }
        ("PUT", path) if path_id(path, "/v1/devices/").is_some() => update_managed_device(
            path_id(path, "/v1/devices/").expect("checked path"),
            &request.body,
            device_registry,
            hook_bridge,
        ),
        ("POST", path) if path_id_with_suffix(path, "/v1/devices/", "/approve").is_some() => {
            approve_managed_device(
                path_id_with_suffix(path, "/v1/devices/", "/approve").expect("checked path"),
                device_registry,
                hook_bridge,
            )
        }
        ("DELETE", path) if path_id(path, "/v1/devices/").is_some() => remove_managed_device(
            path_id(path, "/v1/devices/").expect("checked path"),
            device_registry,
            hook_bridge,
        ),
        ("GET", "/v1/hook-bridge/session") => hook_bridge_session(hook_bridge),
        ("GET", "/v1/hook-bridge/canvas") => hook_canvas_snapshot(),
        ("GET", "/v1/hook-bridge/canvas/workflows") => list_canvas_workflows(canvas_workflow_root),
        ("GET", path) if path_id(path, "/v1/hook-bridge/canvas/workflows/").is_some() => {
            get_canvas_workflow_snapshot(
                path_id(path, "/v1/hook-bridge/canvas/workflows/").expect("checked path"),
                canvas_workflow_root,
            )
        }
        ("PUT", path)
            if path_id_with_suffix(path, "/v1/hook-bridge/canvas/workflows/", "/rename")
                .is_some() =>
        {
            rename_canvas_workflow(
                path_id_with_suffix(path, "/v1/hook-bridge/canvas/workflows/", "/rename")
                    .expect("checked path"),
                &request.body,
                canvas_workflow_root,
            )
        }
        ("PUT", path) if path_id(path, "/v1/hook-bridge/canvas/workflows/").is_some() => {
            save_hook_canvas_workflow(
                path_id(path, "/v1/hook-bridge/canvas/workflows/").expect("checked path"),
                &request.body,
                workflow_store,
                canvas_workflow_root,
            )
        }
        ("DELETE", path) if path_id(path, "/v1/hook-bridge/canvas/workflows/").is_some() => {
            delete_canvas_workflow(
                path_id(path, "/v1/hook-bridge/canvas/workflows/").expect("checked path"),
                canvas_workflow_root,
            )
        }
        ("POST", "/v1/hook-bridge/start") => start_hook_bridge(
            &request.body,
            hook_bridge,
            mcp_servers,
            tool_registry,
            workflow_store,
            settings,
            shared_images,
            ocr_provider,
            framework_registry,
            control_plane_root,
            run_store,
            surface_instances,
            surface_actions,
        ),
        ("POST", "/v1/hook-bridge/stop") => stop_hook_bridge(hook_bridge, shared_images),
        ("POST", "/v1/runs") => start_tea_run(&request.body, run_store),
        ("POST", "/v1/invoke") => invoke_capability(&request.body, run_store, brain_planner),
        ("GET", path) if execution_diagnostics_path_id(path).is_some() => execution_diagnostics(
            execution_diagnostics_path_id(path).expect("checked path"),
            run_store,
        ),
        ("GET", path) if run_events_path_id(path).is_some() => {
            get_run_events(run_events_path_id(path).expect("checked path"), run_store)
        }
        ("GET", path) if run_path_id(path).is_some() => {
            get_run(run_path_id(path).expect("checked path"), run_store)
        }
        ("POST", path) if run_action_path_id(path, "stop").is_some() => run_action(
            run_action_path_id(path, "stop").expect("checked path"),
            &request.body,
            "stopped",
            run_store,
        ),
        ("POST", path) if run_action_path_id(path, "retry").is_some() => run_action(
            run_action_path_id(path, "retry").expect("checked path"),
            &request.body,
            "retrying",
            run_store,
        ),
        _ => structured_error(
            404,
            json!({
                "code": "not_found",
                "message": "Loom endpoint was not found",
            }),
        ),
    }
}
