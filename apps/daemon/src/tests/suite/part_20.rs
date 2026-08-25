// Loom daemon tests fragment 20; included into the shared crate test module.
#[test]
fn hook_art_replacement_and_cancellation_reclaim_owned_resources() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    clear_hook_canvas_runtime_state(None);
    let store = Arc::new(Mutex::new(SharedImageStore::new()));
    let first = hook_art_request("request:first-resource", "node:resource", 1);
    let first_token = match reserve_hook_art_request(&first, &store) {
        HookArtReservation::Execute(token) => token,
        _ => panic!("first request must execute"),
    };
    let first_image = store
        .lock()
        .expect("shared image store")
        .create_rgba8(1, 1, vec![1, 2, 3, 255])
        .expect("create first image");
    assert!(register_hook_art_resource_handles(
        &first,
        &BTreeSet::from([first_image.handle.clone()]),
        false,
    ));

    let replacement = hook_art_request("request:replacement-resource", "node:resource", 2);
    let replacement_token = match reserve_hook_art_request(&replacement, &store) {
        HookArtReservation::Execute(token) => token,
        _ => panic!("replacement request must execute"),
    };
    assert!(first_token.load(Ordering::Acquire));
    assert!(store.lock().expect("shared image store").list().is_empty());

    let replacement_image = store
        .lock()
        .expect("shared image store")
        .create_rgba8(1, 1, vec![4, 5, 6, 255])
        .expect("create replacement image");
    assert!(register_hook_art_resource_handles(
        &replacement,
        &BTreeSet::from([replacement_image.handle.clone()]),
        false,
    ));
    let cancellation = HookArtCancelRequest {
        protocol_version: loom_protocol::HOOK_PROTOCOL_VERSION.to_owned(),
        request_id: replacement.request_id.clone(),
        node_id: replacement.node_id.clone(),
        generation: replacement.generation,
        device_id: replacement.device_id.clone(),
    };
    let response: HookResponse =
        serde_json::from_str(&cancel_hook_art_request(&cancellation, &store))
            .expect("cancel response");
    assert_eq!(response.status, HookRequestStatus::CancelRequested);
    assert!(replacement_token.load(Ordering::Acquire));
    assert!(store.lock().expect("shared image store").list().is_empty());

    let late_image = store
        .lock()
        .expect("shared image store")
        .create_rgba8(1, 1, vec![7, 8, 9, 255])
        .expect("create late image");
    let late_handles = BTreeSet::from([late_image.handle.clone()]);
    assert!(!register_hook_art_resource_handles(
        &replacement,
        &late_handles,
        false,
    ));
    release_shared_image_handles(&store, late_handles);
    assert!(store.lock().expect("shared image store").list().is_empty());
    clear_hook_canvas_runtime_state(Some(&store));
}

#[test]
fn hook_art_terminal_eviction_reclaims_unreleased_resources() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    clear_hook_canvas_runtime_state(None);
    let store = Arc::new(Mutex::new(SharedImageStore::new()));
    let first = hook_art_request("request:terminal:0", "node:terminal:0", 1);
    assert!(matches!(
        reserve_hook_art_request(&first, &store),
        HookArtReservation::Execute(_)
    ));
    let image = store
        .lock()
        .expect("shared image store")
        .create_rgba8(1, 1, vec![1, 2, 3, 255])
        .expect("create terminal image");
    assert!(register_hook_art_resource_handles(
        &first,
        &BTreeSet::from([image.handle.clone()]),
        true,
    ));
    finish_hook_art_request(
        &first.request_id,
        &first.node_id,
        first.generation,
        first.device_id.as_deref(),
        hook_art_request_fingerprint(&first),
        HookRequestStatus::Succeeded,
        "{}".to_owned(),
        &store,
    );
    assert_eq!(store.lock().expect("shared image store").list().len(), 1);

    for index in 1..=MAX_HOOK_ART_TERMINAL_REQUESTS {
        let execution = hook_art_request(
            &format!("request:terminal:{index}"),
            &format!("node:terminal:{index}"),
            1,
        );
        assert!(matches!(
            reserve_hook_art_request(&execution, &store),
            HookArtReservation::Execute(_)
        ));
        finish_hook_art_request(
            &execution.request_id,
            &execution.node_id,
            execution.generation,
            execution.device_id.as_deref(),
            hook_art_request_fingerprint(&execution),
            HookRequestStatus::Succeeded,
            "{}".to_owned(),
            &store,
        );
    }
    assert!(store.lock().expect("shared image store").list().is_empty());
    clear_hook_canvas_runtime_state(Some(&store));
}

