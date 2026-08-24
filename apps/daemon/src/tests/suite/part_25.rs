// Loom daemon tests fragment 25; included into the shared crate test module.
#[test]
fn daemon_requires_tls_and_bearer_auth_for_non_loopback_routes() {
    let plaintext_error = LoomDaemon::bind(DaemonConfig::bind_host("0.0.0.0", 0))
        .err()
        .expect("non-loopback daemon binds require an authenticated TLS terminator");
    assert!(plaintext_error
        .to_string()
        .contains("plaintext non-loopback"));

    let daemon = LoomDaemon::bind(
        DaemonConfig::bind_host("0.0.0.0", 0)
            .with_bearer_token("local-token")
            .with_tls_termination(true),
    )
    .expect("bind daemon with token");
    let address = daemon.local_addr().expect("local address");
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

    let unauthorized = http_request_without_auth(
        address.port(),
        "POST",
        "/v1/invoke",
        Some(
            r#"{"requestId":"loom-request-auth","caller":"hook","capability":"brain.plan","input":{"goal":"token protected"}}"#,
        ),
    );
    assert!(unauthorized.starts_with("HTTP/1.1 401 Unauthorized"));
    let unauthorized_body = response_json_body(&unauthorized);
    assert_eq!(unauthorized_body["error"]["code"], "unauthorized");

    let authorized = http_request_with_bearer(
        address.port(),
        "POST",
        "/v1/invoke",
        Some(
            r#"{"requestId":"loom-request-auth","caller":"hook","capability":"brain.plan","input":{"goal":"token protected"}}"#,
        ),
        "local-token",
    );
    assert!(authorized.starts_with("HTTP/1.1 200 OK"));
    let authorized_body = response_json_body(&authorized);
    assert_eq!(authorized_body["requestId"], "loom-request-auth");
    assert_eq!(authorized_body["status"], "succeeded");
    let run_id = authorized_body["output"]["runId"].as_str().expect("run id");

    let unauthorized_status = http_get_without_auth(address.port(), "/status");
    assert!(
        unauthorized_status.starts_with("HTTP/1.1 401 Unauthorized"),
        "unauthorized_status={unauthorized_status}"
    );
    let unauthorized_capabilities = http_get_without_auth(address.port(), "/v1/capabilities");
    assert!(
        unauthorized_capabilities.starts_with("HTTP/1.1 401 Unauthorized"),
        "unauthorized_capabilities={unauthorized_capabilities}"
    );
    let unauthorized_run = http_get_without_auth(address.port(), &format!("/v1/runs/{run_id}"));
    assert!(
        unauthorized_run.starts_with("HTTP/1.1 401 Unauthorized"),
        "unauthorized_run={unauthorized_run}"
    );
    let unauthorized_events =
        http_get_without_auth(address.port(), &format!("/v1/runs/{run_id}/events"));
    assert!(
        unauthorized_events.starts_with("HTTP/1.1 401 Unauthorized"),
        "unauthorized_events={unauthorized_events}"
    );

    let public_health = http_get(address.port(), "/health");
    assert!(
        public_health.starts_with("HTTP/1.1 200 OK"),
        "public_health={public_health}"
    );
    let authorized_events = http_request_with_bearer(
        address.port(),
        "GET",
        &format!("/v1/runs/{run_id}/events"),
        None,
        "local-token",
    );
    assert!(
        authorized_events.starts_with("HTTP/1.1 200 OK"),
        "authorized_events={authorized_events}"
    );

    shutdown_tx.send(()).expect("shutdown");
    server.join().expect("server thread");
}

#[test]
fn daemon_rejects_oversized_declared_request_body() {
    let daemon = LoomDaemon::bind(
        DaemonConfig::bind_host("0.0.0.0", 0)
            .with_bearer_token("local-token")
            .with_tls_termination(true),
    )
    .expect("bind daemon with token");
    let address = daemon.local_addr().expect("local address");
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

    let response = http_request_with_declared_content_length(
        address.port(),
        "POST",
        "/v1/invoke",
        2 * 1024 * 1024,
        Some("local-token"),
    );
    assert!(
        response.starts_with("HTTP/1.1 413 Payload Too Large"),
        "response={response}"
    );

    shutdown_tx.send(()).expect("shutdown");
    server.join().expect("server thread");
}

