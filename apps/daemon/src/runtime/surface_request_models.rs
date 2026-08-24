// Device mutations, app and route helpers, and Surface request models.
fn approve_managed_device(
    device_id: &str,
    device_registry: &SharedDeviceRegistryStore,
    hook_bridge: &SharedHookBridgeRuntime,
) -> Result<(u16, String)> {
    {
        let mut store = device_registry
            .lock()
            .map_err(|_| anyhow::anyhow!("device registry is unavailable"))?;
        let Some(device) = store.devices.get_mut(device_id) else {
            return structured_error(
                404,
                json!({"code": "device_not_found", "message": "device was not found"}),
            );
        };
        let previous = device.approval.clone();
        let previous_epoch = device.session_epoch;
        device.approval = "approved".to_owned();
        device.session_epoch = device.session_epoch.saturating_add(1);
        if let Err(error) = store.persist() {
            if let Some(device) = store.devices.get_mut(device_id) {
                device.approval = previous;
                device.session_epoch = previous_epoch;
            }
            return Err(error);
        }
    }
    managed_devices_response(device_registry, hook_bridge)
}

fn update_managed_device(
    device_id: &str,
    body: &str,
    device_registry: &SharedDeviceRegistryStore,
    hook_bridge: &SharedHookBridgeRuntime,
) -> Result<(u16, String)> {
    let input = match serde_json::from_str::<ManagedDeviceUpdate>(body) {
        Ok(input) => input,
        Err(error) => return invalid_request(format!("invalid device update: {error}")),
    };
    let name = input.name.trim();
    let address = input.address.trim();
    if name.is_empty() || name.len() > 80 {
        return invalid_request("device name must contain 1 to 80 characters");
    }
    if address.is_empty() || address.len() > 240 {
        return invalid_request("device address must contain 1 to 240 characters");
    }
    {
        let mut store = device_registry
            .lock()
            .map_err(|_| anyhow::anyhow!("device registry is unavailable"))?;
        let Some(previous) = store.devices.get(device_id).cloned() else {
            return structured_error(
                404,
                json!({"code": "device_not_found", "message": "device was not found"}),
            );
        };
        if previous.is_local {
            return invalid_request("the Loom host device cannot be edited or disabled");
        }
        if let Some(device) = store.devices.get_mut(device_id) {
            device.name = name.to_owned();
            device.kind = input.kind;
            device.address = address.to_owned();
            if device.enabled != input.enabled {
                device.session_epoch = device.session_epoch.saturating_add(1);
            }
            device.enabled = input.enabled;
        }
        if !input.enabled {
            store.revoke_device_sessions(device_id);
        }
        if let Err(error) = store.persist() {
            store.devices.insert(device_id.to_owned(), previous);
            return Err(error);
        }
    }
    managed_devices_response(device_registry, hook_bridge)
}

fn remove_managed_device(
    device_id: &str,
    device_registry: &SharedDeviceRegistryStore,
    hook_bridge: &SharedHookBridgeRuntime,
) -> Result<(u16, String)> {
    {
        let mut store = device_registry
            .lock()
            .map_err(|_| anyhow::anyhow!("device registry is unavailable"))?;
        if store
            .devices
            .get(device_id)
            .is_some_and(|device| device.is_local)
        {
            return invalid_request("the Loom host device cannot be removed");
        }
        let Some(removed) = store.devices.remove(device_id) else {
            return structured_error(
                404,
                json!({"code": "device_not_found", "message": "device was not found"}),
            );
        };
        store.revoke_device_sessions(device_id);
        if let Err(error) = store.persist() {
            store.devices.insert(device_id.to_owned(), removed);
            return Err(error);
        }
    }
    managed_devices_response(device_registry, hook_bridge)
}

