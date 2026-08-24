//! Streamable HTTP protocol fixtures.

use super::*;

#[derive(Clone, Copy)]
pub(super) enum DelayedHttpMode {
    Headers,
    Body,
}

pub(super) struct DelayedHttpFixture {
    url: String,
    worker: thread::JoinHandle<()>,
}

pub(super) struct ProtocolFallbackHttpFixture {
    url: String,
    worker: thread::JoinHandle<()>,
}

impl ProtocolFallbackHttpFixture {
    pub(super) fn start() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind protocol fallback fixture");
        let address = listener
            .local_addr()
            .expect("protocol fallback fixture address");
        let worker = thread::spawn(move || {
            for request_index in 0..4 {
                let (mut stream, _) = listener.accept().expect("accept protocol fallback request");
                let request = read_http_fixture_request(&mut stream);
                match request_index {
                    0 | 1 => {
                        let message: JsonValue = request
                            .split_once("\r\n\r\n")
                            .and_then(|(_, body)| serde_json::from_str(body).ok())
                            .expect("protocol fallback initialize JSON");
                        let requested = message["params"]["protocolVersion"]
                            .as_str()
                            .expect("requested protocol version");
                        assert_eq!(requested, MCP_SUPPORTED_PROTOCOL_VERSIONS[request_index]);
                        let lower_request = request.to_ascii_lowercase();
                        assert!(lower_request.contains(&format!(
                            "mcp-protocol-version: {}",
                            MCP_SUPPORTED_PROTOCOL_VERSIONS[request_index]
                        )));
                        if request_index == 0 {
                            write_http_fixture_response(
                                &mut stream,
                                "200 OK",
                                "application/json",
                                None,
                                &serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "id": message["id"],
                                    "error": {
                                        "code": -32602,
                                        "message": "unsupported protocol version"
                                    }
                                })
                                .to_string(),
                            );
                        } else {
                            write_http_fixture_response(
                                &mut stream,
                                "200 OK",
                                "application/json",
                                Some("fallback-session"),
                                &serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "id": message["id"],
                                    "result": {
                                        "protocolVersion": MCP_SUPPORTED_PROTOCOL_VERSIONS[1],
                                        "capabilities": {},
                                        "serverInfo": {
                                            "name": "fallback-fixture",
                                            "version": "0.1.0"
                                        }
                                    }
                                })
                                .to_string(),
                            );
                        }
                    }
                    2 => {
                        assert!(request
                            .to_ascii_lowercase()
                            .contains("mcp-session-id: fallback-session"));
                        write_http_fixture_response(
                            &mut stream,
                            "202 Accepted",
                            "application/json",
                            None,
                            "",
                        );
                    }
                    _ => {
                        assert!(request.starts_with("DELETE "));
                        assert!(request
                            .to_ascii_lowercase()
                            .contains("mcp-session-id: fallback-session"));
                        write_http_fixture_response(
                            &mut stream,
                            "204 No Content",
                            "application/json",
                            None,
                            "",
                        );
                    }
                }
            }
        });
        Self {
            url: format!("http://{address}/mcp"),
            worker,
        }
    }

    pub(super) fn url(&self) -> String {
        self.url.clone()
    }

    pub(super) fn finish(self) {
        self.worker
            .join()
            .expect("protocol fallback fixture worker");
    }
}

