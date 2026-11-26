// Publisher identity routes, framework lifecycle, tool readiness, and tool CRUD.
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublisherIdentityRequest {
    #[serde(default)]
    store: Option<String>,
}

fn list_publisher_identity(control_plane_root: &Path) -> Result<(u16, String)> {
    match ensure_local_publisher_identity(control_plane_root) {
        Ok((identity, _)) => Ok((
            200,
            serde_json::to_string(&json!({
                "identity": identity,
                "hasPrivateKey": true
            }))?,
        )),
        Err(error) => structured_error(
            500,
            json!({ "code": "publisher_identity_failed", "message": error }),
        ),
    }
}

fn register_publisher_identity(body: &str, control_plane_root: &Path) -> Result<(u16, String)> {
    let request: PublisherIdentityRequest = if body.trim().is_empty() {
        PublisherIdentityRequest::default()
    } else {
        match serde_json::from_str(body) {
            Ok(request) => request,
            Err(error) => return invalid_request(error.to_string()),
        }
    };
    if custom_art_store_requested(request.store.as_deref()) {
        return structured_error(
            400,
            json!({
                "code": "custom_art_store_not_supported",
                "message": "Loom does not support third-party Art stores"
            }),
        );
    }
    let (identity, key) = match ensure_local_publisher_identity(control_plane_root) {
        Ok(identity) => identity,
        Err(error) => {
            return structured_error(
                500,
                json!({ "code": "publisher_identity_failed", "message": error }),
            )
        }
    };
    if let Some(store) = resolve_art_store_url() {
        if let Err(error) = ensure_remote_publisher_registered(&store, &identity, &key) {
            return structured_error(
                502,
                json!({ "code": "publisher_registration_failed", "message": error }),
            );
        }
    }
    list_publisher_identity(control_plane_root)
}

fn rotate_publisher_identity(body: &str, control_plane_root: &Path) -> Result<(u16, String)> {
    let request: PublisherIdentityRequest = if body.trim().is_empty() {
        PublisherIdentityRequest::default()
    } else {
        match serde_json::from_str(body) {
            Ok(request) => request,
            Err(error) => return invalid_request(error.to_string()),
        }
    };
    if custom_art_store_requested(request.store.as_deref()) {
        return structured_error(
            400,
            json!({
                "code": "custom_art_store_not_supported",
                "message": "Loom does not support third-party Art stores"
            }),
        );
    }
    let (identity, current_key) = match ensure_local_publisher_identity(control_plane_root) {
        Ok(identity) => identity,
        Err(error) => {
            return structured_error(
                500,
                json!({ "code": "publisher_identity_failed", "message": error }),
            )
        }
    };
    let next_key = generate_signing_key(publisher_key_id());
    if let Some(store) = resolve_art_store_url() {
        if let Err(error) = ensure_remote_publisher_registered(&store, &identity, &current_key) {
            return structured_error(
                502,
                json!({ "code": "publisher_registration_failed", "message": error }),
            );
        }
        let message = format!(
            "loom.publisher.rotate.v1\n{}\n{}\n{}\n{}",
            identity.user_id, identity.current_key_id, next_key.key_id, next_key.public_key
        );
        let signature = match sign_message(&current_key, message.as_bytes()) {
            Ok(signature) => signature,
            Err(error) => {
                return structured_error(
                    500,
                    json!({ "code": "publisher_rotation_sign_failed", "message": error.to_string() }),
                )
            }
        };
        let path = format!("/publishers/{}/rotate", identity.user_id);
        let response: RemotePublisherResponse = match post_art_store_json(
            &store,
            &path,
            &json!({
                "currentKeyId": identity.current_key_id,
                "newKeyId": next_key.key_id,
                "newPublicKey": next_key.public_key,
                "signature": signature
            }),
        ) {
            Ok(response) => response,
            Err(error) => {
                return structured_error(
                    502,
                    json!({ "code": "publisher_rotation_failed", "message": error }),
                );
            }
        };
        if response.publisher.user_id != identity.user_id
            || !response.publisher.keys.iter().any(|remote| {
                remote.key_id == next_key.key_id
                    && remote.public_key == next_key.public_key
                    && remote.status == RemotePublisherKeyStatus::Active
            })
        {
            return structured_error(
                502,
                json!({ "code": "publisher_rotation_invalid", "message": "Art store returned an invalid key rotation result" }),
            );
        }
        let next_identity = LocalPublisherIdentity {
            schema_version: publisher_identity_schema_version(),
            user_id: identity.user_id,
            current_key_id: next_key.key_id.clone(),
            public_key: next_key.public_key.clone(),
        };
        if let Err(error) = save_current_signing_key(control_plane_root, &next_key)
            .and_then(|()| save_publisher_identity(control_plane_root, &next_identity))
        {
            return structured_error(
                500,
                json!({ "code": "publisher_identity_persist_failed", "message": error }),
            );
        }
    } else if let Err(error) = reset_local_publisher_identity(control_plane_root, &identity) {
        return structured_error(
            500,
            json!({ "code": "publisher_identity_persist_failed", "message": error }),
        );
    }
    list_publisher_identity(control_plane_root)
}