#[test]
fn daemon_hook_bridge_executes_cloud_api_art_node_image_output() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let root = unique_temp_dir("cloud-art-node");
    let image_data = test_png_base64();
    let fixture = CloudApiFixture::start(CloudApiFixtureMode::Image(image_data.clone()));
    let daemon = LoomDaemon::bind(DaemonConfig::localhost(0).with_control_plane_root(&root))
        .expect("bind daemon");
    let address = daemon.local_addr().expect("local address");
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

    let saved_tool = http_json_put(
        address.port(),
        "/v1/tools/fixture-cloud-art",
        &serde_json::json!({
            "id": "fixture-cloud-art",
            "name": "Fixture Cloud Art",
            "description": "Execute fixture cloud Art through Hook bridge",
            "enabled": true,
            "execution": {
                "type": "cloud_api",
                "endpoint": fixture.url("/image"),
                "method": "POST"
            },
            // A cloud Art only reaches a loopback endpoint when it declares that it wants to.
            "metadata": {
                "permissionPolicy": { "network": { "allowLocalhost": true } }
            }
        })
        .to_string(),
    );
    assert_eq!(saved_tool["tool"]["id"], "fixture-cloud-art");

    let started = http_json_post(address.port(), "/v1/hook-bridge/start", r#"{"port":0}"#);
    let bridge_port = started["port"].as_u64().expect("bridge port") as u16;
    let mut socket = connect_hook_bridge_websocket(bridge_port);

    socket
        .send(tungstenite::Message::Text(formal_art_execute_request(
            "cloud-image",
            "node-cloud",
            "fixture-cloud-art",
            Some(inline_art_input(&image_data)),
            json!({}),
        )))
        .expect("send cloud execute art node");
    let response = read_hook_terminal_response(&mut socket, "cloud-image");

    assert_eq!(response["status"], "succeeded", "response={response}");
    assert_eq!(response["data"]["nodeId"], "node-cloud");
    assert!(response["data"]["outputs"]["output"]["handle"].is_string());

    drop(socket);
    let stopped = http_json_post(address.port(), "/v1/hook-bridge/stop", "{}");
    assert_eq!(stopped["running"], false);

    shutdown_tx.send(()).expect("shutdown");
    server.join().expect("server thread");
    fs::remove_dir_all(root).expect("cleanup cloud art node root");
}

#[test]
fn daemon_hook_bridge_executes_cloud_api_multipart_art_node_with_input_file() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let root = unique_temp_dir("cloud-multipart-art-node");
    let image_data = test_png_base64();
    let fixture = CloudApiFixture::start(CloudApiFixtureMode::MultipartImage(image_data.clone()));
    let daemon = LoomDaemon::bind(DaemonConfig::localhost(0).with_control_plane_root(&root))
        .expect("bind daemon");
    let address = daemon.local_addr().expect("local address");
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

    let saved_tool = http_json_put(
            address.port(),
            "/v1/tools/fixture-cloud-multipart-art",
            &serde_json::json!({
                "id": "fixture-cloud-multipart-art",
                "name": "Fixture Cloud Multipart Art",
                "description": "Execute multipart cloud Art through Hook bridge",
                "enabled": true,
                "execution": {
                    "type": "cloud_api",
                    "endpoint": fixture.url("/multipart/{{inputs.route.value}}"),
                    "method": "POST",
                    "contentType": "multipart/form-data",
                    "headers": "{\"X-Trace\":\"{{inputs.trace.value}}\"}",
                    "body": "{\"file\":\"{{inputs.input.path}}\",\"prompt\":\"{{inputs.prompt.value}}\"}"
                },
                // A cloud Art only reaches a loopback endpoint when it declares that it wants to.
                "metadata": {
                    "permissionPolicy": { "network": { "allowLocalhost": true } }
                }
            })
            .to_string(),
        );
    assert_eq!(saved_tool["tool"]["id"], "fixture-cloud-multipart-art");

    let started = http_json_post(address.port(), "/v1/hook-bridge/start", r#"{"port":0}"#);
    let bridge_port = started["port"].as_u64().expect("bridge port") as u16;
    let mut socket = connect_hook_bridge_websocket(bridge_port);

    socket
        .send(tungstenite::Message::Text(formal_art_execute_request(
            "cloud-multipart",
            "node-cloud-multipart",
            "fixture-cloud-multipart-art",
            Some(inline_art_input(&image_data)),
            json!({
                "route": "image",
                "trace": "trace-bridge",
                "prompt": "hello cloud multipart"
            }),
        )))
        .expect("send multipart cloud execute art node");
    let response = read_hook_terminal_response(&mut socket, "cloud-multipart");

    assert_eq!(response["status"], "succeeded", "response={response}");
    assert_eq!(response["data"]["nodeId"], "node-cloud-multipart");
    assert!(response["data"]["outputs"]["output"]["handle"].is_string());

    let request = fixture.request();
    let request_lower = request.to_ascii_lowercase();
    assert!(request.starts_with("POST /multipart/image HTTP/1.1"));
    assert!(request_lower.contains("x-trace: trace-bridge"));
    assert!(request_lower.contains("content-type: multipart/form-data; boundary="));
    assert!(request.contains("name=\"file\""));
    assert!(request.contains("filename=\"loom-cloud-input.png\""));
    assert!(request.contains("name=\"prompt\""));
    assert!(request.contains("\r\nhello cloud multipart\r\n"));
    assert!(!request.contains("{{"));

    drop(socket);
    let stopped = http_json_post(address.port(), "/v1/hook-bridge/stop", "{}");
    assert_eq!(stopped["running"], false);

    shutdown_tx.send(()).expect("shutdown");
    server.join().expect("server thread");
    fs::remove_dir_all(root).expect("cleanup cloud multipart art node root");
}