#[test]
fn package_install_routes_have_a_larger_but_bounded_body_limit() {
    let package_size = 9 * 1024 * 1024;
    for path in [
        "/v1/frameworks/install",
        "/v1/frameworks/third-party/upgrade",
        "/v1/arts/install",
        "/v1/mcp/servers/install",
    ] {
        let request = format!("POST {path} HTTP/1.1\r\nContent-Length: {package_size}\r\n\r\n");
        assert!(
            !request_exceeds_size_limit(request.as_bytes()),
            "package route rejected its bounded package body: {path}"
        );
    }

    let ordinary_request =
        format!("POST /v1/invoke HTTP/1.1\r\nContent-Length: {package_size}\r\n\r\n");
    assert!(request_exceeds_size_limit(ordinary_request.as_bytes()));

    let oversized_package = MAX_PACKAGE_HTTP_BODY_BYTES + 1;
    let oversized_request = format!(
        "POST /v1/frameworks/install HTTP/1.1\r\nContent-Length: {oversized_package}\r\n\r\n"
    );
    assert!(request_exceeds_size_limit(oversized_request.as_bytes()));

    let bundled_mcp_package = 48 * 1024 * 1024;
    let mcp_request = format!(
        "POST /v1/mcp/servers/install HTTP/1.1\r\nContent-Length: {bundled_mcp_package}\r\n\r\n"
    );
    assert!(!request_exceeds_size_limit(mcp_request.as_bytes()));

    let oversized_mcp_package = MAX_MCP_SERVER_PACKAGE_HTTP_BODY_BYTES + 1;
    let oversized_mcp_request = format!(
        "POST /v1/mcp/servers/install HTTP/1.1\r\nContent-Length: {oversized_mcp_package}\r\n\r\n"
    );
    assert!(request_exceeds_size_limit(oversized_mcp_request.as_bytes()));
}

fn assert_http_request_rejected(raw: &[u8], expected_status: u16) {
    let mut reader = std::io::Cursor::new(raw);
    let outcome = read_http_request(&mut reader).expect("read malformed request");
    let HttpReadOutcome::Rejected { status, body } = outcome else {
        panic!("expected malformed request to be rejected");
    };
    assert_eq!(status, expected_status, "body={body}");
}

#[test]
fn request_reader_rejects_ambiguous_or_invalid_http_framing() {
    for raw in [
        b"POST /v1/invoke HTTP/1.1\r\nContent-Length: 0\r\nContent-Length: 0\r\n\r\n".as_slice(),
        b"POST /v1/invoke HTTP/1.1\r\nContent-Length: invalid\r\n\r\n".as_slice(),
        b"POST /v1/invoke HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n".as_slice(),
    ] {
        assert_http_request_rejected(raw, 400);
    }
}

#[test]
fn request_reader_rejects_malformed_or_non_utf8_heads() {
    assert_http_request_rejected(b"POST /v1/invoke\r\nHost: localhost\r\n\r\n", 400);
    assert_http_request_rejected(b"POST /v1/invoke HTTP/1.1\r\nMalformed-Header\r\n\r\n", 400);
    let mut non_utf8 = b"GET /health HTTP/1.1\r\nX-Test: ".to_vec();
    non_utf8.push(0xff);
    non_utf8.extend_from_slice(b"\r\n\r\n");
    assert_http_request_rejected(&non_utf8, 400);
}

#[test]
fn request_reader_rejects_truncated_excess_and_non_utf8_bodies() {
    assert_http_request_rejected(
        b"POST /v1/invoke HTTP/1.1\r\nContent-Length: 2\r\n\r\n1",
        400,
    );
    assert_http_request_rejected(
        b"POST /v1/invoke HTTP/1.1\r\nContent-Length: 1\r\n\r\n12",
        400,
    );
    let mut non_utf8 = b"POST /v1/invoke HTTP/1.1\r\nContent-Length: 1\r\n\r\n".to_vec();
    non_utf8.push(0xff);
    assert_http_request_rejected(&non_utf8, 400);
}