fn reveal_publisher_private_key(control_plane_root: &Path) -> Result<(u16, String)> {
    match load_current_signing_key(control_plane_root) {
        Ok(Some(key)) => Ok((
            200,
            serde_json::to_string(&json!({
                "keyId": key.key_id,
                "privateKey": key.private_key,
                "publicKey": key.public_key
            }))?,
        )),
        Ok(None) => structured_error(
            404,
            json!({ "code": "publisher_private_key_missing", "message": "Current publisher private key is unavailable" }),
        ),
        Err(error) => structured_error(
            500,
            json!({ "code": "publisher_private_key_failed", "message": error }),
        ),
    }
}

fn list_plugin_credentials(control_plane_root: &Path) -> Result<(u16, String)> {
    match CredentialStore::new(control_plane_root).summaries() {
        Ok(credentials) => Ok((
            200,
            serde_json::to_string(&json!({ "credentials": credentials }))?,
        )),
        Err(error) => structured_error(
            500,
            json!({ "code": "credential_store_failed", "message": error.to_string() }),
        ),
    }
}

fn save_plugin_credential(body: &str, control_plane_root: &Path) -> Result<(u16, String)> {
    let input: CredentialInput = match serde_json::from_str(body) {
        Ok(input) => input,
        Err(error) => {
            return structured_error(
                400,
                json!({ "code": "invalid_credential", "message": error.to_string() }),
            )
        }
    };
    match CredentialStore::new(control_plane_root).upsert(input) {
        Ok(credential) => Ok((
            200,
            serde_json::to_string(&json!({ "credential": credential }))?,
        )),
        Err(error) => structured_error(
            400,
            json!({ "code": "credential_store_failed", "message": error.to_string() }),
        ),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeleteCredentialRequest {
    name: String,
    #[serde(default)]
    scope: CredentialScope,
}

fn delete_plugin_credential(body: &str, control_plane_root: &Path) -> Result<(u16, String)> {
    let request: DeleteCredentialRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(error) => {
            return structured_error(
                400,
                json!({ "code": "invalid_credential_delete", "message": error.to_string() }),
            )
        }
    };
    match CredentialStore::new(control_plane_root).delete(&request.name, &request.scope) {
        Ok(true) => Ok((
            200,
            serde_json::to_string(&json!({ "deleted": true, "name": request.name }))?,
        )),
        Ok(false) => structured_error(
            404,
            json!({ "code": "credential_not_found", "message": "credential was not found" }),
        ),
        Err(error) => structured_error(
            400,
            json!({ "code": "credential_store_failed", "message": error.to_string() }),
        ),
    }
}

fn reveal_plugin_credential(body: &str, control_plane_root: &Path) -> Result<(u16, String)> {
    let request: DeleteCredentialRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(error) => {
            return structured_error(
                400,
                json!({ "code": "invalid_credential_reveal", "message": error.to_string() }),
            )
        }
    };
    if request.name.starts_with("loom-") {
        return structured_error(
            403,
            json!({ "code": "credential_reserved", "message": "System-reserved credentials cannot be revealed through this endpoint" }),
        );
    }
    match CredentialStore::new(control_plane_root).reveal(&request.name, &request.scope) {
        Ok(Some(credential)) => Ok((
            200,
            serde_json::to_string(&json!({ "credential": credential }))?,
        )),
        Ok(None) => structured_error(
            404,
            json!({ "code": "credential_not_found", "message": "credential was not found" }),
        ),
        Err(error) => structured_error(
            400,
            json!({ "code": "credential_store_failed", "message": error.to_string() }),
        ),
    }
}