impl DelayedHttpFixture {
    pub(super) fn start(mode: DelayedHttpMode) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind delayed HTTP fixture");
        let address = listener.local_addr().expect("delayed HTTP fixture address");
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept delayed HTTP request");
            let request = read_http_fixture_request(&mut stream);
            let message: JsonValue = request
                .split_once("\r\n\r\n")
                .and_then(|(_, body)| serde_json::from_str(body).ok())
                .expect("delayed HTTP fixture JSON");
            let body = serde_json::json!({
                "jsonrpc": "2.0",
                "id": message["id"],
                "result": {
                    "protocolVersion": MCP_PREFERRED_PROTOCOL_VERSION,
                    "capabilities": {},
                    "serverInfo": { "name": "delayed", "version": "0.1.0" }
                }
            })
            .to_string();
            match mode {
                DelayedHttpMode::Headers => {
                    thread::sleep(Duration::from_millis(500));
                    let _ = write_http_fixture_response_unchecked(&mut stream, &body);
                }
                DelayedHttpMode::Body => {
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let split = body.len() / 2;
                    let _ = stream.write_all(&body.as_bytes()[..split]);
                    let _ = stream.flush();
                    thread::sleep(Duration::from_millis(500));
                    let _ = stream.write_all(&body.as_bytes()[split..]);
                    let _ = stream.flush();
                }
            }
        });
        Self {
            url: format!("http://{address}/mcp"),
            worker,
        }
    }

    pub(super) fn url(&self) -> String {
        self.url.clone()
    }

    pub(super) fn finish(self) {
        self.worker.join().expect("delayed HTTP fixture worker");
    }
}

pub(super) fn write_http_fixture_response_unchecked(
    stream: &mut TcpStream,
    body: &str,
) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    stream.flush()
}

#[derive(Clone, Copy)]
pub(super) enum SessionCloseMode {
    Success,
    Unsupported,
    Delayed,
}

pub(super) struct SessionCloseHttpFixture {
    url: String,
    worker: thread::JoinHandle<()>,
}

impl SessionCloseHttpFixture {
    pub(super) fn start(mode: SessionCloseMode) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind close HTTP fixture");
        let address = listener.local_addr().expect("close HTTP fixture address");
        let worker = thread::spawn(move || {
            for request_index in 0..3 {
                let (mut stream, _) = listener.accept().expect("accept close HTTP request");
                let request = read_http_fixture_request(&mut stream);
                match request_index {
                    0 => {
                        assert!(request.starts_with("POST "));
                        let message: JsonValue = request
                            .split_once("\r\n\r\n")
                            .and_then(|(_, body)| serde_json::from_str(body).ok())
                            .expect("close fixture initialize JSON");
                        write_http_fixture_response(
                            &mut stream,
                            "200 OK",
                            "application/json",
                            Some("close-session"),
                            &serde_json::json!({
                                "jsonrpc": "2.0",
                                "id": message["id"],
                                "result": {
                                    "protocolVersion": MCP_PREFERRED_PROTOCOL_VERSION,
                                    "capabilities": {},
                                    "serverInfo": { "name": "close-fixture", "version": "0.1.0" }
                                }
                            })
                            .to_string(),
                        );
                    }
                    1 => write_http_fixture_response(
                        &mut stream,
                        "202 Accepted",
                        "application/json",
                        None,
                        "",
                    ),
                    _ => {
                        assert!(request.starts_with("DELETE "));
                        assert!(request
                            .to_ascii_lowercase()
                            .contains("mcp-session-id: close-session"));
                        match mode {
                            SessionCloseMode::Success => write_http_fixture_response(
                                &mut stream,
                                "204 No Content",
                                "application/json",
                                None,
                                "",
                            ),
                            SessionCloseMode::Unsupported => write_http_fixture_response(
                                &mut stream,
                                "405 Method Not Allowed",
                                "text/plain",
                                None,
                                "unsupported",
                            ),
                            SessionCloseMode::Delayed => {
                                thread::sleep(Duration::from_millis(500));
                                let _ = write_http_fixture_response_unchecked(&mut stream, "");
                            }
                        }
                    }
                }
            }
        });
        Self {
            url: format!("http://{address}/mcp"),
            worker,
        }
    }

    pub(super) fn url(&self) -> String {
        self.url.clone()
    }

    pub(super) fn finish(self) {
        self.worker.join().expect("close HTTP fixture worker");
    }
}