#[test]
fn duplicate_authorization_headers_never_supply_a_credential() {
    let request = ParsedHttpRequest::from_raw(
        b"GET /health HTTP/1.1\r\nAuthorization: Bearer trusted\r\nAuthorization: Bearer attacker\r\n\r\n"
            .to_vec(),
    );
    assert!(!request.has_bearer("trusted"));
    assert_eq!(request.authorization_credential("bearer"), None);
}

/// A reader that hands out at most `chunk` bytes per call and counts the calls, so a test can
/// state both how a request is reassembled and how many reads it took.
struct ChunkedReader {
    bytes: Vec<u8>,
    offset: usize,
    chunk: usize,
    reads: usize,
}

impl ChunkedReader {
    fn new(bytes: Vec<u8>, chunk: usize) -> Self {
        Self {
            bytes,
            offset: 0,
            chunk,
            reads: 0,
        }
    }
}

impl Read for ChunkedReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.reads += 1;
        let remaining = self.bytes.len() - self.offset;
        let take = remaining.min(self.chunk).min(buffer.len());
        buffer[..take].copy_from_slice(&self.bytes[self.offset..self.offset + take]);
        self.offset += take;
        Ok(take)
    }
}

#[test]
fn a_request_arriving_one_byte_at_a_time_is_still_split_at_its_first_header_terminator() {
    // The body carries a blank line of its own, so a parser that searched the decoded request for
    // any terminator, or that re-searched from the start of each chunk, would cut it in the wrong
    // place.
    let body = "line\r\n\r\nstill body";
    let raw = format!(
        "POST /v1/ping HTTP/1.1\r\nContent-Length: {}\r\nX-Test: yes\r\n\r\n{body}",
        body.len()
    );
    let mut reader = ChunkedReader::new(raw.clone().into_bytes(), 1);
    let outcome = read_http_request(&mut reader).expect("read the request");
    let HttpReadOutcome::Request(bytes) = outcome else {
        panic!("expected a complete request");
    };
    assert_eq!(bytes, raw.as_bytes());
    let request = ParsedHttpRequest::from_raw(bytes);
    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/v1/ping");
    assert_eq!(request.body, body);
    assert!(request
        .headers
        .iter()
        .any(|(name, value)| name == "X-Test" && value == "yes"));
}

#[test]
fn a_large_body_is_read_in_few_reads_and_handed_over_without_a_copy_per_layer() {
    let body = "b".repeat(200_000);
    let raw = format!(
        "POST /v1/surfaces/resources HTTP/1.1\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    let mut reader = ChunkedReader::new(raw.into_bytes(), HTTP_READ_CHUNK_BYTES);
    let outcome = read_http_request(&mut reader).expect("read the request");
    let HttpReadOutcome::Request(bytes) = outcome else {
        panic!("expected a complete request");
    };
    let expected_reads = 200_000_usize.div_ceil(HTTP_READ_CHUNK_BYTES) + 1;
    assert!(
        reader.reads <= expected_reads,
        "expected at most {expected_reads} reads, took {}",
        reader.reads
    );
    let request = ParsedHttpRequest::from_raw(bytes);
    assert_eq!(request.body.len(), body.len());
    assert_eq!(request.body, body);
}

#[test]
fn a_declared_body_over_the_route_limit_is_rejected_before_the_body_is_read() {
    let declared = MAX_HTTP_BODY_BYTES + 1;
    let raw = format!("POST /v1/invoke HTTP/1.1\r\nContent-Length: {declared}\r\n\r\n");
    let mut reader = ChunkedReader::new(raw.into_bytes(), HTTP_READ_CHUNK_BYTES);
    let outcome = read_http_request(&mut reader).expect("read the request");
    let HttpReadOutcome::Rejected { status, .. } = outcome else {
        panic!("expected the oversized request to be rejected");
    };
    assert_eq!(status, 413);
    assert_eq!(
        reader.reads, 1,
        "the declared length is enough to reject; no body should be read"
    );
}

/// A reader that delivers a request head and then dribbles one byte per `read`, the way a
/// client holding a connection open against a per-read timeout does.
struct TricklingReader {
    head: Vec<u8>,
    offset: usize,
    step: Duration,
}

impl TricklingReader {
    fn new(head: &str, step: Duration) -> Self {
        Self {
            head: head.as_bytes().to_vec(),
            offset: 0,
            step,
        }
    }
}

impl Read for TricklingReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        thread::sleep(self.step);
        if self.offset < self.head.len() {
            let take = (self.head.len() - self.offset).min(buffer.len());
            buffer[..take].copy_from_slice(&self.head[self.offset..self.offset + take]);
            self.offset += take;
            return Ok(take);
        }
        buffer[0] = b'x';
        Ok(1)
    }
}

