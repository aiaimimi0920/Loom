fn verified_authenticated_otter_request(
    f: &Fixture,
    id: &str,
    token: &str,
) -> VerifiedOtterConnection {
    let store = f.devices.lock().unwrap();
    let key = store.devices[id].public_key.as_deref().unwrap();
    VerifiedOtterConnection {
        input: OtterConnectionInput {
            name: "Known Otter".into(),
            public_key: key.into(),
            password: String::new(),
        },
        authentication: Some(
            OtterConnectionAuthentication::capture(&store, Some(id), Some(token), key).unwrap(),
        ),
        policy: store.connection_policy.clone(),
        revision: store.connection_policy_revision,
        created_at: unix_time_millis(),
    }
}

#[test]
fn otter_connection_revocation_during_verification_cannot_reapprove_device() {
    for revoke in ["delete", "epoch", "session", "expiry", "sticky"] {
        let f = Fixture::new();
        let (id, token) = wall_http::pair(f.port, "Known Otter");
        let policy = json!({"name":"Test Loom","discoverable":false,"endpoint":null,
            "approval":"authenticated","password":"test-password-0123"});
        assert_eq!(
            wall_http::admin(f.port, "PUT", "/v1/otter/connections/policy", Some(policy)).0,
            200
        );
        // Deterministic boundary: capture the valid proof, then revoke before final admission.
        let verified = verified_authenticated_otter_request(&f, &id, &token);
        {
            let mut store = f.devices.lock().unwrap();
            let hash = sha256_bytes(token.as_bytes());
            match revoke {
                "delete" => {
                    store.devices.remove(&id);
                }
                "epoch" => {
                    store.devices.get_mut(&id).unwrap().session_epoch += 1;
                }
                "session" => {
                    store.sessions.remove(&hash);
                }
                "expiry" => {
                    store.sessions.get_mut(&hash).unwrap().expires_at_ms = 0;
                }
                "sticky" => {
                    store.sessions[&hash].revoked.store(true, Ordering::SeqCst);
                }
                _ => unreachable!(),
            }
        }
        let key = verified.input.public_key.clone();
        let (status, response) =
            finish_otter_connection_request(verified, &f.devices, &f.bridge).unwrap();
        assert_eq!(status, 403, "{revoke}: {response}");
        assert!(response.contains("connection_authentication_revoked"));
        let store = f.devices.lock().unwrap();
        assert!(store.connection_requests.is_empty());
        if revoke == "delete" {
            assert!(!store
                .devices
                .values()
                .any(|device| device.public_key.as_deref() == Some(key.as_str())));
        }
    }
}

#[test]
fn otter_connection_approved_receipt_does_not_survive_device_epoch_change() {
    let f = Fixture::new();
    let payload = pairing_payload();
    let (_, created) = wall_http::public(
        f.port,
        "POST",
        "/v1/otter/connections/request",
        Some(json!({"name":"Epoch Otter", "publicKey":payload["publicKey"]})),
    );
    let id = created["connection"]["id"].as_str().unwrap();
    let device_id = created["connection"]["deviceId"].as_str().unwrap();
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
    let receipt = json!({"requestId":id,"receipt":created["receipt"]});
    let (_, before) = wall_http::public(
        f.port,
        "POST",
        "/v1/otter/connections/status",
        Some(receipt.clone()),
    );
    assert_eq!(before["phase"], "approved");
    f.devices
        .lock()
        .unwrap()
        .devices
        .get_mut(device_id)
        .unwrap()
        .session_epoch += 1;
    let (_, after) = wall_http::public(
        f.port,
        "POST",
        "/v1/otter/connections/status",
        Some(receipt),
    );
    assert_eq!(after["phase"], "rejected");
}
