// Framework listing, diagnostics, package lookup, dependency checks, and plugin trust.
fn list_frameworks(framework_registry: &FrameworkRegistry) -> Result<(u16, String)> {
    let frameworks = framework_registry.statuses();
    Ok((
        200,
        serde_json::to_string(&json!({ "frameworks": frameworks }))?,
    ))
}

fn framework_doctor(framework_registry: &FrameworkRegistry) -> Result<(u16, String)> {
    let permission_mode = loom_tool_registry::framework::plugin_permission_mode();
    let permission_mode_label = match permission_mode.as_ref() {
        Ok(loom_tool_registry::framework::PluginPermissionMode::Audit) => "audit",
        Ok(loom_tool_registry::framework::PluginPermissionMode::Strict) => "strict",
        Err(_) => "invalid",
    };
    let permission_mode_error = permission_mode.err();
    let frameworks = framework_registry
        .statuses()
        .into_iter()
        .map(|status| {
            let permission_findings =
                loom_tool_registry::framework::unsupported_permission_findings_for(
                    &status.declared_permissions,
                    &status.permission_policy,
                );
            json!({
                "id": status.id,
                "qualifiedId": status.qualified_id,
                "version": status.version,
                "installed": status.installed,
                "enabled": status.enabled,
                "ready": status.ready,
                "detail": status.ready_detail,
                "publisher": status.publisher,
                "trustStatus": status.trust_status,
                "declaredPermissions": status.declared_permissions,
                "permissions": status.permission_policy,
                "permissionFindings": permission_findings,
                "strictCompatible": permission_findings.is_empty(),
                "resources": status.resources,
                "authoringSchemaAvailable": status.authoring_schema.is_some(),
            })
        })
        .collect::<Vec<_>>();
    let unhealthy = frameworks
        .iter()
        .filter(|framework| {
            framework["installed"].as_bool().unwrap_or(false)
                && framework["enabled"].as_bool().unwrap_or(false)
                && !framework["ready"].as_bool().unwrap_or(false)
        })
        .count();
    Ok((
        200,
        serde_json::to_string(&json!({
            "status": if unhealthy == 0 { "ok" } else { "degraded" },
            "unhealthy": unhealthy,
            "permissionMode": permission_mode_label,
            "permissionModeError": permission_mode_error,
            "enforcementMatrix": loom_tool_registry::framework::permission_enforcement_matrix(),
            "frameworks": frameworks,
        }))?,
    ))
}

fn art_doctor(
    tool_registry: &ToolRegistry,
    framework_registry: &FrameworkRegistry,
    control_plane_root: &Path,
) -> Result<(u16, String)> {
    let tools = match tool_registry.list_tools() {
        Ok(tools) => tools,
        Err(error) => return tool_registry_error_response(error),
    };
    let arts = tools
        .into_iter()
        .map(|tool| {
            let framework = framework_id_for_tool(&tool);
            let (framework_ready, framework_detail) = framework_registry.readiness(&framework);
            let package = tool
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("artPackage"));
            let integrity = package.map(|_| {
                loom_tool_registry::install::verify_art_package_integrity(
                    control_plane_root,
                    &tool,
                    framework_registry,
                )
                .map_err(|error| error.to_string())
            });
            let package_valid = integrity.as_ref().is_none_or(|result| result.is_ok());
            let package_detail = integrity.and_then(Result::err);
            json!({
                "id": tool.id,
                "qualifiedId": tool.qualified_id(),
                "publisher": tool.publisher_identity(),
                "enabled": tool.enabled,
                "frameworkId": framework,
                "frameworkReady": framework_ready,
                "frameworkDetail": framework_detail,
                "version": package.and_then(|package| package.get("version")).cloned(),
                "packageHash": package.and_then(|package| package.get("digest")).cloned(),
                "trustStatus": package.and_then(|package| package.get("trustStatus")).cloned(),
                "lockfileValid": package.is_some() && package_valid,
                "packageDetail": package_detail,
                "ready": tool.enabled && framework_ready && package_valid,
            })
        })
        .collect::<Vec<_>>();
    let unhealthy = arts
        .iter()
        .filter(|art| !art["ready"].as_bool().unwrap_or(false))
        .count();
    Ok((
        200,
        serde_json::to_string(&json!({
            "status": if unhealthy == 0 { "ok" } else { "degraded" },
            "unhealthy": unhealthy,
            "arts": arts,
        }))?,
    ))
}

