// Art update policy, signatures, publisher trust, and local publisher identity models.
fn manual_art_credential_name(art_id: &str, alias: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(art_id.as_bytes());
    hasher.update([0]);
    hasher.update(alias.as_bytes());
    let digest = format!("{:x}", hasher.finalize());
    format!("loom-art-secret-{}", &digest[..24])
}

fn update_art_version(
    art_id: &str,
    body: &str,
    tool_registry: &ToolRegistry,
    framework_registry: &FrameworkRegistry,
    workflow_store: &WorkflowStore,
    control_plane_root: &Path,
    hook_bridge: &SharedHookBridgeRuntime,
) -> Result<(u16, String)> {
    let request: UpdateArtVersionRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    if semver::Version::parse(request.version.trim()).is_err() {
        return invalid_request("目标版本必须是有效的 SemVer");
    }
    let tool = match tool_registry.get_tool(art_id) {
        Ok(Some(tool)) => tool,
        Ok(None) => {
            return structured_error(
                404,
                json!({ "code": "tool_not_found", "message": format!("Art `{art_id}` 不存在") }),
            )
        }
        Err(error) => return tool_registry_error_response(error),
    };
    let identity = tool.qualified_id();
    let settings_store = ArtSettingsStore::new(control_plane_root);
    let settings = match settings_store.get(&identity) {
        Ok(settings) => settings,
        Err(error) => {
            return structured_error(
                500,
                json!({ "code": "art_settings", "message": error.to_string() }),
            )
        }
    };
    let target = request.version.trim().to_owned();
    let source = effective_art_update_source(&tool, &settings);
    let remote = source.as_ref().and_then(|source| {
        let catalog = fetch_remote_art_store_catalog(&source.store).ok()?;
        let entry = catalog
            .arts
            .into_iter()
            .find(|entry| remote_art_entry_matches(entry, source))?;
        entry
            .versions
            .into_iter()
            .find(|version| version.version == target)
            .map(|version| (source.clone(), version.sha256))
    });
    let result = if let Some((source, sha256)) = remote {
        install_art_from_store_request(
            &InstallFromStoreRequest {
                art_id: source.art_id,
                version: Some(target.clone()),
                store: None,
                sha256: (!sha256.is_empty()).then_some(sha256),
            },
            tool_registry,
            framework_registry,
            control_plane_root,
            Some(&identity),
        )
        .map(|_| ())
    } else {
        loom_tool_registry::install::activate_art_version(
            control_plane_root,
            &identity,
            &target,
            tool_registry,
            framework_registry,
        )
        .map(|_| ())
    };
    if let Err(error) = result {
        return structured_error(
            409,
            json!({ "code": "art_update_failed", "message": error.to_string() }),
        );
    }
    if let Ok(Some(mut active)) = tool_registry.get_tool(&identity) {
        apply_settings_metadata(&mut active, &settings);
        if let Err(message) = sync_installed_workflow_definition(&active, workflow_store) {
            return structured_error(
                409,
                json!({ "code": "workflow_art_update_failed", "message": message }),
            );
        }
        if let Err(error) = tool_registry.save_tool(active) {
            return tool_registry_error_response(error);
        }
    }
    broadcast_tool_capabilities_updated(hook_bridge);
    get_art_management(&identity, tool_registry, control_plane_root)
}

