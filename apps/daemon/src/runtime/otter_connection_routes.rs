// Public identity/receipt routes and administrator-only connection policy/decisions.
fn route_otter_connections(
    request: &ParsedHttpRequest,
    path: &str,
    actor: Option<&str>,
    admin: bool,
    registry: &SharedDeviceRegistryStore,
    bridge: &SharedOtterBridgeRuntime,
) -> Result<(u16, String)> {
    match (request.method.as_str(), path) {
        ("GET", "/v1/otter/identity") => {
            let store = registry
                .lock()
                .map_err(|_| anyhow::anyhow!("device registry unavailable"))?;
            Ok((200, serde_json::to_string(&store.connection_info())?))
        }
        ("POST", "/v1/otter/connections/request") => create_otter_connection_request(
            &request.body,
            actor,
            request.authorization_credential("Device"),
            registry,
            bridge,
            request.header("X-Loom-Lan-Ingress"),
        ),
        ("POST", "/v1/otter/connections/status") => {
            poll_otter_connection_request(&request.body, registry, false)
        }
        ("POST", "/v1/otter/connections/cancel") => {
            poll_otter_connection_request(&request.body, registry, true)
        }
        ("POST", "/v1/otter/connections/activate") => {
            activate_otter_connection(request, actor, registry)
        }
        _ if !admin => connection_error(403, "connection_admin_required"),
        ("GET", "/v1/otter/connections/lan") => {
            let store = registry
                .lock()
                .map_err(|_| anyhow::anyhow!("device registry unavailable"))?;
            Ok((200, serde_json::to_string(&store.lan.view())?))
        }
        ("PUT", "/v1/otter/connections/lan") => {
            let input: lan_listener::Update = match serde_json::from_str(&request.body) {
                Ok(input) => input,
                Err(_) => return Ok((400, json!({"error":{"code":"lan_input_invalid","message":"请填写有效的局域网 IPv4 地址和端口"}}).to_string())),
            };
            let mut store = registry
                .lock()
                .map_err(|_| anyhow::anyhow!("device registry unavailable"))?;
            let previous = store.lan.endpoint();
            let retired = match store.lan.update(input) {
                Ok(retired) => retired,
                Err(error) => {
                    // Manager supplies fixed public context, never OS paths or private key data.
                    return Ok((400, json!({"error":{"code":"lan_configuration_rejected","message":error.to_string()}}).to_string()));
                }
            };
            if previous != store.lan.endpoint() {
                let ids: BTreeSet<_> = store
                    .connection_requests
                    .values()
                    .filter(|r| r.lan)
                    .map(|r| r.device_id.clone())
                    .collect();
                store.connection_requests.retain(|_, request| !request.lan);
                for session in store
                    .sessions
                    .values()
                    .filter(|s| ids.contains(&s.device_id))
                {
                    session.revoked.store(true, Ordering::SeqCst);
                }
                store.connection_policy_revision = store.connection_policy_revision.wrapping_add(1);
            }
            let response = serde_json::to_string(&store.lan.view())?;
            drop(store);
            drop(retired);
            Ok((200, response))
        }
        ("GET", "/v1/otter/connections") => {
            let mut store = registry
                .lock()
                .map_err(|_| anyhow::anyhow!("device registry unavailable"))?;
            store
                .connection_requests
                .retain(|_, request| request.expires_at_ms > unix_time_millis());
            Ok((
                200,
                serde_json::to_string(&json!({
                    "loom": store.connection_info(),
                    "requests": store.connection_requests.values().filter(|request| request.phase == "pending").collect::<Vec<_>>(),
                }))?,
            ))
        }
        ("PUT", "/v1/otter/connections/policy") => {
            let input = match serde_json::from_str::<connection_policy::Update>(&request.body) {
                Ok(input) => input,
                Err(_) => return connection_error(400, "connection_input_invalid"),
            };
            let (previous, revision) = {
                let store = registry
                    .lock()
                    .map_err(|_| anyhow::anyhow!("device registry unavailable"))?;
                (
                    store.connection_policy.clone(),
                    store.connection_policy_revision,
                )
            };
            let next = match previous.prepare_update(input) {
                Ok(next) => next,
                Err(_) => return connection_error(400, "connection_policy_invalid"),
            };
            let mut store = registry
                .lock()
                .map_err(|_| anyhow::anyhow!("device registry unavailable"))?;
            if revision != store.connection_policy_revision {
                return connection_error(409, "connection_policy_changed");
            }
            next.persist()?;
            let info = next.public_info();
            store.connection_policy = next;
            store.connection_policy_revision = store.connection_policy_revision.wrapping_add(1);
            store.connection_requests.clear();
            Ok((200, serde_json::to_string(&info)?))
        }
        ("POST", "/v1/otter/connections/decision") => {
            decide_otter_connection_request(&request.body, registry)
        }
        _ => connection_error(404, "connection_route_not_found"),
    }
}
