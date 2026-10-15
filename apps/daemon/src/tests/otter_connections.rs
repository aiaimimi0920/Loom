#[test]
fn otter_connection_manual_approval_requires_private_receipt() {
    let f = Fixture::new();
    let payload = pairing_payload();
    let (status, response) = wall_http::public(
        f.port,
        "POST",
        "/v1/otter/connections/request",
        Some(json!({"name":"Otter test", "publicKey":payload["publicKey"]})),
    );
    assert_eq!(status, 201, "{response}");
    assert_eq!(response["connection"]["phase"], "pending");
    let id = response["connection"]["id"].clone();
    let receipt = json!({"requestId":id, "receipt":response["receipt"]});
    assert_eq!(
        wall_http::public(
            f.port,
            "POST",
            "/v1/otter/connections/status",
            Some(json!({"requestId":id,"receipt":"0".repeat(64)}))
        )
        .0,
        404
    );
    assert_eq!(
        wall_http::public(
            f.port,
            "POST",
            "/v1/otter/connections/decision",
            Some(json!({"requestId":id,"approve":true}))
        )
        .0,
        401
    );
    let (_, listing) = wall_http::admin(f.port, "GET", "/v1/otter/connections", None);
    assert!(!listing
        .to_string()
        .contains(response["receipt"].as_str().unwrap()));
    assert_eq!(listing["requests"].as_array().unwrap().len(), 1);
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
    let (_, current) = wall_http::public(
        f.port,
        "POST",
        "/v1/otter/connections/status",
        Some(receipt.clone()),
    );
    assert_eq!(current["phase"], "approved");
    let (_, cancelled) = wall_http::public(
        f.port,
        "POST",
        "/v1/otter/connections/cancel",
        Some(receipt),
    );
    assert_eq!(cancelled["phase"], "cancelled");
}

#[test]
fn otter_connection_password_policy_does_not_approve_wrong_password_or_disabled_device() {
    let f = Fixture::new();
    let policy = json!({"name":"Test Loom","discoverable":false,"endpoint":null,
        "approval":"lan","password":"test-password-0123"});
    assert_eq!(
        wall_http::admin(f.port, "PUT", "/v1/otter/connections/policy", Some(policy)).0,
        200
    );
    let payload = pairing_payload();
    let mut input =
        json!({"name":"Test Otter","publicKey":payload["publicKey"],"password":"wrong"});
    assert_eq!(
        wall_http::public(
            f.port,
            "POST",
            "/v1/otter/connections/request",
            Some(input.clone())
        )
        .0,
        403
    );
    input["password"] = json!("test-password-0123");
    let (status, response) = wall_http::public(
        f.port,
        "POST",
        "/v1/otter/connections/request",
        Some(input.clone()),
    );
    assert_eq!(status, 201, "{response}");
    assert_eq!(response["connection"]["phase"], "pending");
    let device_id = response["connection"]["deviceId"].as_str().unwrap();
    f.devices
        .lock()
        .unwrap()
        .devices
        .get_mut(device_id)
        .unwrap()
        .enabled = false;
    assert_eq!(
        wall_http::public(f.port, "POST", "/v1/otter/connections/request", Some(input)).0,
        403
    );
}

#[test]
fn otter_connection_known_public_key_is_not_authenticated_and_policy_change_revokes_requests() {
    let f = Fixture::new();
    let payload = pairing_payload();
    assert_eq!(
        wall_http::admin(f.port, "POST", "/v1/devices", Some(payload.clone())).0,
        200
    );
    let policy =
        json!({"name":"Test Loom","discoverable":false,"endpoint":null,"approval":"authenticated"});
    assert_eq!(
        wall_http::admin(
            f.port,
            "PUT",
            "/v1/otter/connections/policy",
            Some(policy.clone())
        )
        .0,
        200
    );
    let (_, response) = wall_http::public(
        f.port,
        "POST",
        "/v1/otter/connections/request",
        Some(json!({"name":"Test Otter","publicKey":payload["publicKey"]})),
    );
    assert_eq!(response["connection"]["phase"], "pending");
    assert_eq!(
        wall_http::admin(f.port, "PUT", "/v1/otter/connections/policy", Some(policy)).0,
        200
    );
    let receipt = json!({"requestId":response["connection"]["id"],"receipt":response["receipt"]});
    assert_eq!(
        wall_http::public(
            f.port,
            "POST",
            "/v1/otter/connections/status",
            Some(receipt)
        )
        .0,
        404
    );
}

