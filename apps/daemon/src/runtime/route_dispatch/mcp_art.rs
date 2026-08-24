// MCP, plugin trust, publisher identity, and Art management routes.
#[allow(clippy::too_many_arguments, unused_variables)]
fn route_mcp_art(
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
        ("GET", "/v1/mcp/servers") => {
            list_mcp_servers(mcp_servers, tool_registry, control_plane_root)
        }
        ("GET", "/v1/mcp/registry") => fetch_mcp_registry(
            &request.path,
            mcp_registry_endpoint,
            &mcp_registry_cache_path(control_plane_root),
        ),
        ("POST", "/v1/mcp/test") => test_mcp_connection(&request.body),
        ("POST", "/v1/mcp/call") => call_mcp_tool(&request.body),
        ("POST", "/v1/mcp/package/check") => check_mcp_package_installed(&request.body),
        ("POST", "/v1/mcp/package/install-plan") => build_mcp_package_install_plan(&request.body),
        ("POST", "/v1/mcp/servers/install") => install_mcp_server_package(
            &request.body,
            mcp_servers,
            control_plane_root,
            &mcp_server_store_path(control_plane_root),
        ),
        ("PUT", path)
            if path_id_with_suffix(path, "/v1/mcp/servers/", "/credentials").is_some() =>
        {
            let id = path_id_with_suffix(path, "/v1/mcp/servers/", "/credentials")
                .expect("checked path");
            update_mcp_server_credentials(
                id,
                &request.body,
                mcp_servers,
                control_plane_root,
                &mcp_server_store_path(control_plane_root),
            )
        }
        ("PUT", path) if path_id_with_suffix(path, "/v1/mcp/servers/", "/enabled").is_some() => {
            let id =
                path_id_with_suffix(path, "/v1/mcp/servers/", "/enabled").expect("checked path");
            set_mcp_server_enabled(
                id,
                &request.body,
                mcp_servers,
                &mcp_server_store_path(control_plane_root),
            )
        }
        ("POST", path) if path_id_with_suffix(path, "/v1/mcp/servers/", "/test").is_some() => {
            let id = path_id_with_suffix(path, "/v1/mcp/servers/", "/test").expect("checked path");
            test_installed_mcp_server(id, mcp_servers, control_plane_root)
        }
        ("PUT", path) if path_id(path, "/v1/mcp/servers/").is_some() => put_mcp_server(
            path_id(path, "/v1/mcp/servers/").expect("checked path"),
            &request.body,
            mcp_servers,
            &mcp_server_store_path(control_plane_root),
        ),
        ("DELETE", path) if path_id(path, "/v1/mcp/servers/").is_some() => delete_mcp_server(
            path_id(path, "/v1/mcp/servers/").expect("checked path"),
            mcp_servers,
            &mcp_server_store_path(control_plane_root),
            control_plane_root,
        ),
        ("GET", "/v1/tools") => list_tools(tool_registry),
        ("GET", "/v1/frameworks") => list_frameworks(framework_registry),
        ("GET", "/v1/doctor/frameworks") => framework_doctor(framework_registry),
        ("GET", "/v1/doctor/arts") => {
            art_doctor(tool_registry, framework_registry, control_plane_root)
        }
        ("GET", path) if path.split('?').next() == Some("/v1/support-bundle") => support_bundle(
            path,
            hook_settings,
            run_store,
            run_store_status,
            tool_registry,
            framework_registry,
            control_plane_root,
        ),
        ("GET", "/v1/plugin-trust") => list_plugin_trust(framework_registry),
        ("POST", "/v1/plugin-trust/publishers") => {
            trust_plugin_publisher(&request.body, framework_registry)
        }
        ("POST", "/v1/plugin-trust/revoke") => {
            revoke_plugin_publisher(&request.body, framework_registry)
        }
        ("POST", "/v1/plugin-trust/policy") => {
            set_plugin_trust_policy(&request.body, framework_registry)
        }
        ("POST", "/v1/plugin-trust/users") => trust_plugin_user(&request.body, framework_registry),
        ("POST", "/v1/plugin-trust/users/remove") => {
            untrust_plugin_user(&request.body, framework_registry)
        }
        ("GET", "/v1/publisher-identity") => list_publisher_identity(control_plane_root),
        ("POST", "/v1/publisher-identity/register") => {
            register_publisher_identity(&request.body, control_plane_root)
        }
        ("POST", "/v1/publisher-identity/rotate") => {
            rotate_publisher_identity(&request.body, control_plane_root)
        }
        ("POST", "/v1/publisher-identity/private-key") => {
            reveal_publisher_private_key(control_plane_root)
        }
        ("GET", "/v1/plugin-credentials") => list_plugin_credentials(control_plane_root),
        ("POST", "/v1/plugin-credentials") => {
            save_plugin_credential(&request.body, control_plane_root)
        }
        ("POST", "/v1/plugin-credentials/delete") => {
            delete_plugin_credential(&request.body, control_plane_root)
        }
        ("POST", "/v1/plugin-credentials/reveal") => {
            reveal_plugin_credential(&request.body, control_plane_root)
        }
        ("POST", "/v1/frameworks/install") => {
            install_framework_package(&request.body, framework_registry)
        }
        ("POST", "/v1/arts/install") => install_art(
            &request.body,
            tool_registry,
            framework_registry,
            workflow_store,
            control_plane_root,
            hook_bridge,
            bundled_art_sha256_allowlist,
        ),
        ("POST", "/v1/arts/create") => create_authored_art(
            &request.body,
            tool_registry,
            framework_registry,
            control_plane_root,
            hook_bridge,
        ),
        ("GET", path)
            if decoded_package_path_id_with_suffix(path, "/v1/arts/", "/management").is_some() =>
        {
            get_art_management(
                &decoded_package_path_id_with_suffix(path, "/v1/arts/", "/management")
                    .expect("checked path"),
                tool_registry,
                control_plane_root,
            )
        }
        ("PUT", path)
            if decoded_package_path_id_with_suffix(path, "/v1/arts/", "/settings").is_some() =>
        {
            put_art_management_settings(
                &decoded_package_path_id_with_suffix(path, "/v1/arts/", "/settings")
                    .expect("checked path"),
                &request.body,
                tool_registry,
                control_plane_root,
                hook_bridge,
            )
        }
        ("POST", path)
            if decoded_package_path_id_with_suffix(path, "/v1/arts/", "/update").is_some() =>
        {
            update_art_version(
                &decoded_package_path_id_with_suffix(path, "/v1/arts/", "/update")
                    .expect("checked path"),
                &request.body,
                tool_registry,
                framework_registry,
                workflow_store,
                control_plane_root,
                hook_bridge,
            )
        }
        ("POST", "/v1/arts/auto-update") => auto_update_arts(
            tool_registry,
            framework_registry,
            workflow_store,
            control_plane_root,
            hook_bridge,
            settings,
        ),
        ("POST", path)
            if decoded_package_path_id_with_suffix(path, "/v1/arts/", "/rollback").is_some() =>
        {
            rollback_art(
                &decoded_package_path_id_with_suffix(path, "/v1/arts/", "/rollback")
                    .expect("checked path"),
                tool_registry,
                framework_registry,
                workflow_store,
                control_plane_root,
                hook_bridge,
            )
        }
        ("POST", path)
            if decoded_package_path_id_with_suffix(path, "/v1/arts/", "/uninstall").is_some() =>
        {
            uninstall_art(
                &decoded_package_path_id_with_suffix(path, "/v1/arts/", "/uninstall")
                    .expect("checked path"),
                &request.body,
                tool_registry,
                control_plane_root,
                hook_bridge,
                mcp_servers,
                &mcp_server_store_path(control_plane_root),
            )
        }
        ("GET", path) if path.split('?').next() == Some("/v1/arts/store/catalog") => {
            fetch_art_store_catalog(path)
        }
        ("POST", "/v1/arts/store/install") => install_art_from_store(
            &request.body,
            tool_registry,
            framework_registry,
            workflow_store,
            control_plane_root,
            hook_bridge,
        ),
        ("GET", path)
            if decoded_package_path_id_with_suffix(path, "/v1/arts/", "/package").is_some() =>
        {
            package_art(
                &decoded_package_path_id_with_suffix(path, "/v1/arts/", "/package")
                    .expect("checked path"),
                tool_registry,
                control_plane_root,
            )
        }
        _ => route_framework_tools(
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
