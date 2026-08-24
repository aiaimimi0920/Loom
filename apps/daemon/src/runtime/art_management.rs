// Art management settings, validation, credentials, and parameter presentation.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ArtManagementParameter {
    id: String,
    label: String,
    parameter_type: String,
    required: bool,
    secret: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    default: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    minimum: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    maximum: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    step: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PutArtManagementSettingsRequest {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    auto_update: Option<bool>,
    #[serde(default)]
    defaults: Option<BTreeMap<String, Value>>,
    #[serde(default)]
    value_bindings: Option<BTreeMap<String, String>>,
    #[serde(default)]
    credential_bindings: Option<BTreeMap<String, String>>,
    #[serde(default)]
    secret_values: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateArtVersionRequest {
    version: String,
}

fn get_art_management(
    art_id: &str,
    tool_registry: &ToolRegistry,
    control_plane_root: &Path,
) -> Result<(u16, String)> {
    match build_art_management_response(art_id, tool_registry, control_plane_root) {
        Ok(response) => Ok((200, serde_json::to_string(&response)?)),
        Err((status, code, message)) => {
            structured_error(status, json!({ "code": code, "message": message }))
        }
    }
}

fn build_art_management_response(
    art_id: &str,
    tool_registry: &ToolRegistry,
    control_plane_root: &Path,
) -> std::result::Result<Value, (u16, &'static str, String)> {
    let tool = tool_registry
        .get_tool(art_id)
        .map_err(|error| (500, "tool_registry", error.to_string()))?
        .ok_or_else(|| (404, "tool_not_found", format!("Art `{art_id}` 不存在")))?;
    let identity = tool.qualified_id();
    let mut settings = ArtSettingsStore::new(control_plane_root)
        .get(&identity)
        .map_err(|error| (500, "art_settings", error.to_string()))?;
    let secret_parameters = art_secret_parameter_ids(&tool);
    settings
        .defaults
        .retain(|id, _| !secret_parameters.contains(id));
    let installed_versions = loom_tool_registry::install::list_installed_art_versions(
        control_plane_root,
        &identity,
        tool_registry,
    )
    .unwrap_or_else(|_| {
        vec![loom_tool_registry::install::ArtInstalledVersion {
            version: art_version_from_tool(&tool),
            digest: String::new(),
            active: true,
        }]
    });
    let current_version = installed_versions
        .iter()
        .find(|version| version.active)
        .map(|version| version.version.clone())
        .unwrap_or_else(|| art_version_from_tool(&tool));
    let source = effective_art_update_source(&tool, &settings);
    let remote_entry = source.as_ref().and_then(|source| {
        fetch_remote_art_store_catalog(&source.store)
            .ok()?
            .arts
            .into_iter()
            .find(|entry| remote_art_entry_matches(entry, source))
    });
    let mut available_versions = installed_versions
        .iter()
        .map(|version| version.version.clone())
        .collect::<Vec<_>>();
    if let Some(entry) = &remote_entry {
        available_versions.extend(entry.versions.iter().map(|version| version.version.clone()));
        if !entry.latest_version.is_empty() {
            available_versions.push(entry.latest_version.clone());
        }
    }
    sort_versions(&mut available_versions);
    available_versions.dedup();
    let highest_version = available_versions
        .last()
        .cloned()
        .unwrap_or_else(|| current_version.clone());
    let available_credentials = CredentialStore::new(control_plane_root)
        .summaries()
        .map_err(|error| (500, "credential_store", error.to_string()))?
        .into_iter()
        .filter(|credential| {
            credential.scope.framework_id.is_none()
                && credential
                    .scope
                    .art_id
                    .as_deref()
                    .is_none_or(|art_id| art_id == identity.as_str())
        })
        .collect::<Vec<_>>();

    Ok(json!({
        "artId": identity,
        "name": tool.name,
        "description": tool.description,
        "locallyAuthored": art_is_locally_authored(&tool),
        "canEditIdentity": art_is_locally_authored(&tool),
        "currentVersion": current_version,
        "highestVersion": highest_version,
        "autoUpdate": settings.auto_update,
        "installedVersions": installed_versions,
        "availableVersions": available_versions,
        "parameters": art_management_parameters(&tool),
        "defaults": settings.defaults,
        "valueBindings": settings.value_bindings,
        "credentialBindings": settings.credential_bindings,
        "availableCredentials": available_credentials,
        "updateAvailable": version_is_newer(&highest_version, &current_version),
    }))
}

fn put_art_management_settings(
    art_id: &str,
    body: &str,
    tool_registry: &ToolRegistry,
    control_plane_root: &Path,
    hook_bridge: &SharedHookBridgeRuntime,
) -> Result<(u16, String)> {
    let request: PutArtManagementSettingsRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let mut tool = match tool_registry.get_tool(art_id) {
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
    let locally_authored = art_is_locally_authored(&tool);
    if !locally_authored
        && (request
            .name
            .as_deref()
            .is_some_and(|name| name.trim() != tool.name)
            || request
                .description
                .as_deref()
                .is_some_and(|description| description != tool.description))
    {
        return structured_error(
            403,
            json!({
                "code": "art_identity_read_only",
                "message": "其他发布者的 Art 名称和描述不可修改",
            }),
        );
    }
    let store = ArtSettingsStore::new(control_plane_root);
    let mut settings = match store.get(&identity) {
        Ok(settings) => settings,
        Err(error) => {
            return structured_error(
                500,
                json!({ "code": "art_settings", "message": error.to_string() }),
            )
        }
    };
    let previous_bindings = settings.credential_bindings.clone();
    if settings.source.is_none() {
        settings.source = effective_art_update_source(&tool, &settings);
    }
    if let Some(auto_update) = request.auto_update {
        settings.auto_update = auto_update;
    }
    let secret_parameters = art_secret_parameter_ids(&tool);
    settings
        .defaults
        .retain(|id, _| !secret_parameters.contains(id));
    if let Some(defaults) = request.defaults {
        if let Some(secret) = defaults
            .keys()
            .find(|id| secret_parameters.contains(id.as_str()))
        {
            return invalid_request(format!(
                "机密参数 `{secret}` 必须引用机密或使用专用机密输入，不能保存到普通默认值"
            ));
        }
        settings.defaults = defaults;
    }
    if let Some(bindings) = request.value_bindings {
        settings.value_bindings = bindings;
    }
    if let Some(bindings) = request.credential_bindings {
        settings.credential_bindings = bindings;
    }
    for (alias, value) in &request.secret_values {
        if !secret_parameters.contains(alias) {
            return invalid_request(format!("Art 不包含机密参数 `{alias}`"));
        }
        if value.trim().is_empty() {
            return invalid_request(format!("机密参数 `{alias}` 的值不能为空"));
        }
    }
    let pending_art_credentials = request
        .secret_values
        .keys()
        .map(|alias| manual_art_credential_name(&identity, alias))
        .collect::<BTreeSet<_>>();
    for (alias, credential) in request
        .secret_values
        .keys()
        .map(|alias| (alias.clone(), manual_art_credential_name(&identity, alias)))
    {
        settings.credential_bindings.insert(alias, credential);
    }
    if locally_authored {
        if let Some(name) = request.name {
            let name = name.trim();
            if name.is_empty() {
                return invalid_request("Art 名称不能为空");
            }
            settings.name = Some(name.to_owned());
        }
        if let Some(description) = request.description {
            settings.description = Some(description);
        }
    }
    if let Err(message) = validate_art_management_settings(
        &tool,
        &settings,
        control_plane_root,
        &pending_art_credentials,
    ) {
        return invalid_request(message);
    }
    let credential_store = CredentialStore::new(control_plane_root);
    for (alias, value) in request.secret_values {
        let name = manual_art_credential_name(&identity, &alias);
        if let Err(error) = credential_store.upsert(CredentialInput {
            name,
            value,
            value_type: CredentialValueType::String,
            scope: CredentialScope {
                framework_id: None,
                art_id: Some(identity.clone()),
                mcp_server_id: None,
            },
            expires_at: None,
        }) {
            return structured_error(
                400,
                json!({ "code": "credential_store_failed", "message": error.to_string() }),
            );
        }
    }
    if let Err(error) = store.save(&identity, settings.clone()) {
        return structured_error(
            500,
            json!({ "code": "art_settings", "message": error.to_string() }),
        );
    }
    apply_settings_metadata(&mut tool, &settings);
    if let Err(error) = tool_registry.save_tool(tool) {
        return tool_registry_error_response(error);
    }
    for parameter_id in &secret_parameters {
        let old_name = previous_bindings.get(parameter_id);
        let expected_name = manual_art_credential_name(&identity, parameter_id);
        if old_name.is_some_and(|name| name == &expected_name)
            && settings.credential_bindings.get(parameter_id) != Some(&expected_name)
        {
            let _ = credential_store.delete(
                &expected_name,
                &CredentialScope {
                    framework_id: None,
                    art_id: Some(identity.clone()),
                    mcp_server_id: None,
                },
            );
        }
    }
    broadcast_tool_capabilities_updated(hook_bridge);
    get_art_management(&identity, tool_registry, control_plane_root)
}

fn validate_art_management_settings(
    tool: &ToolDefinition,
    settings: &ArtUserSettings,
    control_plane_root: &Path,
    pending_credentials: &BTreeSet<String>,
) -> std::result::Result<(), String> {
    let parameters = art_parameter_definitions(tool);
    let parameter_ids = parameters
        .iter()
        .map(|parameter| parameter.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    if let Some(unknown) = settings
        .defaults
        .keys()
        .chain(settings.value_bindings.keys())
        .chain(settings.credential_bindings.keys())
        .find(|id| !parameter_ids.contains(id.as_str()))
    {
        return Err(format!("Art 不包含参数 `{unknown}`"));
    }
    if let Some(overlap) = settings
        .defaults
        .keys()
        .find(|id| settings.value_bindings.contains_key(*id))
    {
        return Err(format!("参数 `{overlap}` 不能同时保存默认值和全局值引用"));
    }
    for parameter in &parameters {
        if parameter.secret {
            if settings.defaults.contains_key(&parameter.id) {
                return Err(format!(
                    "机密参数 `{}` 不能保存到普通默认值",
                    parameter.label
                ));
            }
            if settings.value_bindings.contains_key(&parameter.id) {
                return Err(format!("机密参数 `{}` 必须使用机密引用", parameter.label));
            }
        } else if settings.credential_bindings.contains_key(&parameter.id) {
            return Err(format!("普通参数 `{}` 必须使用全局值引用", parameter.label));
        }
        if let Some(value) = settings.defaults.get(&parameter.id) {
            validate_parameter_value(parameter, value)?;
        }
    }

    let resolved_values = CredentialStore::new(control_plane_root)
        .global_values_for_bindings(&settings.value_bindings)
        .map_err(|error| error.to_string())?;
    for (id, resolved) in &resolved_values {
        let parameter = parameters
            .iter()
            .find(|parameter| parameter.id == *id)
            .ok_or_else(|| format!("Art 不包含参数 `{id}`"))?;
        if !credential_value_type_matches_parameter(parameter, resolved.value_type) {
            return Err(format!(
                "全局值 `{}` 的类型与参数 `{}` 不匹配",
                settings
                    .value_bindings
                    .get(id)
                    .map(String::as_str)
                    .unwrap_or_default(),
                parameter.label
            ));
        }
        validate_parameter_value(parameter, &resolved.value)?;
    }

    for parameter in &parameters {
        if parameter.required {
            if parameter.secret {
                if !settings.credential_bindings.contains_key(&parameter.id) {
                    return Err(format!("必须为 `{}` 引用机密或直接填写", parameter.label));
                }
                continue;
            }
            if settings.value_bindings.contains_key(&parameter.id) {
                continue;
            }
            let value = settings
                .defaults
                .get(&parameter.id)
                .or(parameter.default.as_ref());
            if !value.is_some_and(art_setting_value_present) {
                return Err(format!("必须填写参数 `{}`", parameter.label));
            }
        }
    }
    let identity = tool.qualified_id();
    let available_credentials = CredentialStore::new(control_plane_root)
        .summaries()
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|credential| {
            credential.scope.framework_id.is_none()
                && credential
                    .scope
                    .art_id
                    .as_deref()
                    .is_none_or(|art_id| art_id == identity.as_str())
        })
        .collect::<Vec<_>>();
    for (alias, credential_name) in &settings.credential_bindings {
        let parameter = parameters
            .iter()
            .find(|parameter| parameter.id == *alias)
            .ok_or_else(|| format!("Art 不包含参数 `{alias}`"))?;
        if !parameter.secret {
            return Err(format!("普通参数 `{}` 不能使用机密引用", parameter.label));
        }
        if pending_credentials.contains(credential_name) {
            continue;
        }
        let credential = available_credentials
            .iter()
            .filter(|credential| credential.name == *credential_name)
            .max_by_key(|credential| usize::from(credential.scope.art_id.is_some()))
            .ok_or_else(|| format!("机密 `{credential_name}` 不存在或不适用于当前 Art"))?;
        if credential.value_type != CredentialValueType::String {
            return Err(format!("机密 `{credential_name}` 必须是文本类型"));
        }
    }
    Ok(())
}

fn art_management_parameter(parameter: ArtParameterDefinition) -> ArtManagementParameter {
    ArtManagementParameter {
        id: parameter.id,
        label: parameter.label,
        parameter_type: parameter.parameter_type,
        required: parameter.required,
        secret: parameter.secret,
        default: parameter.default,
        options: parameter.options,
        minimum: parameter.minimum,
        maximum: parameter.maximum,
        step: parameter.step,
    }
}
