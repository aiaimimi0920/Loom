// Loom daemon tests fragment 21; included into the shared crate test module.
impl CloudApiFixture {
    fn start(mode: CloudApiFixtureMode) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind cloud API fixture");
        let port = listener
            .local_addr()
            .expect("cloud API fixture address")
            .port();
        let captured_request = Arc::new(Mutex::new(None));
        let worker_captured_request = Arc::clone(&captured_request);
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept cloud API fixture request");
            let request = read_cloud_fixture_request(&mut stream);
            *worker_captured_request
                .lock()
                .expect("lock cloud request capture") = Some(request.clone());
            let Some((_, body)) = request.split_once("\r\n\r\n") else {
                return;
            };
            let prompt = serde_json::from_str::<serde_json::Value>(body)
                .ok()
                .and_then(|value| {
                    value
                        .get("prompt")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
                .unwrap_or_default();
            let response = match mode {
                CloudApiFixtureMode::Text => serde_json::json!({
                    "content": [
                        {
                            "type": "text",
                            "text": format!("cloud saw {prompt}")
                        }
                    ]
                }),
                CloudApiFixtureMode::Image(image_data) => serde_json::json!({
                    "content": [
                        {
                            "type": "image",
                            "data": image_data,
                            "mimeType": "image/png"
                        }
                    ]
                }),
                CloudApiFixtureMode::MultipartImage(image_data) => serde_json::json!({
                    "content": [
                        {
                            "type": "image",
                            "data": image_data,
                            "mimeType": "image/png"
                        }
                    ]
                }),
            };
            write_cloud_fixture_response(
                &mut stream,
                "200 OK",
                "application/json",
                &response.to_string(),
            );
        });
        Self {
            port,
            worker: Some(worker),
            captured_request,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }

    fn request(&self) -> String {
        self.captured_request
            .lock()
            .expect("lock cloud request capture")
            .clone()
            .expect("captured cloud request")
    }
}

impl Drop for CloudApiFixture {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = TcpStream::connect(("127.0.0.1", self.port));
            let _ = worker.join();
        }
    }
}

struct HttpImageFixture {
    port: u16,
    worker: Option<thread::JoinHandle<()>>,
}

impl HttpImageFixture {
    fn start(content_type: &'static str, body: Vec<u8>) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind HTTP image fixture");
        let port = listener
            .local_addr()
            .expect("HTTP image fixture address")
            .port();
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener
                .accept()
                .expect("accept HTTP image fixture request");
            let _ = read_cloud_fixture_request(&mut stream);
            let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
            let _ = stream.write_all(&body);
            let _ = stream.flush();
        });
        Self {
            port,
            worker: Some(worker),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }
}

impl Drop for HttpImageFixture {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = TcpStream::connect(("127.0.0.1", self.port));
            let _ = worker.join();
        }
    }
}

fn read_cloud_fixture_request(stream: &mut TcpStream) -> String {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 8192];
    // Read headers first (until CRLFCRLF), then the body per Content-Length.
    // A single 8 KB read stops at the header boundary because reqwest sends
    // the multipart body in a later packet, so multipart assertions (file
    // parts, field values) would miss the body without this.
    loop {
        let read = stream.read(&mut chunk).expect("read cloud fixture request");
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        let text = String::from_utf8_lossy(&buffer);
        if let Some(header_end) = text.find("\r\n\r\n") {
            let headers = &text[..header_end];
            let content_length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.trim()
                        .eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .unwrap_or(0);
            let body_start = header_end + 4;
            if buffer.len() >= body_start + content_length {
                break;
            }
        }
    }
    String::from_utf8_lossy(&buffer).to_string()
}

fn write_cloud_fixture_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &str,
) {
    let _ = write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
    let _ = stream.flush();
}

struct TranslateFixture {
    port: u16,
    worker: Option<thread::JoinHandle<()>>,
    captured_request: Arc<Mutex<Option<String>>>,
}

impl TranslateFixture {
    fn start() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind translate fixture");
        let port = listener
            .local_addr()
            .expect("translate fixture address")
            .port();
        let captured_request = Arc::new(Mutex::new(None));
        let worker_captured_request = Arc::clone(&captured_request);
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept translate fixture request");
            let request = read_cloud_fixture_request(&mut stream);
            *worker_captured_request
                .lock()
                .expect("lock translate request capture") = Some(request.clone());
            let body = request
                .split_once("\r\n\r\n")
                .map(|(_, body)| body)
                .unwrap_or("{}");
            let payload = serde_json::from_str::<Value>(body).unwrap_or_else(|_| json!({}));
            let text = payload
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let target_lang = payload
                .get("target_lang")
                .or_else(|| payload.get("targetLang"))
                .and_then(Value::as_str)
                .unwrap_or("auto");
            let response = json!({
                "code": 200,
                "data": format!("translated:{text}:{target_lang}")
            });
            write_cloud_fixture_response(
                &mut stream,
                "200 OK",
                "application/json",
                &response.to_string(),
            );
        });
        Self {
            port,
            worker: Some(worker),
            captured_request,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }

    fn request(&self) -> String {
        self.captured_request
            .lock()
            .expect("lock translate request capture")
            .clone()
            .expect("captured translate request")
    }
}

