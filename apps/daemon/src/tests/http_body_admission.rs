#[test]
fn large_unauthenticated_body_is_rejected_before_upload() {
    let fixture = Fixture::new();
    let mut stream = TcpStream::connect(("127.0.0.1", fixture.port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    write!(stream, "POST /v1/mcp/servers/install HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Type: application/json\r\nContent-Length: 100663296\r\n\r\n", fixture.port).unwrap();
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("reject from headers without waiting for upload");
    assert!(response.starts_with("HTTP/1.1 401"), "{response}");
}

fn header_only_status(fixture: &Fixture, host: &str, extras: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", fixture.port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    write!(stream, "POST /v1/mcp/servers/install HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nContent-Length: 100663296\r\n{extras}\r\n").unwrap();
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("header-only rejection");
    response
}

#[test]
fn large_body_host_origin_and_device_scope_are_checked_before_upload() {
    let fixture = Fixture::new();
    let token = test_bound_daemon_token(fixture.port).unwrap();
    let bearer = format!("Authorization: Bearer {token}\r\n");
    assert!(header_only_status(&fixture, "attacker.example", &bearer).starts_with("HTTP/1.1 400"));
    assert!(header_only_status(
        &fixture,
        "127.0.0.1",
        &format!("{bearer}Origin: https://attacker.example\r\n")
    )
    .starts_with("HTTP/1.1 403"));
    let (_, device_token) = wall_http::pair(fixture.port, "Upload scope");
    assert!(header_only_status(
        &fixture,
        "127.0.0.1",
        &format!(
            "Authorization: Device {device_token}\r\nX-Loom-Device-Nonce: {}\r\n",
            Uuid::new_v4()
        )
    )
    .starts_with("HTTP/1.1 403"));
}

#[test]
fn body_admission_preserves_admin_upload_and_consumes_device_nonce_only_at_dispatch() {
    let fixture = Fixture::new();
    let response = http_request(fixture.port, "POST", "/v1/mcp/servers/install", Some("{}"));
    assert!(!response.starts_with("HTTP/1.1 401"), "{response}");
    let (device, token) = wall_http::pair(fixture.port, "Projection upload");
    let nonce = Uuid::new_v4().to_string();
    let hash = sha256_bytes(token.as_bytes());
    {
        let registry = fixture.devices.lock().unwrap();
        assert_eq!(
            registry.validate_device_session(&token, &nonce).unwrap(),
            device
        );
        assert_eq!(
            registry.validate_device_session(&token, &nonce).unwrap(),
            device
        );
        assert!(!registry.sessions[&hash].used_nonces.contains(&nonce));
    }
    let response = http_request_with_extra_headers(
        fixture.port,
        "POST",
        "/v1/projections/create",
        Some("{}"),
        &format!("Authorization: Device {token}\r\nX-Loom-Device-Nonce: {nonce}\r\n"),
    );
    assert!(
        !response.starts_with("HTTP/1.1 401") && !response.starts_with("HTTP/1.1 409"),
        "{response}"
    );
    assert!(fixture.devices.lock().unwrap().sessions[&hash]
        .used_nonces
        .contains(&nonce));
    assert_eq!(
        fixture
            .devices
            .lock()
            .unwrap()
            .validate_device_session(&token, &nonce)
            .unwrap_err()
            .code,
        "device_request_replayed"
    );
    fixture
        .devices
        .lock()
        .unwrap()
        .revoke_device_sessions(&device);
    assert!(fixture
        .devices
        .lock()
        .unwrap()
        .validate_device_session(&token, &Uuid::new_v4().to_string())
        .is_err());
}

#[test]
fn declared_body_length_does_not_determine_reader_allocation() {
    // A complete small payload retains only the bytes received, not its route's 96 MiB allowance.
    let request = b"POST /v1/mcp/servers/install HTTP/1.1\r\nContent-Length: 2\r\n\r\n{}";
    let HttpReadOutcome::Request(bytes) = read_http_request(&mut &request[..]).unwrap() else {
        panic!("bounded request should be accepted");
    };
    assert!(bytes.capacity() <= HTTP_READ_CHUNK_BYTES);
}

#[test]
fn full_size_upload_capacity_is_capped_at_declared_length() {
    let header = format!("POST /v1/mcp/servers/install HTTP/1.1\r\nContent-Length: {MAX_MCP_SERVER_PACKAGE_HTTP_BODY_BYTES}\r\n\r\n");
    let expected = header.len() + MAX_MCP_SERVER_PACKAGE_HTTP_BODY_BYTES;
    let mut source = std::io::Cursor::new(header.into_bytes())
        .chain(std::io::repeat(b'x').take(MAX_MCP_SERVER_PACKAGE_HTTP_BODY_BYTES as u64));
    let HttpReadOutcome::Request(bytes) = read_http_request(&mut source).unwrap() else {
        panic!("full-size bounded upload should be accepted");
    };
    assert_eq!(bytes.len(), expected);
    assert!(
        bytes.capacity() <= expected,
        "capacity={} expected={expected}",
        bytes.capacity()
    );
}

#[test]
fn exhausted_device_nonce_budget_rejects_header_only_upload() {
    let fixture = Fixture::new();
    let (_, token) = wall_http::pair(fixture.port, "Exhausted upload");
    let hash = sha256_bytes(token.as_bytes());
    fixture
        .devices
        .lock()
        .unwrap()
        .sessions
        .get_mut(&hash)
        .unwrap()
        .used_nonces = (0..DEVICE_SESSION_MAX_NONCES)
        .map(|n| format!("used-nonce-{n:016}"))
        .collect();
    let mut stream = TcpStream::connect(("127.0.0.1", fixture.port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    write!(stream, "POST /v1/projections/create HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: 4194304\r\nAuthorization: Device {token}\r\nX-Loom-Device-Nonce: {}\r\n\r\n", Uuid::new_v4()).unwrap();
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("exhausted session must reject from headers");
    assert!(response.starts_with("HTTP/1.1 401"), "{response}");
    assert!(
        response.contains("device_session_nonce_capacity"),
        "{response}"
    );
    assert_eq!(
        fixture.devices.lock().unwrap().sessions[&hash]
            .used_nonces
            .len(),
        DEVICE_SESSION_MAX_NONCES
    );
}

#[test]
fn eager_unauthenticated_upload_receives_complete_admission_error() {
    let fixture = Fixture::new();
    let mut stream = TcpStream::connect(("127.0.0.1", fixture.port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let payload = vec![b' '; 4 * 1024 * 1024];
    write!(stream, "POST /v1/projections/create HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n", payload.len()).unwrap();
    stream
        .write_all(&payload)
        .expect("eager upload should be drained, not reset");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("complete admission response, not TCP reset");
    assert!(response.starts_with("HTTP/1.1 401"), "{response}");
    assert!(response.contains("unauthorized"), "{response}");
    let (_, body) = response
        .split_once("\r\n\r\n")
        .expect("complete response headers");
    let body: Value = serde_json::from_str(body).expect("complete rejection JSON");
    assert_eq!(body["error"]["code"], "unauthorized");
}

#[test]
fn admin_only_device_routes_reject_before_upload() {
    let fixture = Fixture::new();
    let token = test_bound_daemon_token(fixture.port).unwrap();
    for (path, code) in [
        (
            "/v1/offline-projections/create",
            "projection_device_required",
        ),
        ("/v1/projections/create", "projection_pairing_required"),
    ] {
        let mut stream = TcpStream::connect(("127.0.0.1", fixture.port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        write!(stream, "POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: 4194304\r\nAuthorization: Bearer {token}\r\n\r\n").unwrap();
        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .expect("admin alone cannot admit device-only upload");
        assert!(
            response.starts_with("HTTP/1.1 403") && response.contains(code),
            "{response}"
        );
    }
}