fn install_framework(id: &str, framework_registry: &FrameworkRegistry) -> Result<(u16, String)> {
    match framework_registry.install(id) {
        Ok(status) => Ok((200, serde_json::to_string(&json!({ "framework": status }))?)),
        Err(error) => framework_error_response(error, "framework_install_failed", id),
    }
}

fn uninstall_framework(id: &str, framework_registry: &FrameworkRegistry) -> Result<(u16, String)> {
    match framework_registry.uninstall(id) {
        Ok(status) => Ok((200, serde_json::to_string(&json!({ "framework": status }))?)),
        Err(error) => framework_error_response(error, "framework_uninstall_failed", id),
    }
}

fn rollback_framework(id: &str, framework_registry: &FrameworkRegistry) -> Result<(u16, String)> {
    match framework_registry.rollback(id) {
        Ok(status) => Ok((200, serde_json::to_string(&json!({ "framework": status }))?)),
        Err(error) => framework_error_response(error, "framework_rollback_failed", id),
    }
}

fn install_framework_package(
    body: &str,
    framework_registry: &FrameworkRegistry,
) -> Result<(u16, String)> {
    let package = match decode_framework_package_request(body) {
        Ok(package) => package,
        Err(error) => {
            return structured_error(
                400,
                json!({
                    "code": "invalid_framework_package_request",
                    "message": error.to_string()
                }),
            )
        }
    };
    match framework_registry.install_framework_package_from_zip(&package) {
        Ok(status) => Ok((200, serde_json::to_string(&json!({ "framework": status }))?)),
        Err(error) => framework_error_response(error, "framework_install_failed", "package"),
    }
}

fn upgrade_framework_package(
    id: &str,
    body: &str,
    framework_registry: &FrameworkRegistry,
) -> Result<(u16, String)> {
    let package = match decode_framework_package_request(body) {
        Ok(package) => package,
        Err(error) => {
            return structured_error(
                400,
                json!({
                    "code": "invalid_framework_package_request",
                    "message": error.to_string()
                }),
            )
        }
    };
    match framework_registry.upgrade_framework_package(id, &package) {
        Ok(status) => Ok((200, serde_json::to_string(&json!({ "framework": status }))?)),
        Err(error) => framework_error_response(error, "framework_upgrade_failed", id),
    }
}

fn set_framework_enabled(
    id: &str,
    enabled: bool,
    framework_registry: &FrameworkRegistry,
) -> Result<(u16, String)> {
    let result = if enabled {
        framework_registry.enable(id)
    } else {
        framework_registry.disable(id)
    };
    match result {
        Ok(status) => Ok((200, serde_json::to_string(&json!({ "framework": status }))?)),
        Err(error) => framework_error_response(
            error,
            if enabled {
                "framework_enable_failed"
            } else {
                "framework_disable_failed"
            },
            id,
        ),
    }
}