#[test]
fn a_trickling_request_is_rejected_when_it_outlives_the_read_deadline() {
    // Every read succeeds, so the per-read timeout never fires and cannot end this; only the
    // wall-clock deadline can. Without it the reader stayed here until the body was complete,
    // which for a large declared length is hours.
    let head = "POST /v1/invoke HTTP/1.1\r\nContent-Length: 4096\r\n\r\n";
    let mut reader = TricklingReader::new(head, Duration::from_millis(5));
    let deadline = Instant::now() + Duration::from_millis(200);
    let outcome = read_http_request_until(&mut reader, deadline, &AtomicBool::new(false))
        .expect("read the request");
    let HttpReadOutcome::Rejected { status, .. } = outcome else {
        panic!("expected the trickling request to be rejected");
    };
    assert_eq!(status, 408);
    assert!(Instant::now() >= deadline, "returned before the deadline");
}

#[test]
fn a_read_is_abandoned_once_the_daemon_starts_draining() {
    // Shutdown must not wait on a client that may never finish sending, so a draining reader
    // stops before its next read rather than seeing the request through.
    let head = "POST /v1/invoke HTTP/1.1\r\nContent-Length: 4096\r\n\r\n";
    let mut reader = TricklingReader::new(head, Duration::from_millis(1));
    let deadline = Instant::now() + Duration::from_millis(MAX_REQUEST_READ_MILLIS);
    let outcome = read_http_request_until(&mut reader, deadline, &AtomicBool::new(true))
        .expect("read the request");
    assert!(
        matches!(outcome, HttpReadOutcome::Empty),
        "a draining read should hand back nothing to dispatch"
    );
}

fn http_get(port: u16, path: &str) -> String {
    http_request(port, "GET", path, None)
}

fn http_get_without_auth(port: u16, path: &str) -> String {
    http_request_without_auth(port, "GET", path, None)
}

fn http_post(port: u16, path: &str, body: &str) -> String {
    let response = http_request(port, "POST", path, Some(body));
    assert!(
        response.starts_with("HTTP/1.1 200 OK"),
        "unexpected response: {response}"
    );
    response
        .split_once("\r\n\r\n")
        .expect("response body")
        .1
        .to_string()
}

fn http_json_get(port: u16, path: &str) -> serde_json::Value {
    let response = http_get(port, path);
    assert!(
        response.starts_with("HTTP/1.1 200 OK"),
        "unexpected response: {response}"
    );
    response_json_body(&response)
}

fn http_json_post(port: u16, path: &str, body: &str) -> serde_json::Value {
    let response = http_request(port, "POST", path, Some(body));
    assert!(
        response.starts_with("HTTP/1.1 200 OK"),
        "unexpected response: {response}"
    );
    response_json_body(&response)
}

fn http_json_put(port: u16, path: &str, body: &str) -> serde_json::Value {
    let response = http_request(port, "PUT", path, Some(body));
    assert!(
        response.starts_with("HTTP/1.1 200 OK"),
        "unexpected response: {response}"
    );
    response_json_body(&response)
}

fn shared_local_capability_example(name: &str) -> String {
    std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("local-capability")
            .join(name),
    )
    .expect("read standalone local capability fixture")
}
