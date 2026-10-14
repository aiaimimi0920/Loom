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
    assert_eq!(
        wall_http::admin(
            f.port,
            "POST",
            "/v1/otter/connections/decision",
            Some(json!({"requestId":id,"approve":true}))
        )
        .0,
        200
    );
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