fn support_bundle(
    path: &str,
    hook_settings: &HookSettings,
    run_store: &SharedRunStore,
    run_store_status: RunStoreStatus,
    tool_registry: &ToolRegistry,
    framework_registry: &FrameworkRegistry,
    control_plane_root: &Path,
) -> Result<(u16, String)> {
    let selected_run = if let Some(run_id) = query_value(path, "runId") {
        let store = match lock_run_store(run_store) {
            Ok(store) => store,
            Err(error) => return run_store_failed(error),
        };
        let Some(run) = store.get_run(&run_id).map_err(anyhow::Error::from)? else {
            return run_not_found(&run_id);
        };
        let events = store
            .get_events(&run_id)
            .map_err(anyhow::Error::from)?
            .unwrap_or_default();
        Some(json!({ "run": run, "events": events }))
    } else {
        None
    };
    let trust = framework_registry.trust_store().ok();
    let credential_summaries = CredentialStore::new(control_plane_root)
        .summaries()
        .unwrap_or_default();
    let tools = tool_registry
        .list_tools()
        .unwrap_or_default()
        .into_iter()
        .map(|tool| {
            json!({
                "id": tool.id,
                "qualifiedId": tool.qualified_id(),
                "publisher": tool.publisher_identity(),
                "enabled": tool.enabled,
                "frameworkId": framework_id_for_tool(&tool),
                "package": tool.metadata.as_ref().and_then(|metadata| metadata.get("artPackage")).map(|package| json!({
                    "version": package.get("version").cloned(),
                    "digest": package.get("digest").cloned(),
                    "trustStatus": package.get("trustStatus").cloned(),
                })),
            })
        })
        .collect::<Vec<_>>();
    let mut bundle = json!({
        "schemaVersion": 1,
        "generatedAtUnixMs": SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
        "daemon": {
            "version": env!("CARGO_PKG_VERSION"),
            "pid": std::process::id(),
            "modules": module_statuses(),
            "hooks": hook_settings.summary(),
            "runStore": run_store_status,
        },
        "frameworks": framework_registry.statuses(),
        "arts": tools,
        "trust": {
            "publisherKeyCount": trust.as_ref().map(|store| store.publishers.len()).unwrap_or_default(),
            "revokedKeyCount": trust.as_ref().map(|store| store.publishers.iter().filter(|record| record.revoked).count()).unwrap_or_default(),
        },
        "credentials": {
            "count": credential_summaries.len(),
            "scopes": credential_summaries.into_iter().map(|credential| credential.scope).collect::<Vec<_>>(),
        },
        "selectedExecution": selected_run,
    });
    redact_json(&mut bundle);
    Ok((200, serde_json::to_string(&bundle)?))
}

fn execution_diagnostics(run_id: &str, run_store: &SharedRunStore) -> Result<(u16, String)> {
    let store = match lock_run_store(run_store) {
        Ok(store) => store,
        Err(error) => return run_store_failed(error),
    };
    let Some(run) = store.get_run(run_id).map_err(anyhow::Error::from)? else {
        return run_not_found(run_id);
    };
    let events = store
        .get_events(run_id)
        .map_err(anyhow::Error::from)?
        .unwrap_or_default();
    drop(store);
    let mut response = json!({ "execution": run, "events": events });
    redact_json(&mut response);
    Ok((200, serde_json::to_string(&response)?))
}

fn redact_json(value: &mut Value) {
    match value {
        Value::Object(object) => {
            for (key, value) in object.iter_mut() {
                let normalized = key.to_ascii_lowercase().replace(['_', '-'], "");
                if normalized.contains("password")
                    || normalized.contains("secret")
                    || normalized.contains("authorization")
                    || normalized.contains("accesstoken")
                    || normalized.contains("refreshtoken")
                    || normalized.contains("privatekey")
                    || normalized.contains("bearer")
                    || normalized.contains("cookie")
                    || normalized == "token"
                    || normalized.ends_with("token")
                    || normalized == "apikey"
                    || normalized == "credentialvalue"
                {
                    *value = Value::String("[REDACTED]".to_owned());
                } else {
                    redact_json(value);
                }
            }
        }
        Value::Array(values) => values.iter_mut().for_each(redact_json),
        Value::String(text) => {
            let lowercase = text.trim_start().to_ascii_lowercase();
            if lowercase.starts_with("bearer ")
                || lowercase.starts_with("basic ")
                || text.contains("-----BEGIN PRIVATE KEY-----")
                || text.contains("-----BEGIN OPENSSH PRIVATE KEY-----")
            {
                *text = "[REDACTED]".to_owned();
                return;
            }
            if let Ok(mut url) = reqwest::Url::parse(text) {
                if matches!(url.scheme(), "http" | "https")
                    && (url.query().is_some()
                        || url.fragment().is_some()
                        || !url.username().is_empty()
                        || url.password().is_some())
                {
                    url.set_query(None);
                    url.set_fragment(None);
                    if !url.username().is_empty() {
                        let _ = url.set_username("[REDACTED]");
                    }
                    if url.password().is_some() {
                        let _ = url.set_password(Some("[REDACTED]"));
                    }
                    *text = url.to_string();
                }
            }
            if text.len() > 4096 {
                text.truncate(4096);
                text.push_str(" [truncated]");
            }
        }
        _ => {}
    }
}

