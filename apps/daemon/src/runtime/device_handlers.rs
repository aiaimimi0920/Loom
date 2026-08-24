// Device handlers, route identifiers, app paths, and Surface request models.
fn managed_devices_response(
    device_registry: &SharedDeviceRegistryStore,
    hook_bridge: &SharedHookBridgeRuntime,
) -> Result<(u16, String)> {
    let connected_clients = hook_bridge
        .lock()
        .map_err(|_| anyhow::anyhow!("hook bridge state is unavailable"))?
        .connected_clients
        .load(Ordering::SeqCst);
    let store = device_registry
        .lock()
        .map_err(|_| anyhow::anyhow!("device registry is unavailable"))?;
    let mut devices = Vec::new();
    let mut pending = Vec::new();
    for device in store.devices.values().cloned() {
        if device.approval == "pending" {
            pending.push(device);
        } else {
            devices.push(device);
        }
    }
    Ok((
        200,
        serde_json::to_string(&json!({
            "devices": devices,
            "pending": pending,
            "connectedClients": connected_clients,
        }))?,
    ))
}

fn list_managed_devices(
    device_registry: &SharedDeviceRegistryStore,
    hook_bridge: &SharedHookBridgeRuntime,
) -> Result<(u16, String)> {
    managed_devices_response(device_registry, hook_bridge)
}

fn poll_surface_stream(
    path: &str,
    hook_bridge: &SharedHookBridgeRuntime,
    surface_instances: &SharedSurfaceInstanceStore,
    authenticated_device_id: Option<&str>,
) -> Result<(u16, String)> {
    let after = query_value(path, "after")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or_default();
    let timeout_ms = query_value(path, "timeoutMs")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(5_000)
        .min(5_000);
    let hub = hook_bridge
        .lock()
        .map_err(|_| anyhow::anyhow!("hook bridge runtime is unavailable"))?
        .broadcast_hub
        .clone();
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let mut cursor = after.max(HOOK_BRIDGE_RECOVERY_CURSOR);
    let mut reset = false;
    let mut messages = if after == 0 {
        surface_snapshot_recovery_messages_for_device(surface_instances, authenticated_device_id)
            .into_iter()
            .filter_map(|message| serde_json::from_str::<Value>(&message).ok())
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    loop {
        let remaining = if messages.is_empty() {
            deadline.saturating_duration_since(Instant::now())
        } else {
            Duration::ZERO
        };
        let (next, batch_reset, entries) = hub.wait_after(cursor, remaining);
        cursor = next;
        reset |= batch_reset;
        for entry in entries {
            let Ok(message) = serde_json::from_str::<Value>(&entry.message) else {
                continue;
            };
            if surface_message_visible_to_device(
                &message,
                authenticated_device_id,
                surface_instances,
            ) {
                messages.push(message);
            }
            if messages.len() >= HOOK_BRIDGE_POLL_MAX_MESSAGES {
                break;
            }
        }
        if !messages.is_empty() || Instant::now() >= deadline || (cursor == after && !reset) {
            break;
        }
    }
    if after != 0 && reset {
        messages.extend(
            surface_snapshot_recovery_messages_for_device(
                surface_instances,
                authenticated_device_id,
            )
            .into_iter()
            .filter_map(|message| serde_json::from_str::<Value>(&message).ok()),
        );
    }
    Ok((
        200,
        serde_json::to_string(&json!({
            "protocolVersion": loom_protocol::SURFACE_STREAM_PROTOCOL_VERSION,
            "next": cursor,
            "reset": reset,
            "messages": messages,
        }))?,
    ))
}

fn create_device_session_challenge(
    body: &str,
    device_registry: &SharedDeviceRegistryStore,
) -> Result<(u16, String)> {
    let input = match serde_json::from_str::<DeviceSessionChallengeRequest>(body) {
        Ok(input) => input,
        Err(error) => return invalid_request(format!("invalid device challenge payload: {error}")),
    };
    let response = device_registry
        .lock()
        .map_err(|_| anyhow::anyhow!("device registry is unavailable"))?
        .create_session_challenge(input.device_id.trim());
    match response {
        Ok(response) => Ok((201, serde_json::to_string(&response)?)),
        Err(error) => device_auth_error_response(error),
    }
}

fn issue_device_session(
    body: &str,
    device_registry: &SharedDeviceRegistryStore,
) -> Result<(u16, String)> {
    let input = match serde_json::from_str::<DeviceSessionIssueRequest>(body) {
        Ok(input) => input,
        Err(error) => return invalid_request(format!("invalid device session payload: {error}")),
    };
    let response = device_registry
        .lock()
        .map_err(|_| anyhow::anyhow!("device registry is unavailable"))?
        .issue_device_session(input);
    match response {
        Ok(response) => Ok((201, serde_json::to_string(&response)?)),
        Err(error) => device_auth_error_response(error),
    }
}

fn add_managed_device(
    body: &str,
    approval: &str,
    device_registry: &SharedDeviceRegistryStore,
    hook_bridge: &SharedHookBridgeRuntime,
) -> Result<(u16, String)> {
    let input = match serde_json::from_str::<ManagedDeviceInput>(body) {
        Ok(input) => input,
        Err(error) => return invalid_request(format!("invalid device payload: {error}")),
    };
    let name = input.name.trim();
    let address = input.address.trim();
    if name.is_empty() || name.len() > 80 {
        return invalid_request("device name must contain 1 to 80 characters");
    }
    if address.is_empty() || address.len() > 240 {
        return invalid_request("device address must contain 1 to 240 characters");
    }
    let public_key = input
        .public_key
        .as_deref()
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .map(str::to_owned);
    if approval == "pending" && public_key.is_none() {
        return structured_error(
            400,
            json!({
                "code": "device_public_key_required",
                "message": "a device pairing request must include its Ed25519 public key",
            }),
        );
    }
    let key_fingerprint = match public_key.as_deref() {
        Some(key) => match device_public_key_fingerprint(key) {
            Ok(fingerprint) => Some(fingerprint),
            Err(error) => return device_auth_error_response(error),
        },
        None => None,
    };

    let created_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let id = format!(
        "device-{created_at:x}-{:x}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            & 0xfffff
    );
    {
        let mut store = device_registry
            .lock()
            .map_err(|_| anyhow::anyhow!("device registry is unavailable"))?;
        if let Some(fingerprint) = key_fingerprint.as_deref() {
            if store.devices.values().any(|device| {
                device.key_fingerprint.as_deref() == Some(fingerprint)
                    && device.approval != "rejected"
            }) {
                if approval == "pending" {
                    drop(store);
                    return managed_devices_response(device_registry, hook_bridge);
                }
                return structured_error(
                    409,
                    json!({
                        "code": "device_key_already_registered",
                        "message": "this device public key is already registered",
                    }),
                );
            }
        }
        store.devices.insert(
            id.clone(),
            ManagedDevice {
                id,
                name: name.to_owned(),
                kind: input.kind,
                address: address.to_owned(),
                approval: approval.to_owned(),
                created_at,
                last_seen_at: None,
                is_local: false,
                enabled: true,
                public_key,
                key_fingerprint,
                session_epoch: u64::from(approval == "approved"),
            },
        );
        if let Err(error) = store.persist() {
            store
                .devices
                .retain(|_, device| device.created_at != created_at);
            return Err(error);
        }
    }
    managed_devices_response(device_registry, hook_bridge)
}
