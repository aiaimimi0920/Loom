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

#[test]
fn naccount_http_routes_keep_platform_separate() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let root = ProjectionRoot::new();
    let (port, mut server) = start(&root.0);
    let (status, view) = admin(port, "/v1/naccount/status", json!({}));
    assert_eq!(status, 200);
    assert_eq!(view["status"], "signed_out");
    assert_eq!(public(port, "/v1/naccount/status", json!({})).0, 401);
    let wrong_port = admin(port, "/v1/naccount/start", json!({}));
    assert_eq!(wrong_port.0, 409);
    assert_eq!(error_code(&wrong_port), "naccount_callback_port_required");
    assert_eq!(
        admin(port, "/v1/naccount/status", json!({})).1["status"],
        "signed_out"
    );
    assert_eq!(admin(port, "/v1/asset-library/resources", json!({})).0, 401);
    let callback = response(http_request_with_extra_headers(
        port,
        "GET",
        "/v1/naccount/callback?state=unknown&code=unknown",
        None,
        "",
    ));
    assert_eq!(callback.0, 400);
    let navigation =
        "Sec-Fetch-Site: cross-site\r\nSec-Fetch-Mode: navigate\r\nSec-Fetch-Dest: document\r\n";
    let callback_path = "/v1/naccount/callback?state=unknown&code=unknown";
    let redirected = response(http_request_with_extra_headers(
        port,
        "GET",
        callback_path,
        None,
        navigation,
    ));
    assert_eq!(redirected.0, 400);
    assert_eq!(error_code(&redirected), error_code(&callback));
    for (path, headers) in [
        ("/v1/naccount/status", navigation.to_owned()),
        ("/v1/naccount/callback/", navigation.to_owned()),
        (callback_path, navigation.replace("navigate", "cors")),
        (callback_path, navigation.replace("document", "iframe")),
        (callback_path, "Sec-Fetch-Site: cross-site\r\n".to_owned()),
        (
            callback_path,
            format!("{navigation}Sec-Fetch-Mode: navigate\r\n"),
        ),
        (
            callback_path,
            format!("{navigation}Sec-Fetch-Site: cross-site\r\n"),
        ),
    ] {
        let denied = response(http_request_with_extra_headers(
            port, "GET", path, None, &headers,
        ));
        assert_eq!(denied.0, 403);
        assert_eq!(error_code(&denied), "browser_context_denied");
    }
    let denied_post = response(http_request_with_extra_headers(
        port,
        "POST",
        callback_path,
        Some("{}"),
        navigation,
    ));
    assert_eq!(denied_post.0, 403);
    let denied_origin = response(http_request_with_extra_headers(
        port,
        "GET",
        callback_path,
        None,
        &format!("{navigation}Origin: https://untrusted.example\r\n"),
    ));
    assert_eq!(denied_origin.0, 403);
    assert_eq!(error_code(&denied_origin), "origin_denied");
    assert_eq!(
        admin(port, "/v1/account/status", json!({})).1["status"],
        "signed_out"
    );
    server.finish().unwrap();
}
