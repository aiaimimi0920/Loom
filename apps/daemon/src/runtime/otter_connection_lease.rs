// A live connection requires both a per-connection approval and proof of the device key.
// Heartbeats extend a short lease; a crashed or disconnected Otter cannot stay online forever.
fn activate_otter_connection(
    request: &ParsedHttpRequest,
    actor: Option<&str>,
    registry: &SharedDeviceRegistryStore,
) -> Result<(u16, String)> {
    let input: OtterConnectionReceipt = match serde_json::from_str(&request.body) {
        Ok(input) => input,
        Err(_) => return connection_error(400, "connection_input_invalid"),
    };
    let mut store = registry
        .lock()
        .map_err(|_| anyhow::anyhow!("device registry unavailable"))?;
    let Some(connection) = store.connection_requests.get(&input.request_id) else {
        return connection_error(404, "connection_request_unavailable");
    };
    if !otter_connection_receipt_matches(connection, &input.receipt, unix_time_millis())
        || !matches!(connection.phase.as_str(), "approved" | "connected")
        || actor != Some(connection.device_id.as_str())
    {
        return connection_error(403, "connection_not_authorized");
    }
    let key = store
        .devices
        .get(&connection.device_id)
        .and_then(|device| device.public_key.as_deref());
    let authorized = key
        .and_then(|key| {
            OtterConnectionAuthentication::capture(
                &store,
                actor,
                request.authorization_credential("Device"),
                key,
            )
        })
        .is_some_and(|proof| proof.epoch == connection.device_epoch);
    if !authorized {
        return connection_error(403, "connection_authentication_revoked");
    }
    let device_id = connection.device_id.clone();
    if !store.devices[&device_id].otter_connected {
        store.devices.get_mut(&device_id).unwrap().otter_connected = true;
        if let Err(error) = store.persist() {
            store.devices.get_mut(&device_id).unwrap().otter_connected = false;
            return Err(error);
        }
    }
    let loom = store.connection_info();
    let connection = store
        .connection_requests
        .get_mut(&input.request_id)
        .unwrap();
    connection.phase = "connected".into();
    connection.expires_at_ms = unix_time_millis().saturating_add(30_000);
    Ok((
        200,
        serde_json::to_string(&json!({"connection":connection,"loom":loom}))?,
    ))
}