pub(super) struct StreamableHttpFixture {
    url: String,
    worker: thread::JoinHandle<()>,
}

impl StreamableHttpFixture {
    pub(super) fn start() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind HTTP MCP fixture");
        let address = listener.local_addr().expect("HTTP MCP fixture address");
        let worker = thread::spawn(move || {
            for request_index in 0..4 {
                let (mut stream, _) = listener.accept().expect("accept HTTP MCP request");
                let request = read_http_fixture_request(&mut stream);
                assert!(request
                    .to_ascii_lowercase()
                    .contains("accept: application/json, text/event-stream"));
                assert!(request
                    .to_ascii_lowercase()
                    .contains("authorization: bearer fixture-token"));
                if request_index > 0 {
                    assert!(request
                        .to_ascii_lowercase()
                        .contains("mcp-session-id: fixture-session"));
                }
                let body = request
                    .split_once("\r\n\r\n")
                    .map(|(_, body)| body)
                    .unwrap_or("{}");
                let message: JsonValue = serde_json::from_str(body).expect("HTTP MCP fixture JSON");
                match message["method"].as_str().unwrap_or_default() {
                    "initialize" => write_http_fixture_response(
                        &mut stream,
                        "200 OK",
                        "application/json",
                        Some("fixture-session"),
                        &serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": message["id"],
                            "result": {
                                "protocolVersion": MCP_PREFERRED_PROTOCOL_VERSION,
                                "capabilities": { "tools": {} },
                                "serverInfo": { "name": "loom-http-fixture", "version": "0.1.0" }
                            }
                        })
                        .to_string(),
                    ),
                    "notifications/initialized" => write_http_fixture_response(
                        &mut stream,
                        "202 Accepted",
                        "application/json",
                        None,
                        "",
                    ),
                    "tools/list" => write_http_fixture_response(
                        &mut stream,
                        "200 OK",
                        "text/event-stream",
                        None,
                        &format!(
                            "event: message\ndata: {}\n\n",
                            serde_json::json!({
                                "jsonrpc": "2.0",
                                "id": message["id"],
                                "result": { "tools": [{ "name": "echo", "inputSchema": { "type": "object" } }] }
                            })
                        ),
                    ),
                    "tools/call" => write_http_fixture_response(
                        &mut stream,
                        "200 OK",
                        "application/json",
                        None,
                        &serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": message["id"],
                            "result": {
                                "content": [{ "type": "text", "text": message["params"]["arguments"]["text"] }]
                            }
                        })
                        .to_string(),
                    ),
                    method => panic!("unexpected HTTP MCP method {method}"),
                }
            }
        });
        Self {
            url: format!("http://{address}/mcp"),
            worker,
        }
    }

    pub(super) fn url(&self) -> String {
        self.url.clone()
    }

    pub(super) fn finish(self) {
        self.worker.join().expect("HTTP MCP fixture worker");
    }
}

pub(super) fn read_http_fixture_request(stream: &mut TcpStream) -> String {
    let mut request = Vec::new();
    let mut buffer = [0u8; 4096];
    loop {
        let read = stream.read(&mut buffer).expect("read HTTP MCP request");
        if read == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..read]);
        let text = String::from_utf8_lossy(&request);
        let Some(header_end) = text.find("\r\n\r\n") else {
            continue;
        };
        let content_length = text[..header_end]
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.trim()
                    .eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            })
            .unwrap_or(0);
        if request.len() >= header_end + 4 + content_length {
            break;
        }
    }
    String::from_utf8(request).expect("HTTP MCP request UTF-8")
}

pub(super) fn write_http_fixture_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    session_id: Option<&str>,
    body: &str,
) {
    let session_header = session_id
        .map(|value| format!("MCP-Session-Id: {value}\r\n"))
        .unwrap_or_default();
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\n{session_header}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .expect("write HTTP MCP response");
    stream.flush().expect("flush HTTP MCP response");
}
