// Settings, shortcut and autostart routes plus workflow and canvas persistence.
fn get_settings(settings_store: &SharedLoomSettingsStore) -> Result<(u16, String)> {
    let store = settings_store
        .lock()
        .map_err(|_| anyhow::anyhow!("lock Loom settings"))?;
    Ok((
        200,
        serde_json::to_string(&json!({
            "settings": store.settings,
        }))?,
    ))
}

fn put_settings(
    body: &str,
    settings_store: &SharedLoomSettingsStore,
    hook_bridge: &SharedHookBridgeRuntime,
) -> Result<(u16, String)> {
    let mut settings: LoomSettings = match serde_json::from_str(body) {
        Ok(settings) => settings,
        Err(error) => return invalid_request(error.to_string()),
    };
    settings.appearance_version = CURRENT_APPEARANCE_VERSION;
    if let Err(error) = settings.hook_cache.validate() {
        return invalid_request(error);
    }
    if let Err(error) = settings.mcp.validate() {
        return invalid_request(error);
    }
    if let Err(error) = settings.loom_cache.validate() {
        return invalid_request(error);
    }
    if let Err(error) = settings.network.loom.validate() {
        return invalid_request(error);
    }
    if let Err(error) = settings.network.hook.validate() {
        return invalid_request(error);
    }
    if !matches!(
        settings.system.loom_log_level.as_str(),
        "error" | "warn" | "info" | "debug"
    ) || !matches!(
        settings.system.hook_log_level.as_str(),
        "error" | "warn" | "info" | "debug"
    ) {
        return invalid_request("日志级别必须是 error、warn、info 或 debug");
    }
    let mut store = settings_store
        .lock()
        .map_err(|_| anyhow::anyhow!("lock Loom settings"))?;
    let previous = std::mem::replace(&mut store.settings, settings.clone());
    if let Err(error) = store.save() {
        store.settings = previous;
        return Err(error);
    }
    drop(store);
    apply_runtime_settings(&settings);
    broadcast_hook_bridge_json(
        hook_bridge,
        HookEvent {
            protocol_version: loom_protocol::HOOK_PROTOCOL_VERSION.to_owned(),
            method: HOOK_EVENT_CACHE_CONTROL.to_owned(),
            params: json!({
                "action": "settings",
                "settings": HookCacheSettingsWire::from(&settings.hook_cache),
            }),
        },
    );
    broadcast_settings_updated(hook_bridge, &settings);
    Ok((
        200,
        serde_json::to_string(&json!({
            "settings": settings,
            "saved": true,
        }))?,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HookCacheControlRequest {
    action: String,
}

fn broadcast_hook_cache_control(
    body: &str,
    hook_bridge: &SharedHookBridgeRuntime,
) -> Result<(u16, String)> {
    let request: HookCacheControlRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let action = request.action.trim();
    if !matches!(action, "clearRecycleBin" | "clearReferenceLibrary") {
        return invalid_request("不支持的 Hook 缓存控制操作");
    }
    let broadcast = HookEvent {
        protocol_version: loom_protocol::HOOK_PROTOCOL_VERSION.to_owned(),
        method: HOOK_EVENT_CACHE_CONTROL.to_owned(),
        params: json!({ "action": action }),
    };
    let serialized = serde_json::to_string(&broadcast)?;
    let hub = {
        let runtime = hook_bridge
            .lock()
            .map_err(|_| anyhow::anyhow!("lock hook bridge runtime"))?;
        runtime.broadcast_hub.clone()
    };
    let delivered_clients = broadcast_hook_bridge_messages_with_count(&hub, &[serialized]);
    if delivered_clients == 0 {
        return structured_error(
            409,
            json!({
                "code": "hook_client_not_connected",
                "message": "Hook 当前未连接，无法修改正在使用的回收站或参考图。"
            }),
        );
    }
    Ok((
        200,
        serde_json::to_string(&json!({
            "action": action,
            "deliveredClients": delivered_clients,
        }))?,
    ))
}

fn get_shortcuts(settings_store: &SharedLoomSettingsStore) -> Result<(u16, String)> {
    let store = settings_store
        .lock()
        .map_err(|_| anyhow::anyhow!("lock Loom settings"))?;
    let mut shortcuts = store
        .settings
        .shortcuts
        .values()
        .cloned()
        .collect::<Vec<_>>();
    shortcuts.sort_by(|left, right| left.id.cmp(&right.id));
    Ok((
        200,
        serde_json::to_string(&json!({
            "shortcuts": shortcuts,
        }))?,
    ))
}

fn put_shortcut(
    path_id: &str,
    body: &str,
    settings_store: &SharedLoomSettingsStore,
    hook_bridge: &SharedHookBridgeRuntime,
) -> Result<(u16, String)> {
    let shortcut: LoomShortcutConfig = match serde_json::from_str(body) {
        Ok(shortcut) => shortcut,
        Err(error) => return invalid_request(error.to_string()),
    };
    if shortcut.id != path_id {
        return id_mismatch("shortcut", path_id, &shortcut.id);
    }
    let mut store = settings_store
        .lock()
        .map_err(|_| anyhow::anyhow!("lock Loom settings"))?;
    let previous = store
        .settings
        .shortcuts
        .insert(shortcut.id.clone(), shortcut.clone());
    if let Err(error) = store.save() {
        if let Some(previous) = previous {
            store
                .settings
                .shortcuts
                .insert(shortcut.id.clone(), previous);
        } else {
            store.settings.shortcuts.remove(&shortcut.id);
        }
        return Err(error);
    }
    let settings = store.settings.clone();
    drop(store);
    broadcast_settings_updated(hook_bridge, &settings);
    Ok((
        200,
        serde_json::to_string(&json!({
            "shortcut": shortcut,
            "saved": true,
        }))?,
    ))
}

fn broadcast_settings_updated(hook_bridge: &SharedHookBridgeRuntime, settings: &LoomSettings) {
    broadcast_hook_bridge_json(
        hook_bridge,
        HookEvent {
            protocol_version: loom_protocol::HOOK_PROTOCOL_VERSION.to_owned(),
            method: HOOK_EVENT_SETTINGS_UPDATED.to_owned(),
            params: json!({ "settings": hook_settings_protocol_value(settings) }),
        },
    );
}

fn get_app_paths(data_dir: &Path) -> Result<(u16, String)> {
    let config_dir = data_dir.join("settings");
    let log_dir = std::env::var_os("LOOM_LOG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| data_dir.join("logs"));
    Ok((
        200,
        serde_json::to_string(&json!({
            "dataDir": data_dir.to_string_lossy(),
            "configDir": config_dir.to_string_lossy(),
            "logDir": log_dir.to_string_lossy(),
        }))?,
    ))
}

fn get_autostart(settings_store: &SharedLoomSettingsStore) -> Result<(u16, String)> {
    let store = settings_store
        .lock()
        .map_err(|_| anyhow::anyhow!("lock Loom settings"))?;
    Ok((
        200,
        serde_json::to_string(&json!({
            "enabled": store.settings.general.auto_start,
            "sideEffect": false,
        }))?,
    ))
}

fn set_autostart(body: &str, settings_store: &SharedLoomSettingsStore) -> Result<(u16, String)> {
    let request: ToggleRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    set_autostart_preference(request.enabled, settings_store)
}

fn set_autostart_preference(
    enabled: bool,
    settings_store: &SharedLoomSettingsStore,
) -> Result<(u16, String)> {
    let mut store = settings_store
        .lock()
        .map_err(|_| anyhow::anyhow!("lock Loom settings"))?;
    let previous = store.settings.general.auto_start;
    store.settings.general.auto_start = enabled;
    if let Err(error) = store.save() {
        store.settings.general.auto_start = previous;
        return Err(error);
    }
    Ok((
        200,
        serde_json::to_string(&json!({
            "enabled": enabled,
            "sideEffect": false,
            "message": "Loom saved the requested autostart preference but did not mutate Windows startup entries.",
        }))?,
    ))
}

fn set_minimize_to_tray(
    body: &str,
    settings_store: &SharedLoomSettingsStore,
) -> Result<(u16, String)> {
    let request: ToggleRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let mut store = settings_store
        .lock()
        .map_err(|_| anyhow::anyhow!("lock Loom settings"))?;
    let previous = store.settings.general.minimize_to_tray;
    store.settings.general.minimize_to_tray = request.enabled;
    if let Err(error) = store.save() {
        store.settings.general.minimize_to_tray = previous;
        return Err(error);
    }
    Ok((
        200,
        serde_json::to_string(&json!({
            "enabled": request.enabled,
            "sideEffect": false,
        }))?,
    ))
}

fn workflow_timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().to_string())
        .unwrap_or_else(|_| "0".to_owned())
}

fn list_workflows(workflow_store: &WorkflowStore) -> Result<(u16, String)> {
    let workflows = match workflow_store.list_workflows() {
        Ok(workflows) => workflows,
        Err(error) => return workflow_store_error_response(error),
    };
    Ok((
        200,
        serde_json::to_string(&json!({ "workflows": workflows }))?,
    ))
}

fn get_workflow(path_id: &str, workflow_store: &WorkflowStore) -> Result<(u16, String)> {
    let data = match workflow_store.load_workflow(path_id) {
        Ok(data) => data,
        Err(error) => return workflow_store_error_response(error),
    };
    let metadata = workflow_store.list_workflows().ok().and_then(|workflows| {
        workflows
            .into_iter()
            .find(|workflow| workflow.id == path_id)
    });
    let workflow = match metadata {
        Some(metadata) => {
            let mut value = serde_json::to_value(metadata)?;
            if let Some(object) = value.as_object_mut() {
                object.insert("data".to_owned(), json!(data));
            }
            value
        }
        None => json!({
            "id": path_id,
            "name": path_id,
            "nodeCount": 0,
            "updatedAt": "",
            "data": data,
        }),
    };

    Ok((
        200,
        serde_json::to_string(&json!({ "workflow": workflow }))?,
    ))
}

fn put_workflow(
    path_id: &str,
    body: &str,
    workflow_store: &WorkflowStore,
) -> Result<(u16, String)> {
    let request = match serde_json::from_str::<PutWorkflowRequest>(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let workflow = match workflow_store.save_workflow(path_id, &request.data) {
        Ok(workflow) => workflow,
        Err(error) => return workflow_store_error_response(error),
    };
    Ok((
        200,
        serde_json::to_string(&json!({ "workflow": workflow }))?,
    ))
}
