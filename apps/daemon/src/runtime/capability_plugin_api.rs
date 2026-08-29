// Authenticated lifecycle API for independently installable Capability Plugins.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InstallCapabilityRequest {
    zip_base64: String,
}

const MAX_LOCAL_CAPABILITY_ZIP_BASE64_BYTES: usize = 12 * 1024 * 1024;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SelectCapabilityVersionRequest {
    #[serde(default)]
    digest: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ApproveCapabilityRequest {
    digest: String,
    permissions: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UpdateCapabilityConfigRequest {
    expected_revision: u64,
    values: serde_json::Map<String, Value>,
}

fn route_capability_plugins(
    request: &ParsedHttpRequest,
    route_path: &str,
    control_plane_root: &Path,
    runtime: &SharedCapabilityRuntime,
    resources: &SharedCapabilityResourceBroker,
    hook_bridge: &SharedHookBridgeRuntime,
) -> Option<Result<(u16, String)>> {
    const PREFIX: &str = "/v1/capability-plugins/";
    if route_path != "/v1/capability-plugins" && !route_path.starts_with(PREFIX) {
        return None;
    }
    // Lifecycle transitions include process and resource side effects in addition
    // to atomic registry writes. Serialize the complete transition so a competing
    // request cannot make compensation restore stale state.
    let _operation_guard = match capability_plugin_operation_lock().lock() {
        Ok(guard) => guard,
        Err(_) => return Some(Err(anyhow::anyhow!("capability lifecycle lock is unavailable"))),
    };
    let previous_generation = runtime
        .contribution_snapshot()
        .ok()
        .map(|snapshot| snapshot.generation);
    let response = match (request.method.as_str(), route_path) {
        ("GET", "/v1/capability-plugins/extensions") => capability_extension_snapshot(runtime),
        ("GET", "/v1/capability-plugins") => list_capability_plugins(control_plane_root),
        ("GET", "/v1/capability-plugins/grants") => {
            list_capability_grants(control_plane_root)
        }
        ("GET", "/v1/capability-plugins/catalog") => {
            list_capability_catalog(control_plane_root, hook_bridge)
        }
        ("POST", "/v1/capability-plugins/catalog/install") => {
            install_capability_catalog_plugin(&request.body, control_plane_root, hook_bridge)
        }
        ("POST", "/v1/capability-plugins/install") => {
            install_capability_plugin(&request.body, control_plane_root)
        }
        ("POST", path)
            if decoded_package_path_id_with_suffix(path, PREFIX, "/approve").is_some() =>
        {
            approve_capability_plugin(
                &decoded_package_path_id_with_suffix(path, PREFIX, "/approve")
                    .expect("checked capability path"),
                &request.body,
                control_plane_root,
            )
        }
        ("POST", path)
            if decoded_package_path_id_with_suffix(path, PREFIX, "/enable").is_some() =>
        {
            enable_capability_plugin(
                &decoded_package_path_id_with_suffix(path, PREFIX, "/enable")
                    .expect("checked capability path"),
                &request.body,
                control_plane_root,
                runtime,
            )
        }
        ("POST", path)
            if decoded_package_path_id_with_suffix(path, PREFIX, "/disable").is_some() =>
        {
            disable_capability_plugin(
                &decoded_package_path_id_with_suffix(path, PREFIX, "/disable")
                    .expect("checked capability path"),
                control_plane_root,
                runtime,
                resources,
            )
        }
        ("POST", path)
            if decoded_package_path_id_with_suffix(path, PREFIX, "/upgrade").is_some() =>
        {
            upgrade_capability_plugin(
                &decoded_package_path_id_with_suffix(path, PREFIX, "/upgrade")
                    .expect("checked capability path"),
                &request.body,
                control_plane_root,
                runtime,
            )
        }
        ("POST", path)
            if decoded_package_path_id_with_suffix(path, PREFIX, "/rollback").is_some() =>
        {
            rollback_capability_plugin(
                &decoded_package_path_id_with_suffix(path, PREFIX, "/rollback")
                    .expect("checked capability path"),
                control_plane_root,
                runtime,
            )
        }
        ("POST", path)
            if decoded_package_path_id_with_suffix(path, PREFIX, "/uninstall").is_some() =>
        {
            uninstall_capability_plugin(
                &decoded_package_path_id_with_suffix(path, PREFIX, "/uninstall")
                    .expect("checked capability path"),
                control_plane_root,
                runtime,
                resources,
            )
        }
        ("GET", path)
            if decoded_package_path_id_with_suffix(path, PREFIX, "/config").is_some() =>
        {
            get_capability_config(
                &decoded_package_path_id_with_suffix(path, PREFIX, "/config")
                    .expect("checked capability path"),
                control_plane_root,
            )
        }
        ("PUT", path)
            if decoded_package_path_id_with_suffix(path, PREFIX, "/config").is_some() =>
        {
            update_capability_config(
                &decoded_package_path_id_with_suffix(path, PREFIX, "/config")
                    .expect("checked capability path"),
                &request.body,
                control_plane_root,
            )
        }
        _ => return None,
    };
    if response.as_ref().is_ok_and(|(status, _)| *status < 400) {
        if let Ok(snapshot) = runtime.contribution_snapshot() {
            if previous_generation.is_some_and(|generation| generation != snapshot.generation) {
                broadcast_hook_bridge_json(hook_bridge, extension_snapshot_event(snapshot));
            }
        }
    }
    Some(response)
}

fn capability_extension_snapshot(runtime: &SharedCapabilityRuntime) -> Result<(u16, String)> {
    match runtime.contribution_snapshot() {
        Ok(snapshot) => Ok((
            200,
            serde_json::to_string(&json!({ "snapshot": snapshot }))?,
        )),
        Err(error) => capability_runtime_error_response(error),
    }
}

fn list_capability_plugins(control_plane_root: &Path) -> Result<(u16, String)> {
    let registry = capability_registry(control_plane_root)?;
    capability_response(registry.list().map(|plugins| {
        let disk_bytes = capability_plugin_disk_bytes(&registry, &plugins);
        json!({ "plugins": plugins, "diskBytesByPlugin": disk_bytes })
    }))
}

fn capability_plugin_operation_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn list_capability_grants(control_plane_root: &Path) -> Result<(u16, String)> {
    let store = loom_tool_registry::capability::CapabilityGrantStore::new(control_plane_root);
    capability_response(store.list().map(|grants| json!({ "grants": grants })))
}

fn install_capability_plugin(body: &str, control_plane_root: &Path) -> Result<(u16, String)> {
    let request: InstallCapabilityRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(error) => return capability_bad_request("invalid_capability_install", error.to_string()),
    };
    if request.zip_base64.len() > MAX_LOCAL_CAPABILITY_ZIP_BASE64_BYTES {
        return capability_bad_request(
            "invalid_capability_install",
            "local capability package exceeds the upload limit".to_owned(),
        );
    }
    let zip = match loom_image_io::decode_data_url_bytes(&request.zip_base64) {
        Ok(zip) => zip,
        Err(error) => {
            return capability_bad_request(
                "invalid_capability_install",
                format!("decode capability package: {error}"),
            )
        }
    };
    let registry = capability_registry(control_plane_root)?;
    capability_response(
        loom_tool_registry::capability::install_capability_from_zip(&zip, &registry)
            .map(|report| json!({ "package": report })),
    )
}

fn approve_capability_plugin(
    qualified_id: &str,
    body: &str,
    control_plane_root: &Path,
) -> Result<(u16, String)> {
    let request: ApproveCapabilityRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(error) => return capability_bad_request("invalid_capability_approval", error.to_string()),
    };
    let registry = capability_registry(control_plane_root)?;
    let grants = loom_tool_registry::capability::CapabilityGrantStore::new(control_plane_root);
    capability_response(
        registry
            .approve_permissions(
                &grants,
                qualified_id,
                &request.digest,
                &request.permissions,
            )
            .map(|()| json!({ "approved": true })),
    )
}

