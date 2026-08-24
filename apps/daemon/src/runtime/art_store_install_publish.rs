// Art store installation, uninstall, upgrade, and publication workflows.
fn install_art_from_store(
    body: &str,
    tool_registry: &ToolRegistry,
    framework_registry: &FrameworkRegistry,
    workflow_store: &WorkflowStore,
    control_plane_root: &Path,
    hook_bridge: &SharedHookBridgeRuntime,
) -> Result<(u16, String)> {
    let request = match serde_json::from_str::<InstallFromStoreRequest>(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    match install_art_from_store_request(
        &request,
        tool_registry,
        framework_registry,
        control_plane_root,
        None,
    ) {
        Ok(reports) => {
            for report in &reports {
                if let Err(message) = sync_registered_workflow_definition(
                    &report.tool_id,
                    tool_registry,
                    workflow_store,
                ) {
                    return structured_error(
                        400,
                        json!({ "code": "workflow_art_install_failed", "message": message }),
                    );
                }
            }
            broadcast_tool_capabilities_updated(hook_bridge);
            Ok((200, serde_json::to_string(&json!({ "reports": reports }))?))
        }
        Err(loom_tool_registry::install::ArtInstallError::FrameworkNotReady {
            art_id,
            framework,
            reason,
        }) => structured_error(
            409,
            json!({
                "code": "framework_not_ready",
                "message": format!("art `{art_id}` 需要框架 `{framework}`（未{reason}），请先安装该框架"),
                "framework": framework,
            }),
        ),
        Err(error) => structured_error(
            400,
            json!({ "code": "art_install_failed", "message": error.to_string() }),
        ),
    }
}

fn install_art_from_store_request(
    request: &InstallFromStoreRequest,
    tool_registry: &ToolRegistry,
    framework_registry: &FrameworkRegistry,
    control_plane_root: &Path,
    expected_identity: Option<&str>,
) -> std::result::Result<
    Vec<loom_tool_registry::install::ArtInstallReport>,
    loom_tool_registry::install::ArtInstallError,
> {
    if custom_art_store_requested(request.store.as_deref()) {
        return Err(
            loom_tool_registry::install::ArtInstallError::InvalidPackage(
                "Loom 不支持选择第三方 Art 商店；可以改用本地 Art 包安装".to_owned(),
            ),
        );
    }
    let Some(store) = resolve_art_store_url() else {
        return Err(
            loom_tool_registry::install::ArtInstallError::InvalidPackage(
                "Loom 官方 Art 服务暂不可用".to_owned(),
            ),
        );
    };
    let store = store.trim_end_matches('/').to_owned();
    let trust_store = framework_registry.trust_store().map_err(|error| {
        loom_tool_registry::install::ArtInstallError::InvalidPackage(error.to_string())
    })?;
    if trust_store.effective_policy() == TrustPolicy::RequireTrusted {
        for user_id in trust_store.trusted_publishers {
            sync_trusted_publisher_from_store(&store, &user_id, framework_registry).map_err(
                |error| {
                    loom_tool_registry::install::ArtInstallError::InvalidPackage(format!(
                        "刷新可信用户 `{user_id}` 的公钥失败：{error}"
                    ))
                },
            )?;
        }
    }
    let client = art_store_client().map_err(|error| {
        loom_tool_registry::install::ArtInstallError::InvalidPackage(error.to_string())
    })?;
    let policy = user_configured_outbound_policy();
    let fetch_zip = |id: &str| fetch_art_store_package(&client, &policy, &store, id, None, None);

    let root_zip = fetch_art_store_package(
        &client,
        &policy,
        &store,
        &request.art_id,
        request.version.as_deref(),
        request.sha256.as_deref(),
    )?;
    let root_manifest = loom_tool_registry::install::read_manifest_from_zip(&root_zip)?;
    if root_manifest.id != request.art_id {
        return Err(
            loom_tool_registry::install::ArtInstallError::InvalidPackage(format!(
                "store package `{}` contains Art `{}`",
                request.art_id, root_manifest.id
            )),
        );
    }
    let root_identity = root_manifest.qualified_id();
    if expected_identity.is_some_and(|expected| expected != root_identity) {
        return Err(
            loom_tool_registry::install::ArtInstallError::InvalidPackage(format!(
                "store package `{}` identity mismatch: expected {}, got {root_identity}",
                request.art_id,
                expected_identity.unwrap_or_default()
            )),
        );
    }
    let reports = loom_tool_registry::install::install_art_recursive(
        &root_zip,
        control_plane_root,
        framework_registry,
        tool_registry,
        &fetch_zip,
    )?;
    let mut tool = tool_registry
        .get_tool(&root_identity)
        .map_err(|error| loom_tool_registry::install::ArtInstallError::Registry(error.to_string()))?
        .ok_or_else(|| {
            loom_tool_registry::install::ArtInstallError::InvalidPackage(format!(
                "installed Art `{}` was not registered",
                root_identity
            ))
        })?;
    let identity = tool.qualified_id();
    let global_id = fetch_remote_art_store_catalog(&store)
        .ok()
        .and_then(|catalog| {
            catalog.arts.into_iter().find(|entry| {
                entry.id == request.art_id
                    && (entry.qualified_id.trim().is_empty() || entry.qualified_id == root_identity)
            })
        })
        .and_then(|entry| entry.global_id)
        .filter(|global_id| is_platform_global_art_id(global_id));
    let settings_store = ArtSettingsStore::new(control_plane_root);
    let mut settings = settings_store.get(&identity).map_err(|error| {
        loom_tool_registry::install::ArtInstallError::InvalidPackage(error.to_string())
    })?;
    settings.source = Some(ArtUpdateSource {
        store,
        art_id: request.art_id.clone(),
        qualified_id: Some(identity.clone()),
    });
    settings_store
        .save(&identity, settings.clone())
        .map_err(|error| {
            loom_tool_registry::install::ArtInstallError::InvalidPackage(error.to_string())
        })?;
    apply_settings_metadata(&mut tool, &settings);
    if let Some(global_id) = global_id.as_deref() {
        apply_platform_global_art_id(&mut tool, global_id);
    }
    tool_registry.save_tool(tool).map_err(|error| {
        loom_tool_registry::install::ArtInstallError::Registry(error.to_string())
    })?;
    Ok(reports)
}

// Package an installed art into a zip (manifest + its resource dir), returned as
// a base64 data URL so the frontend can export/save it.
fn package_art(
    id: &str,
    tool_registry: &ToolRegistry,
    control_plane_root: &Path,
) -> Result<(u16, String)> {
    let tool = match tool_registry.get_tool(id) {
        Ok(Some(tool)) => tool,
        Ok(None) => {
            return structured_error(
                404,
                json!({ "code": "tool_not_found", "message": format!("art `{id}` 不存在") }),
            )
        }
        Err(error) => return tool_registry_error_response(error),
    };
    let art_dir = installed_art_package_dir(&tool, control_plane_root);
    match loom_tool_registry::install::package_art_to_zip(&tool, &art_dir) {
        Ok(bytes) => {
            use base64::Engine as _;
            let data = format!(
                "data:application/zip;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(&bytes)
            );
            Ok((
                200,
                serde_json::to_string(&json!({ "artId": id, "zipBase64": data }))?,
            ))
        }
        Err(error) => structured_error(
            500,
            json!({ "code": "art_package_failed", "message": error.to_string() }),
        ),
    }
}

fn installed_art_package_dir(tool: &ToolDefinition, control_plane_root: &Path) -> PathBuf {
    tool.metadata
        .as_ref()
        .and_then(|metadata| metadata.get("artPackage"))
        .and_then(|package| package.get("dir"))
        .and_then(Value::as_str)
        .filter(|path| !path.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            control_plane_root
                .join("arts")
                .join(".unresolved")
                .join(&tool.id)
        })
}

