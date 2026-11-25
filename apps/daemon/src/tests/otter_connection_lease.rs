fn approved_otter_receipt(f: &Fixture, id: &str, token: &str) -> serde_json::Value {
    let key = f.devices.lock().unwrap().devices[id]
        .public_key
        .clone()
        .unwrap();
    let (status, created) = wall_http::device(
        f.port,
        token,
        "POST",
        "/v1/otter/connections/request",
        Some(json!({"name":"Lease Otter","publicKey":key})),
    );
    assert_eq!(status, 201, "{created}");
    let id = created["connection"]["id"].clone();
    assert_eq!(created["connection"]["phase"], "approved");
    json!({"requestId":id,"receipt":created["receipt"]})
}

#[test]
fn otter_connection_lease_requires_approval_receipt_and_bound_signed_device() {
    let f = Fixture::new();
    let (id, token) = wall_http::pair(f.port, "Lease Otter");
    let (_, other) = wall_http::pair(f.port, "Other Otter");
    let receipt = approved_otter_receipt(&f, &id, &token);
    let path = "/v1/otter/connections/activate";
    assert_eq!(
        wall_http::public(f.port, "POST", path, Some(receipt.clone())).0,
        401
    );
    assert_eq!(
        wall_http::device(f.port, &other, "POST", path, Some(receipt.clone())).0,
        403
    );
    assert_eq!(
        wall_http::admin(f.port, "POST", path, Some(receipt.clone())).0,
        403
    );
    for _ in 0..2 {
        let before = unix_time_millis();
        let (status, active) =
            wall_http::device(f.port, &token, "POST", path, Some(receipt.clone()));
        assert_eq!(status, 200, "{active}");
        assert_eq!(active["connection"]["phase"], "connected");
        assert_eq!(active["connection"]["deviceId"], id);
        let expiry = active["connection"]["expiresAtMs"].as_u64().unwrap();
        assert!(expiry >= before + 30_000 && expiry <= unix_time_millis() + 30_000);
    }
    wall_http::public(
        f.port,
        "POST",
        "/v1/otter/connections/cancel",
        Some(receipt.clone()),
    );
    assert_eq!(
        wall_http::device(f.port, &token, "POST", path, Some(receipt)).0,
        403
    );
}

#[test]
fn otter_connection_live_lease_expires_and_reports_epoch_revocation() {
    let f = Fixture::new();
    let (id, token) = wall_http::pair(f.port, "Lease Otter");
    let receipt = approved_otter_receipt(&f, &id, &token);
    let path = "/v1/otter/connections/activate";
    assert_eq!(
        wall_http::device(f.port, &token, "POST", path, Some(receipt.clone())).0,
        200
    );
    f.devices
        .lock()
        .unwrap()
        .devices
        .get_mut(&id)
        .unwrap()
        .session_epoch += 1;
    let (_, status) = wall_http::public(
        f.port,
        "POST",
        "/v1/otter/connections/status",
        Some(receipt),
    );
    assert_eq!(status["phase"], "rejected");
    let (id, token) = wall_http::pair(f.port, "Expiry Otter");
    let receipt = approved_otter_receipt(&f, &id, &token);
    assert_eq!(
        wall_http::device(f.port, &token, "POST", path, Some(receipt.clone())).0,
        200
    );
    f.devices
        .lock()
        .unwrap()
        .connection_requests
        .get_mut(receipt["requestId"].as_str().unwrap())
        .unwrap()
        .expires_at_ms = 0;
    assert_eq!(
        wall_http::device(f.port, &token, "POST", path, Some(receipt)).0,
        403
    );
}

#[test]
fn otter_connection_disconnect_keeps_pairing_epoch_and_reconnects_same_device() {
    let f = Fixture::new();
    let key = ed25519_dalek::SigningKey::generate(&mut OsRng);
    let (id, token) = wall_http::pair_with_key(f.port, "Disconnect Otter", &key);
    let receipt = approved_otter_receipt(&f, &id, &token);
    assert_eq!(
        wall_http::device(
            f.port,
            &token,
            "POST",
            "/v1/otter/connections/activate",
            Some(receipt.clone())
        )
        .0,
        200
    );
    let before = f.devices.lock().unwrap().devices[&id].session_epoch;
    let path = "/v1/otter/connections/disconnect";
    assert_eq!(
        wall_http::public(f.port, "POST", path, Some(json!({}))).0,
        401
    );
    assert_eq!(
        wall_http::device(f.port, &token, "POST", path, Some(json!({}))).0,
        200
    );
    assert_eq!(
        wall_http::device(f.port, &token, "GET", "/v1/otter/connection-info", None).0,
        401
    );
    {
        let store = f.devices.lock().unwrap();
        let device = &store.devices[&id];
        assert_eq!(device.session_epoch, before);
        assert_eq!(device.approval, "approved");
        assert!(device.enabled);
        assert!(!store.projections.presence.contains_key(&id));
        assert_eq!(
            store.connection_requests[receipt["requestId"].as_str().unwrap()].phase,
            "cancelled"
        );
    }
    let (status, session) = wall_http::issue_session(f.port, &id, &key);
    assert_eq!(status, 201, "{session}");
    let next = session["token"].as_str().unwrap();
    let (_, response) = wall_http::device(
        f.port,
        next,
        "POST",
        "/v1/otter/connections/request",
        Some(
            json!({"name":"Disconnect Otter", "publicKey":BASE64.encode(key.verifying_key().to_bytes())}),
        ),
    );
    assert_eq!(response["connection"]["deviceId"], id);
    assert_eq!(response["connection"]["phase"], "approved");
    assert_eq!(
        wall_http::device(f.port, next, "GET", "/v1/otter/connection-info", None).0,
        200
    );
}

#[test]
fn otter_connection_device_approval_resolves_pending_request_once() {
    let f = Fixture::new();
    let (_, created) = wall_http::public(
        f.port,
        "POST",
        "/v1/otter/connections/request",
        Some(json!({"name":"Unified approval", "publicKey":pairing_payload()["publicKey"]})),
    );
    let id = created["connection"]["deviceId"].as_str().unwrap();
    let path = format!("/v1/devices/{id}/approve");
    assert_eq!(
        wall_http::admin(f.port, "POST", &path, Some(json!({}))).0,
        200
    );
    let epoch = f.devices.lock().unwrap().devices[id].session_epoch;
    assert_eq!(
        wall_http::admin(f.port, "POST", &path, Some(json!({}))).0,
        200
    );
    assert_eq!(f.devices.lock().unwrap().devices[id].session_epoch, epoch);
    let (_, status) = wall_http::public(
        f.port,
        "POST",
        "/v1/otter/connections/status",
        Some(json!({"requestId":created["connection"]["id"],"receipt":created["receipt"]})),
    );
    assert_eq!(status["phase"], "approved");
}