// Formal inline resources must be materialized to a file so multipart
// templates can upload the real image bytes.

fn connect_hook_bridge_websocket(bridge_port: u16) -> tungstenite::WebSocket<TcpStream> {
    let stream = TcpStream::connect(("127.0.0.1", bridge_port)).expect("connect bridge tcp socket");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("set websocket read timeout");
    stream
        .set_write_timeout(Some(Duration::from_secs(10)))
        .expect("set websocket write timeout");
    tungstenite::client(format!("ws://127.0.0.1:{bridge_port}"), stream)
        .expect("connect bridge websocket")
        .0
}

fn read_hook_bridge_json(socket: &mut tungstenite::WebSocket<TcpStream>) -> serde_json::Value {
    let response = socket.read().expect("read websocket frame");
    let response = response.into_text().expect("text frame");
    serde_json::from_str(&response).expect("response json")
}

fn test_png_base64() -> String {
    format!("data:image/png;base64,{}", BASE64.encode(test_png_bytes()))
}

fn test_png_bytes() -> Vec<u8> {
    let image = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(1, 1, vec![10, 20, 30, 255])
        .expect("test png image");
    let mut png = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut png, ImageFormat::Png)
        .expect("encode test png");
    png.into_inner()
}

fn packaged_ocr_fixture_base64() -> String {
    let image = fs::read(
        workspace_ocr_resources()
            .join("fixtures")
            .join("test_1.png"),
    )
    .expect("read packaged OCR fixture");
    format!("data:image/png;base64,{}", BASE64.encode(image))
}

fn workspace_ocr_resources() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find_map(|candidate| {
            let path = candidate.join("resources").join("ocr");
            path.join("ch_PP-OCRv4_det_infer.onnx")
                .exists()
                .then_some(path)
        })
        .expect("locate Loom/resources/ocr")
}

struct GatewayBrainPlanFixture {
    port: u16,
    worker: Option<thread::JoinHandle<()>>,
    captured_request: Arc<Mutex<Option<String>>>,
}

impl GatewayBrainPlanFixture {
    fn start(status: &'static str, body: String) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind Gateway fixture");
        let port = listener
            .local_addr()
            .expect("Gateway fixture address")
            .port();
        let captured_request = Arc::new(Mutex::new(None));
        let worker_captured_request = Arc::clone(&captured_request);
        let worker = thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let request = read_gateway_fixture_request(&mut stream);
            *worker_captured_request
                .lock()
                .expect("lock Gateway request capture") = Some(request);
            write_cloud_fixture_response(&mut stream, status, "application/json", &body);
        });
        Self {
            port,
            worker: Some(worker),
            captured_request,
        }
    }

    fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    fn request(&self) -> String {
        self.captured_request
            .lock()
            .expect("lock Gateway request capture")
            .clone()
            .expect("captured Gateway request")
    }
}

impl Drop for GatewayBrainPlanFixture {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = TcpStream::connect(("127.0.0.1", self.port));
            let _ = worker.join();
        }
    }
}

fn read_gateway_fixture_request(stream: &mut TcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("set Gateway fixture read timeout");
    let mut request = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        let bytes = stream
            .read(&mut chunk)
            .expect("read Gateway fixture request");
        if bytes == 0 {
            break;
        }
        request.extend_from_slice(&chunk[..bytes]);
        let Some(header_end) = request
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .map(|position| position + 4)
        else {
            continue;
        };
        let headers = String::from_utf8_lossy(&request[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length").then(|| {
                    value
                        .trim()
                        .parse::<usize>()
                        .expect("Gateway content length")
                })
            })
            .unwrap_or(0);
        if request.len() >= header_end + content_length {
            break;
        }
    }
    String::from_utf8(request).expect("Gateway fixture request UTF-8")
}

enum CloudApiFixtureMode {
    Text,
    Image(String),
    MultipartImage(String),
}

struct CloudApiFixture {
    port: u16,
    worker: Option<thread::JoinHandle<()>>,
    captured_request: Arc<Mutex<Option<String>>>,
}