fn decode_framework_package_request(body: &str) -> Result<Vec<u8>> {
    let request: FrameworkPackageRequest = serde_json::from_str(body)
        .map_err(|error| anyhow::anyhow!("invalid framework package request: {error}"))?;
    let encoded = request.zip_base64.trim();
    if encoded.is_empty() {
        return Err(anyhow::anyhow!("zipBase64 is required"));
    }
    let encoded = encoded
        .strip_prefix("data:application/zip;base64,")
        .unwrap_or(encoded);
    BASE64
        .decode(encoded)
        .map_err(|error| anyhow::anyhow!("invalid zipBase64: {error}"))
}

fn framework_error_response(
    error: loom_tool_registry::framework::FrameworkError,
    operation_code: &str,
    id: &str,
) -> Result<(u16, String)> {
    use loom_tool_registry::framework::FrameworkError;
    match error {
        FrameworkError::UnknownFramework(unknown_id) => structured_error(
            404,
            json!({
                "code": "unknown_framework",
                "message": format!("unknown framework `{unknown_id}`")
            }),
        ),
        FrameworkError::FrameworkNotInstalled(framework_id) => structured_error(
            409,
            json!({
                "code": "framework_not_installed",
                "message": format!("framework `{framework_id}` is not installed")
            }),
        ),
        FrameworkError::NoRollback { id } => structured_error(
            409,
            json!({
                "code": "framework_rollback_unavailable",
                "message": format!("framework `{id}` has no rollback version")
            }),
        ),
        FrameworkError::InvalidPackage {
            id: package_id,
            reason,
        } => structured_error(
            400,
            json!({
                "code": "invalid_framework_package",
                "message": format!("framework package `{package_id}` is invalid: {reason}")
            }),
        ),
        FrameworkError::CorruptState { path, reason } => structured_error(
            500,
            json!({
                "code": "framework_state_corrupt",
                "framework": id,
                "path": path,
                "message": format!(
                    "framework state file `{path}` is unreadable ({reason}); repair or remove the file before installing, toggling, or uninstalling the framework"
                )
            }),
        ),
        other => structured_error(
            500,
            json!({
                "code": operation_code,
                "framework": id,
                "message": other.to_string()
            }),
        ),
    }
}

// Report whether an art is runnable: its framework must be installed + ready.
fn tool_readiness(
    id: &str,
    tool_registry: &ToolRegistry,
    framework_registry: &FrameworkRegistry,
) -> Result<(u16, String)> {
    let tool = match tool_registry.get_tool(id) {
        Ok(Some(tool)) => tool,
        Ok(None) => {
            return structured_error(
                404,
                json!({ "code": "tool_not_found", "message": format!("tool `{id}` does not exist") }),
            )
        }
        Err(error) => return tool_registry_error_response(error),
    };
    let framework_id = loom_tool_registry::framework::framework_id_for_execution(&tool.execution);
    let installed = framework_registry.is_installed(framework_id);
    let (ready, detail) = if !tool.enabled {
        (false, "Art is disabled".to_owned())
    } else if installed {
        framework_registry.readiness(framework_id)
    } else {
        (false, "framework is not installed".to_owned())
    };
    Ok((
        200,
        serde_json::to_string(&json!({
            "toolId": id,
            "framework": framework_id,
            "frameworkInstalled": installed,
            "toolEnabled": tool.enabled,
            "ready": ready,
            "detail": detail,
        }))?,
    ))
}

fn put_tool(
    path_id: &str,
    body: &str,
    tool_registry: &ToolRegistry,
    hook_bridge: &SharedOtterBridgeRuntime,
) -> Result<(u16, String)> {
    let tool = match serde_json::from_str::<ToolDefinition>(body) {
        Ok(tool) => tool,
        Err(error) => return invalid_request(error.to_string()),
    };
    if tool.id != path_id {
        return id_mismatch("tool", path_id, &tool.id);
    }

    let saved = match tool_registry.save_tool(tool) {
        Ok(saved) => saved,
        Err(error) => return tool_registry_error_response(error),
    };
    broadcast_otter_bridge_json(hook_bridge, capabilities_updated_event());
    Ok((200, serde_json::to_string(&json!({ "tool": saved }))?))
}