fn enable_capability_plugin(
    qualified_id: &str,
    body: &str,
    control_plane_root: &Path,
    runtime: &SharedCapabilityRuntime,
) -> Result<(u16, String)> {
    let request: SelectCapabilityVersionRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(error) => return capability_bad_request("invalid_capability_enable", error.to_string()),
    };
    let registry = capability_registry(control_plane_root)?;
    let grants = loom_tool_registry::capability::CapabilityGrantStore::new(control_plane_root);
    let previous = match required_capability_record(&registry, qualified_id) {
        Ok(record) => record,
        Err(error) => return capability_error_response(error),
    };
    let updated = match registry.enable(&grants, qualified_id, request.digest.as_deref()) {
        Ok(record) => record,
        Err(error) => return capability_error_response(error),
    };
    activate_committed_record(&registry, runtime, previous, updated)
}

fn disable_capability_plugin(
    qualified_id: &str,
    control_plane_root: &Path,
    runtime: &SharedCapabilityRuntime,
    resources: &SharedCapabilityResourceBroker,
) -> Result<(u16, String)> {
    let registry = capability_registry(control_plane_root)?;
    let previous = match required_capability_record(&registry, qualified_id) {
        Ok(record) => record,
        Err(error) => return capability_error_response(error),
    };
    if let Err(error) = runtime.deactivate(qualified_id) {
        return capability_runtime_error_response(error);
    }
    resources.release_plugin(qualified_id);
    match registry.disable(qualified_id) {
        Ok(plugin) => Ok((200, serde_json::to_string(&json!({ "plugin": plugin }))?)),
        Err(error) => {
            restore_runtime_record(&registry, runtime, &previous);
            capability_error_response(error)
        }
    }
}