#[test]
fn otter_connection_authenticated_policy_checks_signed_session_and_exact_public_key() {
    let f = Fixture::new();
    let (id, token) = wall_http::pair(f.port, "Authenticated Otter");
    let key = f.devices.lock().unwrap().devices[&id]
        .public_key
        .clone()
        .unwrap();
    let policy =
        json!({"name":"Test Loom","discoverable":false,"endpoint":null,"approval":"authenticated"});
    assert_eq!(
        wall_http::device(
            f.port,
            &token,
            "PUT",
            "/v1/otter/connections/policy",
            Some(policy.clone())
        )
        .0,
        403
    );
    assert_eq!(
        wall_http::admin(f.port, "PUT", "/v1/otter/connections/policy", Some(policy)).0,
        200
    );
    let (_, response) = wall_http::device(
        f.port,
        &token,
        "POST",
        "/v1/otter/connections/request",
        Some(json!({"name":"Authenticated Otter","publicKey":key})),
    );
    assert_eq!(response["connection"]["phase"], "pending");
    assert_eq!(
        wall_http::admin(
            f.port,
            "POST",
            "/v1/otter/connections/decision",
            Some(json!({"requestId":response["connection"]["id"],"approve":true}))
        )
        .0,
        200
    );
    let receipt = json!({"requestId":response["connection"]["id"],"receipt":response["receipt"]});
    assert_eq!(
        wall_http::device(
            f.port,
            &token,
            "POST",
            "/v1/otter/connections/activate",
            Some(receipt)
        )
        .0,
        200
    );
    assert!(f.devices.lock().unwrap().devices[&id].otter_connected);
    let (_, connected) = wall_http::device(
        f.port,
        &token,
        "POST",
        "/v1/otter/connections/request",
        Some(json!({"name":"Known Otter","publicKey":key})),
    );
    assert_eq!(connected["connection"]["phase"], "approved");
    let other = pairing_payload();
    let (_, response) = wall_http::device(
        f.port,
        &token,
        "POST",
        "/v1/otter/connections/request",
        Some(json!({"name":"Other device","publicKey":other["publicKey"]})),
    );
    assert_eq!(response["connection"]["phase"], "pending");
    let manual =
        json!({"name":"Test Loom","discoverable":false,"endpoint":null,"approval":"manual"});
    assert_eq!(
        wall_http::admin(f.port, "PUT", "/v1/otter/connections/policy", Some(manual)).0,
        200
    );
    let (_, response) = wall_http::device(
        f.port,
        &token,
        "POST",
        "/v1/otter/connections/request",
        Some(json!({"name":"Authenticated Otter","publicKey":key})),
    );
    assert_eq!(response["connection"]["phase"], "pending");
}

#[test]
fn otter_connection_expired_requests_cannot_be_approved_or_polled() {
    let f = Fixture::new();
    let payload = pairing_payload();
    let (_, response) = wall_http::public(
        f.port,
        "POST",
        "/v1/otter/connections/request",
        Some(json!({"name":"Test Otter","publicKey":payload["publicKey"]})),
    );
    let id = response["connection"]["id"].as_str().unwrap();
    f.devices
        .lock()
        .unwrap()
        .connection_requests
        .get_mut(id)
        .unwrap()
        .expires_at_ms = 0;
    assert_eq!(
        wall_http::admin(
            f.port,
            "POST",
            "/v1/otter/connections/decision",
            Some(json!({"requestId":id,"approve":true}))
        )
        .0,
        409
    );
    assert_eq!(
        wall_http::public(
            f.port,
            "POST",
            "/v1/otter/connections/status",
            Some(json!({"requestId":id,"receipt":response["receipt"]}))
        )
        .0,
        404
    );
}
