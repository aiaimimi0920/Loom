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