fn auto_update_arts(
    tool_registry: &ToolRegistry,
    framework_registry: &FrameworkRegistry,
    workflow_store: &WorkflowStore,
    control_plane_root: &Path,
    hook_bridge: &SharedHookBridgeRuntime,
    settings_store: &SharedLoomSettingsStore,
) -> Result<(u16, String)> {
    let auto_update_enabled = settings_store
        .lock()
        .map_err(|_| anyhow::anyhow!("lock Loom settings"))?
        .settings
        .art_store
        .auto_update;
    if !auto_update_enabled {
        return Ok((
            200,
            serde_json::to_string(&json!({
                "updated": [],
                "errors": [],
                "disabled": true
            }))?,
        ));
    }
    let mut stored_settings = match ArtSettingsStore::new(control_plane_root).list() {
        Ok(settings) => settings,
        Err(error) => {
            return structured_error(
                500,
                json!({ "code": "art_settings", "message": error.to_string() }),
            )
        }
    };
    let mut updated = Vec::new();
    let mut errors = Vec::new();
    let tools = match tool_registry.list_tools() {
        Ok(tools) => tools,
        Err(error) => return tool_registry_error_response(error),
    };
    for tool in tools {
        let identity = tool.qualified_id();
        let settings = stored_settings.remove(&identity).unwrap_or_default();
        if !settings.auto_update {
            continue;
        }
        let Some(source) = effective_art_update_source(&tool, &settings) else {
            continue;
        };
        let current = art_version_from_tool(&tool);
        let remote = match fetch_remote_art_store_catalog(&source.store) {
            Ok(catalog) => catalog
                .arts
                .into_iter()
                .find(|entry| remote_art_entry_matches(entry, &source)),
            Err(error) => {
                errors.push(json!({ "artId": identity, "message": error }));
                continue;
            }
        };
        let Some(remote) = remote else {
            continue;
        };
        if !version_is_newer(&remote.latest_version, &current) {
            continue;
        }
        let digest = remote
            .versions
            .iter()
            .find(|version| version.version == remote.latest_version)
            .map(|version| version.sha256.clone())
            .filter(|digest| !digest.is_empty());
        let request = InstallFromStoreRequest {
            art_id: source.art_id,
            version: Some(remote.latest_version.clone()),
            store: None,
            sha256: digest,
        };
        match install_art_from_store_request(
            &request,
            tool_registry,
            framework_registry,
            control_plane_root,
            Some(&identity),
        ) {
            Ok(_) => {
                match sync_registered_workflow_definition(&identity, tool_registry, workflow_store)
                {
                    Ok(()) => updated.push(json!({
                        "artId": identity,
                        "from": current,
                        "to": remote.latest_version,
                    })),
                    Err(message) => errors.push(json!({ "artId": identity, "message": message })),
                }
            }
            Err(error) => errors.push(json!({ "artId": identity, "message": error.to_string() })),
        }
    }
    if !updated.is_empty() {
        broadcast_tool_capabilities_updated(hook_bridge);
    }
    Ok((
        200,
        serde_json::to_string(&json!({ "updated": updated, "errors": errors }))?,
    ))
}

fn art_management_parameters(tool: &ToolDefinition) -> Vec<ArtManagementParameter> {
    art_parameter_definitions(tool)
        .into_iter()
        .map(art_management_parameter)
        .collect()
}

fn art_secret_parameter_ids(tool: &ToolDefinition) -> std::collections::BTreeSet<String> {
    art_management_parameters(tool)
        .into_iter()
        .filter(|parameter| parameter.secret)
        .map(|parameter| parameter.id)
        .collect()
}

fn art_setting_value_present(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::String(value) => !value.trim().is_empty(),
        _ => true,
    }
}

fn art_version_from_tool(tool: &ToolDefinition) -> String {
    tool.metadata
        .as_ref()
        .and_then(|metadata| metadata.get("artPackage"))
        .and_then(|package| package.get("version"))
        .or_else(|| {
            tool.metadata
                .as_ref()
                .and_then(|metadata| metadata.get("packageSecurity"))
                .and_then(|security| security.get("version"))
        })
        .and_then(Value::as_str)
        .unwrap_or("0.0.0")
        .to_owned()
}

fn resolve_registered_tool_package(
    tool: &ToolDefinition,
    tool_registry: &ToolRegistry,
    framework_registry: &FrameworkRegistry,
    control_plane_root: &Path,
) -> std::result::Result<ToolDefinition, loom_tool_registry::install::ArtInstallError> {
    let Some(package) = tool
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("artPackage"))
    else {
        return Ok(tool.clone());
    };
    let version = package
        .get("version")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            loom_tool_registry::install::ArtInstallError::InvalidPackage(
                "installed Art package has no version".to_owned(),
            )
        })?;
    let digest = package
        .get("digest")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            loom_tool_registry::install::ArtInstallError::InvalidPackage(
                "installed Art package has no digest".to_owned(),
            )
        })?;
    let mut resolved = loom_tool_registry::install::resolve_installed_art_package(
        control_plane_root,
        &tool.qualified_id(),
        version,
        digest,
        tool_registry,
        framework_registry,
    )?;
    resolved.enabled = tool.enabled;
    if let Some(user_settings) = tool
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("artUserSettings"))
    {
        let metadata = resolved.metadata.get_or_insert_with(|| json!({}));
        if !metadata.is_object() {
            *metadata = json!({});
        }
        metadata
            .as_object_mut()
            .expect("resolved metadata normalized")
            .insert("artUserSettings".to_owned(), user_settings.clone());
    }
    Ok(resolved)
}

