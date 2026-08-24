// Art store, framework, tool execution, shared-memory, and shared-image routes.
#[allow(clippy::too_many_arguments, unused_variables)]
fn route_framework_tools(
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
        ("POST", "/v1/arts/store/publish") => {
            publish_art_to_store(&request.body, tool_registry, control_plane_root)
        }
        ("POST", path)
            if decoded_package_path_id_with_suffix(path, "/v1/frameworks/", "/install")
                .is_some() =>
        {
            install_framework(
                &decoded_package_path_id_with_suffix(path, "/v1/frameworks/", "/install")
                    .expect("checked path"),
                framework_registry,
            )
        }
        ("POST", path)
            if decoded_package_path_id_with_suffix(path, "/v1/frameworks/", "/enable")
                .is_some() =>
        {
            set_framework_enabled(
                &decoded_package_path_id_with_suffix(path, "/v1/frameworks/", "/enable")
                    .expect("checked path"),
                true,
                framework_registry,
            )
        }
        ("POST", path)
            if decoded_package_path_id_with_suffix(path, "/v1/frameworks/", "/disable")
                .is_some() =>
        {
            set_framework_enabled(
                &decoded_package_path_id_with_suffix(path, "/v1/frameworks/", "/disable")
                    .expect("checked path"),
                false,
                framework_registry,
            )
        }
        ("POST", path)
            if decoded_package_path_id_with_suffix(path, "/v1/frameworks/", "/upgrade")
                .is_some() =>
        {
            upgrade_framework_package(
                &decoded_package_path_id_with_suffix(path, "/v1/frameworks/", "/upgrade")
                    .expect("checked path"),
                &request.body,
                framework_registry,
            )
        }
        ("POST", path)
            if decoded_package_path_id_with_suffix(path, "/v1/frameworks/", "/rollback")
                .is_some() =>
        {
            rollback_framework(
                &decoded_package_path_id_with_suffix(path, "/v1/frameworks/", "/rollback")
                    .expect("checked path"),
                framework_registry,
            )
        }
        ("POST", path)
            if decoded_package_path_id_with_suffix(path, "/v1/frameworks/", "/uninstall")
                .is_some() =>
        {
            uninstall_framework(
                &decoded_package_path_id_with_suffix(path, "/v1/frameworks/", "/uninstall")
                    .expect("checked path"),
                framework_registry,
            )
        }
        ("GET", path)
            if decoded_package_path_id_with_suffix(path, "/v1/tools/", "/readiness").is_some() =>
        {
            tool_readiness(
                &decoded_package_path_id_with_suffix(path, "/v1/tools/", "/readiness")
                    .expect("checked path"),
                tool_registry,
                framework_registry,
            )
        }
        ("POST", path) if tool_execute_path_id(path).is_some() => execute_registered_tool(
            &tool_execute_path_id(path).expect("checked path"),
            &request.body,
            mcp_servers,
            tool_registry,
            workflow_store,
            framework_registry,
            run_store,
            control_plane_root,
        ),
        ("GET", "/v1/tools/enabled") => list_enabled_tools(tool_registry),
        ("GET", path) if path_id(path, "/v1/tools/").is_some() => get_tool(
            path_id(path, "/v1/tools/").expect("checked path"),
            tool_registry,
        ),
        ("POST", path) if path_id_with_suffix(path, "/v1/tools/", "/enable").is_some() => {
            set_tool_enabled(
                path_id_with_suffix(path, "/v1/tools/", "/enable").expect("checked path"),
                true,
                tool_registry,
                hook_bridge,
            )
        }
        ("POST", path) if path_id_with_suffix(path, "/v1/tools/", "/disable").is_some() => {
            set_tool_enabled(
                path_id_with_suffix(path, "/v1/tools/", "/disable").expect("checked path"),
                false,
                tool_registry,
                hook_bridge,
            )
        }
        ("PUT", path) if path_id_with_suffix(path, "/v1/tools/", "/defaults").is_some() => {
            update_tool_defaults(
                path_id_with_suffix(path, "/v1/tools/", "/defaults").expect("checked path"),
                &request.body,
                tool_registry,
                hook_bridge,
            )
        }
        ("PUT", path) if decoded_package_path_id(path, "/v1/tools/").is_some() => put_tool(
            &decoded_package_path_id(path, "/v1/tools/").expect("checked path"),
            &request.body,
            tool_registry,
            hook_bridge,
        ),
        ("DELETE", path) if decoded_package_path_id(path, "/v1/tools/").is_some() => delete_tool(
            &decoded_package_path_id(path, "/v1/tools/").expect("checked path"),
            tool_registry,
            hook_bridge,
        ),
        ("GET", "/v1/art-authoring/python/status") => python_engine_status(framework_registry),
        ("GET", "/v1/art-authoring/python/arts") => list_python_arts(),
        ("POST", "/v1/art-authoring/source/read") => read_python_art_source(&request.body),
        ("POST", "/v1/art-authoring/source/read-art-json") => read_python_art_json(&request.body),
        ("POST", "/v1/art-authoring/source/check-art-json") => {
            check_python_art_json_nearby(&request.body)
        }
        ("POST", "/v1/art-authoring/source/infer-ports") => infer_python_art_ports(&request.body),
        ("GET", "/v1/shared-memory/buffers") => list_shared_memory_buffers(shared_images),
        ("POST", "/v1/shared-memory/buffers") => {
            create_shared_memory_buffer(&request.body, shared_images)
        }
        ("GET", path) if path_id(path, "/v1/shared-memory/buffers/").is_some() => {
            get_shared_memory_buffer_info(
                path_id(path, "/v1/shared-memory/buffers/").expect("checked path"),
                shared_images,
            )
        }
        ("DELETE", path) if path_id(path, "/v1/shared-memory/buffers/").is_some() => {
            release_shared_memory_buffer(
                path_id(path, "/v1/shared-memory/buffers/").expect("checked path"),
                shared_images,
            )
        }
        ("GET", "/v1/shared-images") => list_shared_images(shared_images),
        ("POST", "/v1/shared-images") => create_shared_image(&request.body, shared_images),
        ("POST", "/v1/image-helpers/convert") => convert_image_helper(&request.body),
        ("GET", path) if path_id(path, "/v1/shared-images/").is_some() => get_shared_image(
            path_id(path, "/v1/shared-images/").expect("checked path"),
            shared_images,
        ),
        ("DELETE", path) if path_id(path, "/v1/shared-images/").is_some() => delete_shared_image(
            path_id(path, "/v1/shared-images/").expect("checked path"),
            shared_images,
        ),
        ("POST", "/v1/hook-bridge/cache-control") => {
            broadcast_hook_cache_control(&request.body, hook_bridge)
        }
        ("GET", "/v1/settings") => get_settings(settings),
        ("PUT", "/v1/settings") => put_settings(&request.body, settings, hook_bridge),
        _ => route_settings_runs(
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
