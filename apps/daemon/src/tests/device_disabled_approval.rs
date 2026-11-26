// Real HTTP contract: pending approval is retryable, whereas admin disable cannot be cleared by re-registration or session issuance.
#[test]
fn disabled_approval_distinguishes_pending_and_preserves_disable_on_registration() {
    let f = Fixture::new();
    let key = ed25519_dalek::SigningKey::generate(&mut OsRng);
    let registration = json!({
        "name":"Pending disabled Otter", "kind":"computer", "address":"hook://test",
        "publicKey":BASE64.encode(key.verifying_key().to_bytes())
    });
    let (status, registry) = wall_http::public(
        f.port,
        "POST",
        "/v1/devices/requests",
        Some(registration.clone()),
    );
    assert_eq!(status, 200);
    let pending = &registry["pending"][0];
    assert_eq!(pending["enabled"], true);
    let id = pending["id"].as_str().unwrap();
    let (status, denial) = wall_http::issue_session(f.port, id, &key);
    assert_eq!(status, 403);
    assert_eq!(denial["error"]["code"], "device_not_authorized");
    assert!(denial.get("challengeId").is_none());

    let (status, _) = wall_http::admin(
        f.port,
        "PUT",
        &format!("/v1/devices/{id}"),
        Some(json!({
            "name":"Pending disabled Otter", "kind":"computer", "address":"hook://test", "enabled":false
        })),
    );
    assert_eq!(status, 200);
    for approved in [false, true] {
        if approved {
            assert_eq!(
                wall_http::admin(
                    f.port,
                    "POST",
                    &format!("/v1/devices/{id}/approve"),
                    Some(json!({}))
                )
                .0,
                200
            );
            // The approve endpoint may re-enable the device; explicitly disable again to cover `approved + disabled`.
            assert_eq!(wall_http::admin(f.port, "PUT", &format!("/v1/devices/{id}"), Some(json!({
                "name":"Pending disabled Otter", "kind":"computer", "address":"hook://test", "enabled":false
            }))).0, 200);
        }
        let (status, _) = wall_http::public(
            f.port,
            "POST",
            "/v1/devices/requests",
            Some(registration.clone()),
        );
        assert_eq!(status, 200);
        let (status, denial) = wall_http::issue_session(f.port, id, &key);
        assert_eq!(status, 403);
        assert_eq!(denial["error"]["code"], "device_disabled");
        assert!(denial.get("token").is_none());
        assert!(denial.get("challengeId").is_none());
        let store = f.devices.lock().unwrap();
        assert!(!store.devices[id].enabled);
        assert_eq!(
            store
                .devices
                .values()
                .filter(|device| !device.is_local)
                .count(),
            1
        );
    }
}
