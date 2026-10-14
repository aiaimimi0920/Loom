// Explicit Otter connection grants are independent of persistent device pairing.
// A previously paired key alone never bypasses a manual per-connection decision.
const OTTER_CONNECTION_TTL_MS: u64 = 5 * 60_000;
const OTTER_CONNECTION_LIMIT: usize = 64;
const OTTER_CONNECTION_PUBLIC_LIMIT: usize = 48;

fn otter_connection_has_capacity(store: &DeviceRegistryStore, authenticated: bool) -> bool {
    store.connection_requests.len()
        < if authenticated {
            OTTER_CONNECTION_LIMIT
        } else {
            OTTER_CONNECTION_PUBLIC_LIMIT
        }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct OtterConnectionRequest {
    id: String,
    device_id: String,
    name: String,
    phase: String,
    expires_at_ms: u64,
    #[serde(skip)]
    receipt_digest: String,
    #[serde(skip)]
    device_epoch: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OtterConnectionInput {
    name: String,
    public_key: String,
    #[serde(default)]
    password: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OtterConnectionReceipt {
    request_id: String,
    receipt: String,
}

fn connection_error(status: u16, code: &str) -> Result<(u16, String)> {
    structured_error(status, json!({"code": code}))
}

struct OtterConnectionAuthentication {
    device_id: String,
    session_digest: String,
    epoch: u64,
}

impl OtterConnectionAuthentication {
    fn capture(
        store: &DeviceRegistryStore,
        actor: Option<&str>,
        token: Option<&str>,
        key: &str,
    ) -> Option<Self> {
        let device_id = actor?.to_owned();
        let session_digest = sha256_bytes(token?.as_bytes());
        let epoch = store.sessions.get(&session_digest)?.session_epoch;
        let proof = Self {
            device_id,
            session_digest,
            epoch,
        };
        proof.valid(store, key).then_some(proof)
    }

    fn valid(&self, store: &DeviceRegistryStore, key: &str) -> bool {
        store
            .sessions
            .get(&self.session_digest)
            .is_some_and(|session| {
                session.device_id == self.device_id
                    && session.session_epoch == self.epoch
                    && session.expires_at_ms > unix_time_millis()
                    && !session.revoked.load(Ordering::SeqCst)
                    && store.devices.get(&self.device_id).is_some_and(|device| {
                        device.enabled
                            && device.approval == "approved"
                            && device.session_epoch == self.epoch
                            && device.public_key.as_deref() == Some(key)
                    })
            })
    }
}

struct VerifiedOtterConnection {
    input: OtterConnectionInput,
    authentication: Option<OtterConnectionAuthentication>,
    policy: connection_policy::Store,
    revision: u64,
    created_at: u64,
}

fn create_otter_connection_request(
    body: &str,
    actor: Option<&str>,
    token: Option<&str>,
    registry: &SharedDeviceRegistryStore,
    bridge: &SharedOtterBridgeRuntime,
) -> Result<(u16, String)> {
    let input: OtterConnectionInput = match serde_json::from_str(body) {
        Ok(input) => input,
        Err(_) => return connection_error(400, "connection_input_invalid"),
    };
    if input.name.trim().is_empty()
        || input.name.len() > 80
        || input.name.chars().any(char::is_control)
        || input.password.len() > 256
        || input.public_key.len() != 44
        || input.public_key.trim() != input.public_key
    {
        return connection_error(400, "connection_input_invalid");
    }
    if let Err(error) = device_public_key_fingerprint(&input.public_key) {
        return device_auth_error_response(error);
    }
    let now = unix_time_millis();
    let (policy, authentication, revision) = {
        let mut store = registry
            .lock()
            .map_err(|_| anyhow::anyhow!("device registry unavailable"))?;
        store
            .connection_requests
            .retain(|_, request| request.expires_at_ms > now);
        let authentication =
            OtterConnectionAuthentication::capture(&store, actor, token, &input.public_key);
        let authenticated = authentication.is_some();
        // Anonymous traffic cannot consume the authenticated admission budget or final slots.
        if !otter_connection_has_capacity(&store, authenticated) {
            return connection_error(429, "connection_rate_limited");
        }
        let attempts = if authenticated {
            &mut store.connection_authenticated_attempts
        } else {
            &mut store.connection_attempts
        };
        attempts.retain(|time| now.saturating_sub(*time) < 60_000);
        if attempts.len() >= 20 {
            return connection_error(429, "connection_rate_limited");
        }
        attempts.push(now);
        (
            store.connection_policy.clone(),
            authentication,
            store.connection_policy_revision,
        )
    };
    let info = policy.public_info();
    let trusted =
        info.approval == connection_policy::Approval::Authenticated && authentication.is_some();
    // Password work is bounded by admission above and runs outside the registry lock.
    if !trusted && !policy.check_password(&input.password) {
        return connection_error(403, "connection_password_invalid");
    }
    finish_otter_connection_request(
        VerifiedOtterConnection {
            input,
            authentication,
            policy,
            revision,
            created_at: now,
        },
        registry,
        bridge,
    )
}

fn finish_otter_connection_request(
    verified: VerifiedOtterConnection,
    registry: &SharedDeviceRegistryStore,
    bridge: &SharedOtterBridgeRuntime,
) -> Result<(u16, String)> {
    let VerifiedOtterConnection {
        input,
        authentication,
        policy,
        revision,
        created_at,
    } = verified;
    let info = policy.public_info();
    let authenticated = authentication.is_some();
    let trusted = info.approval == connection_policy::Approval::Authenticated && authenticated;
    // Revoke the original proof, not merely the public key: deletion and re-pairing is a new device.
    {
        let store = registry
            .lock()
            .map_err(|_| anyhow::anyhow!("device registry unavailable"))?;
        if authentication
            .as_ref()
            .is_some_and(|proof| !proof.valid(&store, &input.public_key))
        {
            return connection_error(403, "connection_authentication_revoked");
        }
    }
    let registration = serde_json::to_string(&json!({
        "name": input.name, "kind": "computer", "address": "otter://connection",
        "publicKey": input.public_key,
    }))?;
    let (status, response) = add_managed_device(&registration, "pending", registry, bridge)?;
    if status != 200 {
        return Ok((status, response));
    }
    let mut store = registry
        .lock()
        .map_err(|_| anyhow::anyhow!("device registry unavailable"))?;
    // Do not admit against a policy that changed during the expensive verification.
    if store.connection_policy_revision != revision {
        return connection_error(409, "connection_policy_changed");
    }
    if authentication
        .as_ref()
        .is_some_and(|proof| !proof.valid(&store, &input.public_key))
    {
        return connection_error(403, "connection_authentication_revoked");
    }
    // Verification releases the lock; enforce capacity again at the insertion boundary.
    store
        .connection_requests
        .retain(|_, request| request.expires_at_ms > unix_time_millis());
    if !otter_connection_has_capacity(&store, authenticated) {
        return connection_error(429, "connection_rate_limited");
    }
    let device = store
        .devices
        .values()
        .find(|device| {
            device.public_key.as_deref() == Some(input.public_key.as_str())
                && device.approval != "rejected"
        })
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("pairing receipt missing"))?;
    if !device.enabled {
        return connection_error(403, "device_disabled");
    }
    let automatic = trusted
        || (info.approval == connection_policy::Approval::Password && info.password_required);
    if automatic && device.approval != "approved" {
        let mut approved = device.clone();
        approved.approval = "approved".into();
        approved.session_epoch = approved.session_epoch.saturating_add(1);
        store.devices.insert(device.id.clone(), approved);
        if let Err(error) = store.persist() {
            store.devices.insert(device.id.clone(), device);
            return Err(error);
        }
    }
    let receipt = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let device_epoch = store.devices[&device.id].session_epoch;
    let request = OtterConnectionRequest {
        id: format!("connection-{}", Uuid::new_v4()),
        device_id: device.id,
        name: input.name.trim().to_owned(),
        phase: if automatic { "approved" } else { "pending" }.into(),
        expires_at_ms: created_at.saturating_add(OTTER_CONNECTION_TTL_MS),
        receipt_digest: sha256_bytes(receipt.as_bytes()),
        device_epoch,
    };
    let response = json!({"connection": request, "receipt": receipt, "loom": info});
    store
        .connection_requests
        .insert(request.id.clone(), request);
    Ok((201, serde_json::to_string(&response)?))
}

fn otter_connection_receipt_matches(
    request: &OtterConnectionRequest,
    receipt: &str,
    now: u64,
) -> bool {
    receipt.len() == 64
        && request.expires_at_ms > now
        && request.receipt_digest == sha256_bytes(receipt.as_bytes())
}

fn poll_otter_connection_request(
    body: &str,
    registry: &SharedDeviceRegistryStore,
    cancel: bool,
) -> Result<(u16, String)> {
    let input: OtterConnectionReceipt = match serde_json::from_str(body) {
        Ok(input) => input,
        Err(_) => return connection_error(400, "connection_input_invalid"),
    };
    let mut store = registry
        .lock()
        .map_err(|_| anyhow::anyhow!("device registry unavailable"))?;
    let now = unix_time_millis();
    let Some(request) = store.connection_requests.get(&input.request_id) else {
        return connection_error(404, "connection_request_unavailable");
    };
    if !otter_connection_receipt_matches(request, &input.receipt, now) {
        return connection_error(404, "connection_request_unavailable");
    }
    let device_valid = store.devices.get(&request.device_id).is_some_and(|device| {
        device.enabled
            && device.approval != "rejected"
            && (!matches!(request.phase.as_str(), "approved" | "connected")
                || (device.approval == "approved" && device.session_epoch == request.device_epoch))
    });
    let request = store
        .connection_requests
        .get_mut(&input.request_id)
        .unwrap();
    if cancel {
        request.phase = "cancelled".into();
    } else if !device_valid {
        request.phase = "rejected".into();
    }
    Ok((200, serde_json::to_string(request)?))
}

fn decide_otter_connection_request(
    body: &str,
    registry: &SharedDeviceRegistryStore,
) -> Result<(u16, String)> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Decision {
        request_id: String,
        approve: bool,
    }
    let input: Decision = match serde_json::from_str(body) {
        Ok(input) => input,
        Err(_) => return connection_error(400, "connection_input_invalid"),
    };
    let mut store = registry
        .lock()
        .map_err(|_| anyhow::anyhow!("device registry unavailable"))?;
    let Some(request) = store.connection_requests.get(&input.request_id).cloned() else {
        return connection_error(404, "connection_request_unavailable");
    };
    if request.phase != "pending" || request.expires_at_ms <= unix_time_millis() {
        return connection_error(409, "connection_request_not_pending");
    }
    if input.approve {
        let Some(previous) = store
            .devices
            .get(&request.device_id)
            .cloned()
            .filter(|device| device.enabled && device.approval != "rejected")
        else {
            return connection_error(403, "device_disabled");
        };
        let mut device = previous.clone();
        if device.approval != "approved" {
            device.approval = "approved".into();
            device.session_epoch = device.session_epoch.saturating_add(1);
            store.devices.insert(device.id.clone(), device);
            if let Err(error) = store.persist() {
                store.devices.insert(previous.id.clone(), previous);
                return Err(error);
            }
        }
    }
    let device_epoch = store
        .devices
        .get(&request.device_id)
        .map(|device| device.session_epoch);
    let request = store
        .connection_requests
        .get_mut(&input.request_id)
        .unwrap();
    request.phase = if input.approve {
        "approved"
    } else {
        "rejected"
    }
    .into();
    if let Some(epoch) = device_epoch {
        request.device_epoch = epoch;
    }
    Ok((200, serde_json::to_string(request)?))
}