fn is_platform_global_art_id(value: &str) -> bool {
    value.len() == 13
        && value.starts_with("NA")
        && value[2..].bytes().all(|byte| byte.is_ascii_digit())
}

fn is_platform_publisher_id(value: &str) -> bool {
    (value.len() == 13
        && value.starts_with("NU")
        && value[2..].bytes().all(|byte| byte.is_ascii_digit()))
        || (value.len() == 11
            && value.starts_with('L')
            && value[1..].bytes().all(|byte| byte.is_ascii_digit()))
}

fn apply_platform_global_art_id(tool: &mut ToolDefinition, global_id: &str) {
    if !is_platform_global_art_id(global_id) {
        return;
    }
    let metadata = tool
        .metadata
        .get_or_insert_with(|| Value::Object(serde_json::Map::new()));
    if !metadata.is_object() {
        *metadata = Value::Object(serde_json::Map::new());
    }
    let metadata = metadata
        .as_object_mut()
        .expect("Art metadata was normalized to an object");
    let art = metadata
        .entry("art".to_owned())
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    if !art.is_object() {
        *art = Value::Object(serde_json::Map::new());
    }
    art.as_object_mut()
        .expect("Art identity metadata was normalized to an object")
        .insert("globalId".to_owned(), Value::String(global_id.to_owned()));
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublishArtRequest {
    art_id: String,
    #[serde(default)]
    store: Option<String>,
}

// Publish a local art to the remote store: package it, then POST the zip to
// {store}/publish.
fn publish_art_to_store(
    body: &str,
    tool_registry: &ToolRegistry,
    control_plane_root: &Path,
) -> Result<(u16, String)> {
    let request = match serde_json::from_str::<PublishArtRequest>(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    if custom_art_store_requested(request.store.as_deref()) {
        return structured_error(
            400,
            json!({
                "code": "custom_art_store_not_supported",
                "message": "Loom 不支持选择第三方 Art 商店"
            }),
        );
    }
    let mut tool = match tool_registry.get_tool(&request.art_id) {
        Ok(Some(tool)) => tool,
        Ok(None) => {
            return structured_error(
                404,
                json!({ "code": "tool_not_found", "message": format!("art `{}` 不存在", request.art_id) }),
            )
        }
        Err(error) => return tool_registry_error_response(error),
    };
    if !art_is_locally_authored(&tool) {
        return structured_error(
            403,
            json!({
                "code": "art_publish_not_owned",
                "message": "只能发布当前用户本地创建的 Art"
            }),
        );
    }
    let Some(store) = resolve_art_store_url() else {
        return structured_error(
            400,
            json!({ "code": "art_store_not_configured", "message": "Loom 官方 Art 服务暂不可用" }),
        );
    };
    let (identity, signing_key) = match ensure_local_publisher_identity(control_plane_root) {
        Ok(identity) => identity,
        Err(error) => {
            return structured_error(
                500,
                json!({ "code": "publisher_identity_failed", "message": error }),
            )
        }
    };
    if let Err(error) = ensure_remote_publisher_registered(&store, &identity, &signing_key) {
        return structured_error(
            502,
            json!({ "code": "publisher_registration_failed", "message": error }),
        );
    }
    let art_dir = installed_art_package_dir(&tool, control_plane_root);
    let zip = match loom_tool_registry::install::package_signed_art_to_zip(
        &tool,
        &art_dir,
        &identity.user_id,
        &signing_key,
    ) {
        Ok(bytes) => bytes,
        Err(error) => {
            return structured_error(
                500,
                json!({ "code": "art_package_failed", "message": error.to_string() }),
            )
        }
    };
    let url = format!("{}/publish", store.trim_end_matches('/'));
    let client = match art_store_client() {
        Ok(client) => client,
        Err(error) => {
            return structured_error(
                500,
                json!({ "code": "internal", "message": error.to_string() }),
            )
        }
    };
    let policy = user_configured_outbound_policy();
    let parsed_url = match reqwest::Url::parse(&url) {
        Ok(url) => url,
        Err(error) => return invalid_request(format!("invalid art store URL: {error}")),
    };
    if let Err(error) = validate_outbound_url(&parsed_url, &policy) {
        return structured_error(
            403,
            json!({ "code": "art_store_security_policy", "message": error }),
        );
    }
    let digest = sha256_bytes(&zip);
    let response = match client
        .post(parsed_url)
        .header("Content-Type", "application/zip")
        .header("X-Art-Id", &request.art_id)
        .header("X-Art-Sha256", &digest)
        .body(zip)
        .send()
        .and_then(|r| r.error_for_status())
    {
        Ok(response) => response,
        Err(error) => {
            return structured_error(
                502,
                json!({ "code": "art_store_publish_failed", "message": format!("发布失败：{error}"), "url": url }),
            )
        }
    };
    let store_body = match response.text() {
        Ok(body) => body,
        Err(error) => {
            return structured_error(
                502,
                json!({
                    "code": "art_store_publish_contract_invalid",
                    "message": format!("商店未返回有效的发布结果：{error}"),
                    "url": url
                }),
            )
        }
    };
    let store_response = match serde_json::from_str::<Value>(&store_body) {
        Ok(response) => response,
        Err(error) => {
            return structured_error(
                502,
                json!({
                    "code": "art_store_publish_contract_invalid",
                    "message": format!("商店未返回有效的发布结果：{error}"),
                    "url": url
                }),
            )
        }
    };
    let Some(global_id) = store_response
        .get("globalId")
        .and_then(Value::as_str)
        .filter(|value| is_platform_global_art_id(value))
    else {
        return structured_error(
            502,
            json!({
                "code": "art_store_global_id_missing",
                "message": "商店发布成功，但没有返回有效的全局 Art 编号",
                "url": url
            }),
        );
    };
    apply_platform_global_art_id(&mut tool, global_id);
    if let Err(error) = tool_registry.save_tool(tool) {
        return structured_error(
            500,
            json!({
                "code": "art_global_id_persist_failed",
                "message": format!("Art 已发布，但无法保存平台编号：{error}"),
                "globalId": global_id
            }),
        );
    }
    Ok((
        200,
        serde_json::to_string(&json!({
            "artId": request.art_id,
            "globalId": global_id,
            "sha256": digest,
            "published": true
        }))?,
    ))
}
