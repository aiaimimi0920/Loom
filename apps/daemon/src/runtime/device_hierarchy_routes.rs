// Two-level Host Device -> App Sessions hierarchy and Cloud-Relay-tunneled MCP dispatch.

fn build_devices_hierarchy_response(
    device_registry: &SharedDeviceRegistryStore,
    hook_bridge: &SharedOtterBridgeRuntime,
    control_plane_root: &Path,
) -> Result<(u16, String)> {
    let connected_ws_clients = hook_bridge
        .lock()
        .map(|b| b.connected_clients.load(Ordering::SeqCst))
        .unwrap_or(0);
    let mut store = device_registry
        .lock()
        .map_err(|_| anyhow::anyhow!("device registry is unavailable"))?;
    let now = unix_time_millis();
    store.expire_pending_pairings(now)?;

    let mut hosts = Vec::new();
    for device in store.devices.values() {
        if device.approval != "approved" {
            continue;
        }
        let activity = store
            .sessions
            .values()
            .filter(|session| {
                session.device_id == device.id
                    && session.session_epoch == device.session_epoch
                    && session.expires_at_ms > now
                    && !session.revoked.load(Ordering::SeqCst)
            })
            .map(|session| session.last_activity_at_ms)
            .max();
        let online = device.is_local
            || (device.enabled
                && device.public_key.is_some()
                && activity.is_some_and(|at| now.saturating_sub(at) < 30_000));
        let sync_state = otter_sync::device_sync_state(&device.id);
        let otter_connected = device.otter_connected
            || (device.is_local && connected_ws_clients > 0)
            || sync_state.is_some()
            || online;
        let otter_app = json!({
            "appId": "otter",
            "displayName": "Otter",
            "connected": otter_connected,
            "executionState": sync_state
                .as_ref()
                .map(|s| s.execution_state.as_str())
                .unwrap_or(if otter_connected { "idle" } else { "offline" }),
            "activeWorkflowId": sync_state.as_ref().and_then(|s| s.workflow_id.clone()),
            "activeWorkflowName": sync_state.as_ref().and_then(|s| s.workflow_name.clone()),
            "nodeCount": sync_state.as_ref().map(|s| s.node_count).unwrap_or(0),
            "updatedAtMs": sync_state.as_ref().map(|s| s.updated_at_ms).or(activity).or(device.last_seen_at),
        });
        hosts.push(json!({
            "hostId": device.id,
            "name": device.name,
            "kind": device.kind,
            "address": device.address,
            "isLocal": device.is_local,
            "enabled": device.enabled,
            "online": online,
            "lastSeenAt": activity.or(device.last_seen_at),
            "terminalAgentSlotReady": true,
            "apps": [otter_app],
        }));
    }

    let sync_summary = otter_sync::summary_snapshot(control_plane_root);
    Ok((
        200,
        serde_json::to_string(&json!({
            "schemaVersion": 2,
            "model": "host_to_apps",
            "hosts": hosts,
            "otterSync": sync_summary,
        }))?,
    ))
}

fn route_cloud_relay_mcp_dispatch(
    runtime: &DaemonRuntime,
    request: &ParsedHttpRequest,
) -> Option<Result<(u16, String)>> {
    let route_path = request
        .path
        .split('?')
        .next()
        .unwrap_or(request.path.as_str());
    if request.method != "POST" || route_path != "/v1/cloud-relay/mcp" {
        return None;
    }
    let cfg = cloud_relay::load_config(&runtime.control_plane_root);
    if !cfg.enabled || !cfg.expose_mcp {
        return Some(structured_error(
            409,
            json!({
                "code": "cloud_relay_mcp_disabled",
                "message": "Cloud Relay MCP exposure is disabled on this Loom hub"
            }),
        ));
    }
    let status = cloud_relay::status_value(&runtime.control_plane_root);
    if status.get("signedIn").and_then(Value::as_bool) != Some(true) {
        return Some(structured_error(
            401,
            json!({
                "code": "cloud_relay_naccount_required",
                "message": "Cloud Relay MCP requires signed-in NAccount state"
            }),
        ));
    }
    let Some(bearer) = request.authorization_credential("Bearer") else {
        return Some(structured_error(
            401,
            json!({ "code": "missing_mcp_share_token" }),
        ));
    };
    let Some(grant) =
        mcp_share_tokens::authenticate_share_token(&runtime.control_plane_root, bearer)
    else {
        return Some(structured_error(
            401,
            json!({ "code": "invalid_mcp_share_token" }),
        ));
    };
    Some(dispatch_mcp_share_jsonrpc(runtime, &grant, &request.body))
}