fn upgrade_capability_plugin(
    qualified_id: &str,
    body: &str,
    control_plane_root: &Path,
    runtime: &SharedCapabilityRuntime,
) -> Result<(u16, String)> {
    let request: SelectCapabilityVersionRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(error) => return capability_bad_request("invalid_capability_upgrade", error.to_string()),
    };
    let Some(digest) = request.digest else {
        return capability_bad_request(
            "invalid_capability_upgrade",
            "upgrade requires an installed package digest".to_owned(),
        );
    };
    let registry = capability_registry(control_plane_root)?;
    let grants = loom_tool_registry::capability::CapabilityGrantStore::new(control_plane_root);
    let previous = match required_capability_record(&registry, qualified_id) {
        Ok(record) => record,
        Err(error) => return capability_error_response(error),
    };
    let updated = match registry.upgrade(&grants, qualified_id, &digest) {
        Ok(record) => record,
        Err(error) => return capability_error_response(error),
    };
    activate_committed_record(&registry, runtime, previous, updated)
}

fn rollback_capability_plugin(
    qualified_id: &str,
    control_plane_root: &Path,
    runtime: &SharedCapabilityRuntime,
) -> Result<(u16, String)> {
    let registry = capability_registry(control_plane_root)?;
    let grants = loom_tool_registry::capability::CapabilityGrantStore::new(control_plane_root);
    let previous = match required_capability_record(&registry, qualified_id) {
        Ok(record) => record,
        Err(error) => return capability_error_response(error),
    };
    let updated = match registry.rollback(&grants, qualified_id) {
        Ok(record) => record,
        Err(error) => return capability_error_response(error),
    };
    activate_committed_record(&registry, runtime, previous, updated)
}

fn uninstall_capability_plugin(
    qualified_id: &str,
    control_plane_root: &Path,
    runtime: &SharedCapabilityRuntime,
    resources: &SharedCapabilityResourceBroker,
) -> Result<(u16, String)> {
    let registry = capability_registry(control_plane_root)?;
    let previous = match required_capability_record(&registry, qualified_id) {
        Ok(record) => record,
        Err(error) => return capability_error_response(error),
    };
    if let Err(error) = runtime.deactivate(qualified_id) {
        return capability_runtime_error_response(error);
    }
    resources.release_plugin(qualified_id);
    let grants = loom_tool_registry::capability::CapabilityGrantStore::new(control_plane_root);
    let config = loom_tool_registry::capability::CapabilityConfigStore::new(control_plane_root);
    match registry.uninstall(&grants, &config, qualified_id) {
        Ok(plugin) => Ok((
            200,
            serde_json::to_string(&json!({ "plugin": plugin, "uninstalled": true }))?,
        )),
        Err(error) => {
            restore_runtime_record(&registry, runtime, &previous);
            capability_error_response(error)
        }
    }
}

fn get_capability_config(
    qualified_id: &str,
    control_plane_root: &Path,
) -> Result<(u16, String)> {
    let store = loom_tool_registry::capability::CapabilityConfigStore::new(control_plane_root);
    capability_response(
        store
            .read(qualified_id)
            .map(|config| json!({ "config": config })),
    )
}

fn update_capability_config(
    qualified_id: &str,
    body: &str,
    control_plane_root: &Path,
) -> Result<(u16, String)> {
    let request: UpdateCapabilityConfigRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(error) => return capability_bad_request("invalid_capability_config", error.to_string()),
    };
    let store = loom_tool_registry::capability::CapabilityConfigStore::new(control_plane_root);
    capability_response(
        store
            .write(qualified_id, request.expected_revision, request.values)
            .map(|config| json!({ "config": config })),
    )
}

fn capability_registry(
    control_plane_root: &Path,
) -> Result<loom_tool_registry::capability::CapabilityPluginRegistry> {
    let registry =
        loom_tool_registry::capability::CapabilityPluginRegistry::new(control_plane_root);
    registry
        .recover_capability_lifecycle()
        .map_err(anyhow::Error::from)?;
    Ok(registry)
}

fn capability_response(
    result: std::result::Result<
        Value,
        loom_tool_registry::capability::CapabilityInstallError,
    >,
) -> Result<(u16, String)> {
    match result {
        Ok(value) => Ok((200, serde_json::to_string(&value)?)),
        Err(error) => capability_error_response(error),
    }
}

fn capability_error_response(
    error: loom_tool_registry::capability::CapabilityInstallError,
) -> Result<(u16, String)> {
    use loom_tool_registry::capability::CapabilityInstallError as Error;
    let (status, code, message) = match error {
        Error::NotFound(message) => (404, "capability_not_found", message),
        Error::Conflict(message) => (409, "capability_conflict", message),
        Error::PermissionRequired(message) => (409, "capability_permission_required", message),
        Error::InvalidPackage(message) => (400, "invalid_capability_package", message),
        Error::InvalidState(message) => (400, "invalid_capability_state", message),
        Error::InvalidRegistry(_) | Error::Io(_) | Error::Json(_) => (
            500,
            "capability_store_failed",
            "capability control-plane operation failed".to_owned(),
        ),
    };
    structured_error(status, json!({ "code": code, "message": message }))
}

fn capability_bad_request(code: &str, message: String) -> Result<(u16, String)> {
    structured_error(400, json!({ "code": code, "message": message }))
}
