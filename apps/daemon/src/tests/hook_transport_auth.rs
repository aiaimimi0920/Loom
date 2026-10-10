use hook_transport_auth::authenticated_hook_test_request;
mod hook_transport_auth {
    use super::*;
    use tungstenite::client::IntoClientRequest;

    #[test]
    fn hook_transport_broker_requires_admin_and_rotates_listener_credentials() {
        let _guard = lock_ignoring_poison(&ENV_LOCK);
        let root = unique_temp_dir("hook-transport-broker");
        let runtime = test_daemon_runtime(&root, None);
        let path = "/v1/hook-bridge/credentials";
        let request = parsed_request("POST", path, &[], Some("{}"));
        expect_json_text_route_response(route_request(&runtime, &request), 503);
        assert!(!is_public_device_auth_route("POST", path));
        assert!(!device_session_route_allowed("POST", path));
        let mut anonymous = parsed_request("POST", path, &[], Some("{}"));
        anonymous
            .headers
            .retain(|(name, _)| !name.eq_ignore_ascii_case("authorization"));
        expect_json_text_route_response(route_request(&runtime, &anonymous), 401);

        let started = start_test_hook_bridge(&runtime, r#"{"port":0}"#);
        let port = started["port"].as_u64().unwrap() as u16;
        let credential = expect_json_text_route_response(route_request(&runtime, &request), 200);
        assert_eq!(credential["url"], format!("ws://127.0.0.1:{port}"));
        let token = credential["token"].as_str().unwrap();
        assert!(token.starts_with("hook-v1."));
        assert_ne!(token, TEST_DAEMON_AUTH_TOKEN);
        let scoped_http = parsed_request(
            "POST",
            path,
            &[("Authorization", &format!("Bearer {token}"))],
            Some("{}"),
        );
        expect_json_text_route_response(route_request(&runtime, &scoped_http), 401);
        assert!(tungstenite::connect(authenticated_hook_test_request(
            port,
            TEST_DAEMON_AUTH_TOKEN
        ))
        .is_err());
        let (mut socket, _) =
            tungstenite::connect(authenticated_hook_test_request(port, token)).unwrap();
        let _ = socket.close(None);
        drop(socket);
        stop_test_hook_bridge(&runtime);
        expect_json_text_route_response(route_request(&runtime, &request), 503);

        let restarted = start_test_hook_bridge(&runtime, r#"{"port":0}"#);
        let new_port = restarted["port"].as_u64().unwrap() as u16;
        let next = expect_json_text_route_response(route_request(&runtime, &request), 200);
        assert_ne!(next["token"], token);
        assert!(tungstenite::connect(authenticated_hook_test_request(new_port, token)).is_err());
        stop_test_hook_bridge(&runtime);
        drop(runtime);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn hook_transport_broker_never_issues_credentials_for_an_occupied_port() {
        let _guard = lock_ignoring_poison(&ENV_LOCK);
        let root = unique_temp_dir("hook-transport-impostor");
        let runtime = test_daemon_runtime(&root, None);
        let impostor = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = impostor.local_addr().unwrap().port();
        let request = parsed_request(
            "POST",
            "/v1/hook-bridge/start",
            &[],
            Some(&format!(r#"{{"port":{port}}}"#)),
        );
        expect_json_text_route_response(route_request(&runtime, &request), 409);
        expect_json_result_response(hook_bridge_credentials(&runtime.hook_bridge), 503);
        assert!(runtime.hook_bridge.lock().unwrap().auth_token.is_none());
        drop(runtime);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn hook_transport_supports_utf8_native_tokens_and_both_desktop_dev_origins() {
        let mut request = authenticated_hook_test_request(19820, "local-凭据");
        assert!(authorize_hook_upgrade(&request, "local-凭据").is_ok());
        for origin in [
            "http://localhost:1420",
            "http://localhost:1423",
            "http://127.0.0.1:1423",
        ] {
            request
                .headers_mut()
                .insert("origin", origin.parse().unwrap());
            assert!(authorize_hook_upgrade(&request, "local-凭据").is_ok());
        }
    }

    #[test]
    fn hook_transport_rejects_ambiguous_headers_and_url_credentials() {
        for header in ["host", "origin", "authorization", "sec-websocket-protocol"] {
            let mut request = authenticated_hook_test_request(19820, "secret");
            if header == "origin" {
                request
                    .headers_mut()
                    .insert(header, "http://tauri.localhost".parse().unwrap());
            }
            if header == "sec-websocket-protocol" {
                request.headers_mut().remove("authorization");
                request
                    .headers_mut()
                    .insert(header, "loom.hook.v1, loom.auth.c2VjcmV0".parse().unwrap());
            }
            let duplicate = request.headers()[header].clone();
            request.headers_mut().append(header, duplicate);
            assert!(
                authorize_hook_upgrade(&request, "secret").is_err(),
                "{header}"
            );
        }
        for offered in [
            "loom.hook.v1",
            "loom.auth.c2VjcmV0",
            "loom.hook.v1, loom.auth.@@",
            "loom.hook.v1, loom.auth.c2VjcmV0, extra",
            "loom.hook.v1, loom.auth.d3Jvbmc",
        ] {
            let mut request = authenticated_hook_test_request(19820, "secret");
            request.headers_mut().remove("authorization");
            request
                .headers_mut()
                .insert("sec-websocket-protocol", offered.parse().unwrap());
            assert!(authorize_hook_upgrade(&request, "secret").is_err());
        }
        let mut request = authenticated_hook_test_request(19820, "secret");
        *request.uri_mut() = "ws://127.0.0.1:19820/?token=secret".parse().unwrap();
        assert!(authorize_hook_upgrade(&request, "secret").is_err());
        let mut request = authenticated_hook_test_request(19820, "secret");
        request
            .headers_mut()
            .insert("host", "attacker.example:19820".parse().unwrap());
        assert!(authorize_hook_upgrade(&request, "secret").is_err());
        let request = authenticated_hook_test_request(19820, "secret");
        assert!(authorize_hook_upgrade(&request, "").is_err());
    }

    pub(super) fn authenticated_hook_test_request(
        port: u16,
        token: &str,
    ) -> tungstenite::handshake::client::Request {
        let mut request = format!("ws://127.0.0.1:{port}")
            .into_client_request()
            .unwrap();
        request
            .headers_mut()
            .insert("authorization", format!("Bearer {token}").parse().unwrap());
        request
    }

    #[test]
    fn hook_transport_denies_untrusted_upgrade_before_protocol_dispatch() {
        for (credential, origin, expected) in [
            (None, None, 401),
            (Some("wrong"), None, 401),
            (Some("secret"), Some("https://evil.example"), 403),
            (Some("secret"), Some("null"), 403),
            (
                Some("secret"),
                Some("http://localhost:1420.evil.example"),
                403,
            ),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = thread::spawn(move || {
                let (stream, _) = listener.accept().unwrap();
                assert!(accept_authenticated_hook_socket(stream, "secret").is_err());
            });
            let mut request = format!("ws://127.0.0.1:{port}")
                .into_client_request()
                .unwrap();
            if let Some(token) = credential {
                request
                    .headers_mut()
                    .insert("authorization", format!("Bearer {token}").parse().unwrap());
            }
            if let Some(origin) = origin {
                request
                    .headers_mut()
                    .insert("origin", origin.parse().unwrap());
            }
            match tungstenite::connect(request) {
                Err(tungstenite::Error::Http(response)) => {
                    assert_eq!(response.status().as_u16(), expected)
                }
                _ => panic!("unauthenticated upgrade unexpectedly accepted"),
            }
            server.join().unwrap();
        }
    }

    #[test]
    fn hook_transport_accepts_native_and_browser_credentials_without_echoing_secret() {
        for browser in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = thread::spawn(move || {
                let (stream, _) = listener.accept().unwrap();
                assert!(accept_authenticated_hook_socket(stream, "secret").is_ok());
            });
            let mut request = authenticated_hook_test_request(port, "secret");
            if browser {
                request.headers_mut().remove("authorization");
                request
                    .headers_mut()
                    .insert("origin", "http://tauri.localhost".parse().unwrap());
                request.headers_mut().insert(
                    "sec-websocket-protocol",
                    format!("loom.hook.v1, loom.auth.{}", BASE64_URL.encode("secret"))
                        .parse()
                        .unwrap(),
                );
            }
            let (_, response) = tungstenite::connect(request).unwrap();
            assert_eq!(
                response
                    .headers()
                    .get("sec-websocket-protocol")
                    .map(|v| v.to_str().unwrap()),
                browser.then_some("loom.hook.v1")
            );
            assert!(!format!("{:?}", response.headers()).contains("secret"));
            server.join().unwrap();
        }
    }
}