fn effective_art_update_source(
    tool: &ToolDefinition,
    settings: &ArtUserSettings,
) -> Option<ArtUpdateSource> {
    let store = resolve_art_store_url()?;
    if let Some(mut source) = settings.source.clone() {
        // Older versions persisted a caller-selected store per Art. Keep the
        // package identity, but always route updates through Loom's official
        // deployment-managed store.
        source.store = store;
        source
            .qualified_id
            .get_or_insert_with(|| tool.qualified_id());
        return Some(source);
    }
    if tool.publisher_identity().is_none() {
        return None;
    }
    Some(ArtUpdateSource {
        store,
        art_id: tool.id.clone(),
        qualified_id: Some(tool.qualified_id()),
    })
}

fn remote_art_entry_matches(entry: &RemoteArtStoreEntry, source: &ArtUpdateSource) -> bool {
    let remote_identity = if entry.qualified_id.trim().is_empty() {
        entry.id.as_str()
    } else {
        entry.qualified_id.as_str()
    };
    entry.id == source.art_id
        && source
            .qualified_id
            .as_deref()
            .is_none_or(|identity| identity == remote_identity)
}

fn sort_versions(versions: &mut [String]) {
    versions.sort_by(|left, right| {
        match (semver::Version::parse(left), semver::Version::parse(right)) {
            (Ok(left), Ok(right)) => left.cmp(&right),
            _ => left.cmp(right),
        }
    });
}

fn version_is_newer(candidate: &str, current: &str) -> bool {
    match (
        semver::Version::parse(candidate),
        semver::Version::parse(current),
    ) {
        (Ok(candidate), Ok(current)) => candidate > current,
        _ => candidate > current,
    }
}

// The official Art store is configured by the Loom deployment. User requests
// and persisted Art settings must never select an alternate remote store.
fn resolve_art_store_url() -> Option<String> {
    std::env::var("LOOM_ART_STORE_URL")
        .ok()
        .map(|value| value.trim().trim_end_matches('/').to_owned())
        .filter(|value| !value.is_empty())
}

fn custom_art_store_requested(store: Option<&str>) -> bool {
    store.is_some_and(|value| !value.trim().is_empty())
}

fn user_configured_outbound_policy() -> OutboundPolicy {
    OutboundPolicy {
        // Local registry/store fixtures are a supported development mode. Only
        // literal loopback hosts may use HTTP; all other destinations require
        // HTTPS and public addresses.
        allow_http_loopback: true,
        ..OutboundPolicy::default()
    }
}

fn art_store_client() -> Result<reqwest::blocking::Client> {
    secure_client(
        "Loom/0.1 Art Store Client",
        Duration::from_secs(30),
        user_configured_outbound_policy(),
    )
    .map_err(|error| anyhow::anyhow!("build art store client: {error}"))
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteArtStoreCatalog {
    #[serde(default)]
    arts: Vec<RemoteArtStoreEntry>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteArtStoreEntry {
    id: String,
    #[serde(default)]
    qualified_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    global_id: Option<String>,
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    framework: String,
    #[serde(default)]
    latest_version: String,
    #[serde(default)]
    versions: Vec<RemoteArtStoreVersion>,
    #[serde(default)]
    official: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteArtStoreVersion {
    version: String,
    #[serde(default)]
    sha256: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum RemotePublisherKeyStatus {
    Active,
    Retired,
    Revoked,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemotePublisherKey {
    key_id: String,
    public_key: String,
    status: RemotePublisherKeyStatus,
    #[serde(default)]
    created_at: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemotePublisher {
    user_id: String,
    #[serde(default)]
    keys: Vec<RemotePublisherKey>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemotePublisherResponse {
    publisher: RemotePublisher,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalPublisherIdentity {
    #[serde(default = "publisher_identity_schema_version")]
    schema_version: u32,
    user_id: String,
    current_key_id: String,
    public_key: String,
}