impl Drop for TranslateFixture {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = TcpStream::connect(("127.0.0.1", self.port));
            let _ = worker.join();
        }
    }
}

struct McpRegistryFixture {
    port: u16,
    worker: Option<thread::JoinHandle<()>>,
    request_path: Arc<Mutex<Option<String>>>,
}

impl McpRegistryFixture {
    fn start() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind MCP registry fixture");
        let port = listener
            .local_addr()
            .expect("MCP registry fixture address")
            .port();
        let request_path = Arc::new(Mutex::new(None));
        let worker_request_path = Arc::clone(&request_path);
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept MCP registry request");
            let request = read_cloud_fixture_request(&mut stream);
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/")
                .to_owned();
            *worker_request_path
                .lock()
                .expect("lock MCP registry request path") = Some(path);
            write_cloud_fixture_response(
                &mut stream,
                "200 OK",
                "application/json",
                r#"{"servers":[{"server":{"name":"io.modelcontextprotocol/fixture","title":"Fixture MCP","description":"Fixture registry server","packages":[{"registryType":"npm","identifier":"@fixture/mcp","version":"1.0.0","transport":{"type":"stdio"},"runtimeArguments":[{"value":"-y"}],"environmentVariables":[{"name":"FIXTURE_API_KEY","isRequired":true}]}]},"_meta":{"io.modelcontextprotocol.registry/official":{"status":"active","isLatest":true,"updatedAt":"2026-06-12T00:00:00Z"}}}],"metadata":{"count":1}}"#,
            );
        });
        Self {
            port,
            worker: Some(worker),
            request_path,
        }
    }

    fn start_flaky() -> Self {
        let listener =
            TcpListener::bind(("127.0.0.1", 0)).expect("bind flaky MCP registry fixture");
        let port = listener
            .local_addr()
            .expect("flaky MCP registry fixture address")
            .port();
        let request_path = Arc::new(Mutex::new(None));
        let worker_request_path = Arc::clone(&request_path);
        let worker = thread::spawn(move || {
            let (mut first, _) = listener.accept().expect("accept failed registry request");
            let _ = read_cloud_fixture_request(&mut first);
            let _ = first.shutdown(Shutdown::Both);

            let (mut second, _) = listener.accept().expect("accept retried registry request");
            let request = read_cloud_fixture_request(&mut second);
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/")
                .to_owned();
            *worker_request_path
                .lock()
                .expect("lock retried MCP registry request path") = Some(path);
            write_cloud_fixture_response(
                &mut second,
                "200 OK",
                "application/json",
                r#"{"servers":[{"server":{"name":"io.modelcontextprotocol/fixture","title":"Fixture MCP","description":"Fixture registry server","packages":[{"registryType":"npm","identifier":"@fixture/mcp","version":"1.0.0","transport":{"type":"stdio"}}]}}],"metadata":{"count":1}}"#,
            );
        });
        Self {
            port,
            worker: Some(worker),
            request_path,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }

    fn request_path(&self) -> String {
        self.request_path
            .lock()
            .expect("lock MCP registry request path")
            .clone()
            .expect("captured MCP registry request path")
    }
}

impl Drop for McpRegistryFixture {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = TcpStream::connect(("127.0.0.1", self.port));
            let _ = worker.join();
        }
    }
}

#[test]
fn daemon_mcp_fixture_server() {
    if std::env::var("LOOM_DAEMON_MCP_FIXTURE_SERVER")
        .ok()
        .as_deref()
        != Some("1")
    {
        return;
    }

    run_mcp_fixture_server();
    std::process::exit(0);
}

#[test]
fn daemon_auth_token_is_generated_persisted_reused_and_corruption_fails_closed() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let root = unique_temp_dir("daemon-token");
    let previous_token = std::env::var("LOOM_DAEMON_TOKEN").ok();
    std::env::remove_var("LOOM_DAEMON_TOKEN");

    let generated = resolve_daemon_auth_token(None, &root).expect("generate daemon token");
    assert_eq!(generated.len(), 43);
    assert_eq!(
        fs::read_to_string(root.join(DAEMON_AUTH_TOKEN_FILE)).expect("read daemon token file"),
        generated
    );
    assert_eq!(
        resolve_daemon_auth_token(None, &root).expect("reuse daemon token"),
        generated
    );
    assert_eq!(
        resolve_daemon_auth_token(Some("configured-token".to_owned()), &root)
            .expect("configured daemon token"),
        "configured-token"
    );

    fs::write(root.join(DAEMON_AUTH_TOKEN_FILE), "\n").expect("corrupt daemon token");
    let error = resolve_daemon_auth_token(None, &root)
        .expect_err("an empty persisted token must fail closed");
    assert!(error.to_string().contains("is empty"));

    restore_env("LOOM_DAEMON_TOKEN", previous_token);
    fs::remove_dir_all(root).expect("cleanup daemon token root");
}