fn list_plugin_trust(framework_registry: &FrameworkRegistry) -> Result<(u16, String)> {
    match framework_registry.trust_store() {
        Ok(store) => Ok((200, serde_json::to_string(&store)?)),
        Err(error) => structured_error(
            500,
            json!({ "code": "plugin_trust_store_failed", "message": error.to_string() }),
        ),
    }
}

fn trust_plugin_publisher(
    body: &str,
    framework_registry: &FrameworkRegistry,
) -> Result<(u16, String)> {
    let record: PublisherTrustRecord = match serde_json::from_str(body) {
        Ok(record) => record,
        Err(error) => {
            return structured_error(
                400,
                json!({ "code": "invalid_publisher_trust", "message": error.to_string() }),
            )
        }
    };
    let public_key = match BASE64.decode(record.public_key.as_bytes()) {
        Ok(public_key) => public_key,
        Err(error) => {
            return structured_error(
                400,
                json!({ "code": "invalid_publisher_key", "message": error.to_string() }),
            )
        }
    };
    if !is_safe_publisher_id(&record.publisher_id)
        || !is_safe_package_id(&record.key_id)
        || public_key.len() != 32
    {
        return structured_error(
            400,
            json!({
                "code": "invalid_publisher_trust",
                "message": "publisherId/keyId must be safe IDs and publicKey must be a 32-byte Ed25519 key"
            }),
        );
    }
    match framework_registry.trust_publisher(record) {
        Ok(()) => list_plugin_trust(framework_registry),
        Err(error) => structured_error(
            500,
            json!({ "code": "plugin_trust_store_failed", "message": error.to_string() }),
        ),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RevokePublisherRequest {
    publisher_id: String,
    key_id: String,
}

fn revoke_plugin_publisher(
    body: &str,
    framework_registry: &FrameworkRegistry,
) -> Result<(u16, String)> {
    let request: RevokePublisherRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(error) => {
            return structured_error(
                400,
                json!({ "code": "invalid_publisher_revoke", "message": error.to_string() }),
            )
        }
    };
    match framework_registry.revoke_publisher(&request.publisher_id, &request.key_id) {
        Ok(true) => list_plugin_trust(framework_registry),
        Ok(false) => structured_error(
            404,
            json!({ "code": "publisher_key_not_found", "message": "publisher key was not found" }),
        ),
        Err(error) => structured_error(
            500,
            json!({ "code": "plugin_trust_store_failed", "message": error.to_string() }),
        ),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrustPolicyRequest {
    policy: TrustPolicy,
}

fn set_plugin_trust_policy(
    body: &str,
    framework_registry: &FrameworkRegistry,
) -> Result<(u16, String)> {
    let request: TrustPolicyRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    match framework_registry.set_trust_policy(request.policy) {
        Ok(store) => Ok((200, serde_json::to_string(&store)?)),
        Err(error) => structured_error(
            500,
            json!({ "code": "plugin_trust_store_failed", "message": error.to_string() }),
        ),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrustedUserRequest {
    user_id: String,
    #[serde(default)]
    store: Option<String>,
}

fn trust_plugin_user(body: &str, framework_registry: &FrameworkRegistry) -> Result<(u16, String)> {
    let request: TrustedUserRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let user_id = request.user_id.trim();
    if !is_platform_publisher_id(user_id) {
        return invalid_request("用户 ID 格式无效");
    }
    if custom_art_store_requested(request.store.as_deref()) {
        return structured_error(
            400,
            json!({
                "code": "custom_art_store_not_supported",
                "message": "Loom 不支持选择第三方 Art 商店"
            }),
        );
    }
    let Some(store) = resolve_art_store_url() else {
        return structured_error(
            400,
            json!({ "code": "art_store_not_configured", "message": "Loom 官方 Art 服务暂不可用" }),
        );
    };
    if let Err(error) = sync_trusted_publisher_from_store(&store, user_id, framework_registry) {
        return structured_error(
            502,
            json!({ "code": "publisher_directory_unavailable", "message": error }),
        );
    }
    list_plugin_trust(framework_registry)
}

fn untrust_plugin_user(
    body: &str,
    framework_registry: &FrameworkRegistry,
) -> Result<(u16, String)> {
    let request: TrustedUserRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    match framework_registry.untrust_publisher(request.user_id.trim()) {
        Ok(store) => Ok((200, serde_json::to_string(&store)?)),
        Err(error) => structured_error(
            500,
            json!({ "code": "plugin_trust_store_failed", "message": error.to_string() }),
        ),
    }
}
