// 云端来源不授予额外信任：转给与手动安装相同的宿主生命周期入口。
#[allow(clippy::too_many_arguments)]
fn install_asset_library_package(
    package: asset_library::InstallPackage,
    control_plane_root: &Path,
    capability_runtime: &SharedCapabilityRuntime,
    capability_resources: &SharedCapabilityResourceBroker,
    hook_bridge: &SharedOtterBridgeRuntime,
    tool_registry: &ToolRegistry,
    framework_registry: &FrameworkRegistry,
    workflow_store: &WorkflowStore,
    mcp_servers: &SharedMcpServerStore,
) -> Result<(u16, String)> {
    if package.kind == "application" {
        return match asset_library::application::install(control_plane_root, &package) {
            Ok(installation) => Ok((
                200,
                serde_json::to_string(
                    &json!({"download":package.download, "installation":installation}),
                )?,
            )),
            Err(_) => structured_error(
                400,
                json!({"code":"asset_library_host_install_rejected", "message":"application package rejected"}),
            ),
        };
    }
    let path = if package.kind == "capability" {
        "/v1/capability-plugins/install"
    } else if package.kind == "mcp" {
        "/v1/mcp/servers/install"
    } else {
        "/v1/arts/install"
    };
    let request = ParsedHttpRequest {
        method: "POST".into(),
        path: path.into(),
        headers: vec![],
        body: serde_json::to_string(&json!({"zipBase64": BASE64.encode(&package.bytes)}))?,
    };
    let response = if package.kind == "capability" {
        // 复用原路由的变更锁和清单广播；安装后不自动批准权限或启用。
        route_capability_plugins(
            &request,
            path,
            control_plane_root,
            capability_runtime,
            capability_resources,
            hook_bridge,
        )
        .expect("fixed capability install route")?
    } else if package.kind == "mcp" {
        // 复用宿主验签和配置持久化；更新保留已有启用状态与凭据绑定。
        install_mcp_server_package(
            &request.body,
            mcp_servers,
            control_plane_root,
            &mcp_server_store_path(control_plane_root),
        )?
    } else {
        // 请求不含 bundledCatalog，且 allowlist 显式为空，不能走内置包信任旁路。
        install_art(
            &request.body,
            tool_registry,
            framework_registry,
            workflow_store,
            control_plane_root,
            hook_bridge,
            &BTreeSet::new(),
        )?
    };
    if response.0 >= 400 {
        let payload: Value = serde_json::from_str(&response.1)?;
        let code = if payload["error"]["code"] == "framework_not_ready" {
            "asset_library_framework_required"
        } else {
            "asset_library_host_install_rejected"
        };
        return structured_error(
            response.0,
            json!({"code":code, "message":code,
            "hostCode":payload["error"]["code"]}),
        );
    }
    Ok((
        response.0,
        serde_json::to_string(&json!({
            "download": package.download,
            "installation": serde_json::from_str::<Value>(&response.1)?,
        }))?,
    ))
}
