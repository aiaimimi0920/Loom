// Device-scoped long polling and snapshot recovery for Surface streams.
fn poll_surface_stream(
    path: &str,
    hook_bridge: &SharedOtterBridgeRuntime,
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

