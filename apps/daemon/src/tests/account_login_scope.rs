#[test]
fn account_login_http_denies_projection_devices_and_unauthenticated_callers() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let root = ProjectionRoot::new();
    let (port, mut server) = start(&root.0);
    let hook = Identity::pair(port, "Account scope fixture");
    for action in ["start", "status", "poll", "refresh", "logout"] {
        let path = format!("/v1/account/{action}");
        assert_eq!(public(port, &path, json!({})).0, 401);
        let headers = format!(
            "Authorization: Device {}\r\nX-Loom-Device-Nonce: {}\r\n",
            hook.token,
            Uuid::new_v4()
        );
        let denied = response(http_request_with_extra_headers(
            port,
            "POST",
            &path,
            Some("{}"),
            &headers,
        ));
        assert_eq!(denied.0, 403);
        assert_eq!(error_code(&denied), "device_session_scope_denied");
    }
    let (status, view) = admin(port, "/v1/account/status", json!({}));
    assert_eq!(status, 200);
    assert_eq!(view["status"], "signed_out");
    assert_eq!(admin(port, "/v1/account/logout", json!({})).0, 200);
    assert_eq!(
        admin(
            port,
            "/v1/account/start",
            json!({"origin":"https://platform.example", "deviceName":""})
        )
        .0,
        400
    );
    server.finish().unwrap();
}
