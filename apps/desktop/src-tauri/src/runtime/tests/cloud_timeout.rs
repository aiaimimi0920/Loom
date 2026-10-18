use super::*;

#[test]
fn cloud_actions_have_bounded_network_budget_without_extending_local_routes() {
    for path in [
        "/v1/naccount/refresh",
        "/v1/naccount/logout",
        "/v1/asset-library/library",
        "/v1/asset-library/resources",
    ] {
        assert_eq!(daemon_action_timeout(path), Duration::from_secs(90));
    }
    assert_eq!(
        daemon_action_timeout("/v1/asset-library/download"),
        Duration::from_secs(120)
    );
    assert_eq!(
        daemon_action_timeout("/v1/asset-library/install"),
        Duration::from_secs(120)
    );
    for path in [
        "/v1/runs",
        "/v1/asset-library/download/other",
        "/v1/asset-library/install/other",
        "/v1/naccount/unknown",
    ] {
        assert_eq!(
            daemon_action_timeout(path),
            LOOM_DAEMON_DEFAULT_REQUEST_TIMEOUT
        );
    }
}

#[test]
fn refresh_response_after_local_budget_succeeds_only_with_cloud_budget() {
    // 模拟真实刷新超过本机三秒预算；不连接账号服务或使用真实刷新令牌。
    for old_budget in [true, false] {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind refresh fixture");
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept refresh fixture");
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let request = read_test_http_request(&mut stream);
            assert!(request.starts_with("POST /v1/naccount/refresh HTTP/1.1\r\n"));
            thread::sleep(LOOM_DAEMON_DEFAULT_REQUEST_TIMEOUT + Duration::from_secs(1));
            let body = r#"{"status":"signed_in","principal":"fixture","name":"fixture"}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            // 旧预算会先断开，写失败是预期结果；线程始终收尾。
            let _ = stream.write_all(response.as_bytes());
        });
        let url = format!("http://{address}");
        let body = serde_json::json!({});
        let result = if old_budget {
            http_request_json_with_timeout(
                &url,
                "POST",
                "/v1/naccount/refresh",
                Some(&body),
                LOOM_DAEMON_DEFAULT_REQUEST_TIMEOUT,
            )
        } else {
            http_post_json(&url, "/v1/naccount/refresh", &body)
        };
        server.join().expect("join refresh fixture");
        if old_budget {
            assert!(result.is_err(), "旧三秒预算必须复现失败");
        } else {
            assert_eq!(result.expect("云操作应等待实际响应")["status"], "signed_in");
        }
    }
}
