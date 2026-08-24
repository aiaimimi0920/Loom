// Device input validation, session authentication, and attachment authorization.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManagedDeviceInput {
    name: String,
    kind: ManagedDeviceKind,
    address: String,
    #[serde(default)]
    public_key: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManagedDeviceUpdate {
    name: String,
    kind: ManagedDeviceKind,
    address: String,
    enabled: bool,
}

fn validate_device_auth_identifier(
    label: &str,
    value: &str,
) -> std::result::Result<(), DeviceAuthError> {
    if value.is_empty()
        || value.len() > 160
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:/-".contains(&byte))
    {
        return Err(DeviceAuthError::new(
            400,
            "device_auth_invalid",
            format!("{label} is not a valid protocol identifier"),
        ));
    }
    Ok(())
}

fn validate_device_auth_nonce(nonce: &str) -> std::result::Result<(), DeviceAuthError> {
    if nonce.len() < 16
        || nonce.len() > 160
        || !nonce
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(DeviceAuthError::new(
            400,
            "device_nonce_invalid",
            "device request nonce must be 16 to 160 URL-safe characters",
        ));
    }
    Ok(())
}

fn decode_device_public_key(encoded: &str) -> std::result::Result<VerifyingKey, DeviceAuthError> {
    let bytes = BASE64.decode(encoded.trim()).map_err(|_| {
        DeviceAuthError::new(
            400,
            "device_public_key_invalid",
            "device public key is not valid Base64",
        )
    })?;
    let bytes: [u8; 32] = bytes.try_into().map_err(|_| {
        DeviceAuthError::new(
            400,
            "device_public_key_invalid",
            "device public key must contain 32 Ed25519 bytes",
        )
    })?;
    VerifyingKey::from_bytes(&bytes).map_err(|_| {
        DeviceAuthError::new(
            400,
            "device_public_key_invalid",
            "device public key is not a valid Ed25519 key",
        )
    })
}

fn device_public_key_fingerprint(encoded: &str) -> std::result::Result<String, DeviceAuthError> {
    let key = decode_device_public_key(encoded)?;
    Ok(format!("sha256:{}", sha256_bytes(key.as_bytes())))
}

fn is_public_device_auth_route(method: &str, path: &str) -> bool {
    path == "/health"
        || (method == "POST"
            && matches!(
                path,
                "/v1/devices/requests" | "/v1/device-sessions/challenges" | "/v1/device-sessions"
            ))
}

fn device_session_route_allowed(method: &str, path: &str) -> bool {
    if method == "GET" {
        return path == "/v1/capabilities"
            || path == "/v1/surfaces/stream"
            || path.starts_with("/v1/surfaces/resources/")
            || path == "/v1/hook-bridge/status";
    }
    if method != "POST" {
        return false;
    }
    path == "/v1/surfaces/actions/cancel"
        || path == "/v1/surfaces/attach"
        || path == "/v1/surfaces/confirmations/decision"
        || path_id_with_suffix(path, "/v1/surfaces/instances/", "/attachments").is_some()
        || path_id_with_suffix(path, "/v1/surfaces/instances/", "/events").is_some()
        || path_id_with_suffix(path, "/v1/surfaces/instances/", "/lifecycle").is_some()
}

fn authenticate_http_device_session(
    request: &ParsedHttpRequest,
    device_registry: &SharedDeviceRegistryStore,
) -> std::result::Result<Option<String>, DeviceAuthError> {
    let Some(token) = request.authorization_credential("Device") else {
        return Ok(None);
    };
    let nonce = request.header("X-Loom-Device-Nonce").ok_or_else(|| {
        DeviceAuthError::new(
            400,
            "device_nonce_missing",
            "authenticated device requests require X-Loom-Device-Nonce",
        )
    })?;
    let device_id = device_registry
        .lock()
        .map_err(|_| {
            DeviceAuthError::new(
                503,
                "device_registry_unavailable",
                "device registry is unavailable",
            )
        })?
        .authenticate_device_session(token, nonce)?;
    Ok(Some(device_id))
}

fn validate_authenticated_device_identity(
    authenticated_device_id: Option<&str>,
    expected_device_id: &str,
) -> std::result::Result<(), DeviceAuthError> {
    if authenticated_device_id.is_some_and(|device_id| device_id != expected_device_id) {
        return Err(DeviceAuthError::new(
            403,
            "device_identity_mismatch",
            "device session cannot act on behalf of another device",
        ));
    }
    Ok(())
}

fn validate_authenticated_surface_attachment(
    authenticated_device_id: Option<&str>,
    instance_id: &str,
    attachment_id: &str,
    surface_instances: &SharedSurfaceInstanceStore,
) -> std::result::Result<(), DeviceAuthError> {
    let Some(authenticated_device_id) = authenticated_device_id else {
        return Ok(());
    };
    let store = surface_instances.lock().map_err(|_| {
        DeviceAuthError::new(
            503,
            "surface_store_unavailable",
            "Surface instance store is unavailable",
        )
    })?;
    let instance = store.get(instance_id).ok_or_else(|| {
        DeviceAuthError::new(
            404,
            "surface_instance_not_found",
            "Surface instance was not found",
        )
    })?;
    let attachment = instance.attachments.get(attachment_id).ok_or_else(|| {
        DeviceAuthError::new(
            404,
            "surface_attachment_not_found",
            "Surface attachment was not found",
        )
    })?;
    validate_authenticated_device_identity(
        Some(authenticated_device_id),
        &attachment.descriptor.device_id,
    )
}