fn configuration_claim(
    app: &str,
    registry: &ConfigRegistry,
    settings_base_url: &str,
) -> Result<(u16, String)> {
    let app_id = match app.parse::<ManagedAppId>() {
        Ok(app_id) => app_id,
        Err(error) => {
            return structured_error(
                404,
                json!({
                    "code": managed_config_error_code(error.code()),
                    "message": error.message(),
                }),
            );
        }
    };
    let Some(adapter) = registry.get(app_id) else {
        return structured_error(
            404,
            json!({
                "code": "unknown_app",
                "message": format!("unknown managed app: {app_id}"),
            }),
        );
    };
    let managed = managed_app_set().contains(app_id);
    let panel_url = managed.then(|| {
        let (base, query) = settings_base_url
            .split_once('?')
            .map_or((settings_base_url, None), |(base, query)| {
                (base, Some(query))
            });
        let panel = format!("{}/{}", base.trim_end_matches('/'), app_id);
        query.map_or(panel.clone(), |query| format!("{panel}?{query}"))
    });

    Ok((
        200,
        serde_json::to_string(&json!({
            "app": app_id,
            "managed": managed,
            "owner": if managed { "loom" } else { app_id.as_str() },
            "source": if managed { "loom-managed" } else { "local" },
            "panel_url": panel_url,
            "reason": if managed { "Loom manages this app configuration" } else { "Loom has not claimed this app configuration" },
            "schema_version": adapter.schema_version(),
        }))?,
    ))
}

fn managed_app_set() -> ManagedAppSet {
    ManagedAppSet::parse(&std::env::var("LOOM_MANAGED_CONFIG_APPS").unwrap_or_default())
}

fn app_from_path(path: &str, prefix: &str) -> Option<ManagedAppId> {
    path.strip_prefix(prefix)?.parse().ok()
}

fn path_id<'a>(path: &'a str, prefix: &str) -> Option<&'a str> {
    let id = path.strip_prefix(prefix)?;
    (!id.is_empty() && !id.contains('/')).then_some(id)
}

fn path_id_with_suffix<'a>(path: &'a str, prefix: &str, suffix: &str) -> Option<&'a str> {
    let id = path.strip_prefix(prefix)?.strip_suffix(suffix)?;
    (!id.is_empty() && !id.contains('/')).then_some(id)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateSurfaceInstanceRequest {
    art_id: String,
    #[serde(default)]
    expected_version: Option<String>,
    #[serde(default)]
    state_schema_version: Option<u32>,
    #[serde(default = "default_surface_persistence")]
    persistence: SurfaceInstancePersistence,
}

fn default_surface_persistence() -> SurfaceInstancePersistence {
    SurfaceInstancePersistence::Persistent
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AttachSurfaceInstanceRequest {
    hook_node_id: String,
    device_id: String,
    #[serde(default)]
    capabilities: Option<SurfaceHostCapabilities>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AttachAndMountSurfaceRequest {
    art_id: String,
    hook_node_id: String,
    device_id: String,
    #[serde(default)]
    capabilities: Option<SurfaceHostCapabilities>,
    #[serde(default)]
    expected_version: Option<String>,
    #[serde(default)]
    state_schema_version: Option<u32>,
    #[serde(default = "default_surface_persistence")]
    persistence: SurfaceInstancePersistence,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BeginSurfaceGenerationRequest {
    #[serde(default)]
    expected_generation: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MountSurfaceInstanceRequest {
    attachment_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MigrateSurfaceInstanceRequest {
    target_version: String,
    target_digest: String,
    #[serde(default)]
    expected_generation: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SurfaceStateMigrationFile {
    from: u32,
    to: u32,
    #[serde(default)]
    state_patch: Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateSurfaceResourceRequest {
    kind: SurfaceResourceKind,
    mime: String,
    data_base64: String,
    #[serde(default)]
    width: Option<u32>,
    #[serde(default)]
    height: Option<u32>,
    #[serde(default)]
    lease_millis: Option<u64>,
    #[serde(default)]
    preferred_transport: Option<SurfaceResourceTransportKind>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeclarativeSurfaceDocument {
    #[serde(default)]
    protocol_version: Option<String>,
    scene: SurfaceNode,
    #[serde(default)]
    authoritative_state: Value,
    #[serde(default)]
    resources: Vec<SurfaceResourceDescriptor>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum DeclarativeSurfaceFile {
    Document(DeclarativeSurfaceDocument),
    Scene(SurfaceNode),
}
