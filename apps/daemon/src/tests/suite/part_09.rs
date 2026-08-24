// Loom daemon tests fragment 9; included into the shared crate test module.
#[test]
fn concurrency_fixture_requests_shutdown_before_joining_clients() {
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let (client_release_tx, client_release_rx) = mpsc::channel();
    let server = thread::spawn(move || -> Result<()> {
        shutdown_rx
            .recv_timeout(Duration::from_secs(3))
            .context("wait fixture shutdown")?;
        let _ = client_release_tx.send(());
        Ok(())
    });
    let mut fixture = ConcurrencyTestFixture::new(shutdown_tx, server);
    let client_result_rx = fixture.spawn_client(move || {
        client_release_rx
            .recv_timeout(Duration::from_secs(2))
            .is_ok()
    });

    let (finish_tx, finish_rx) = mpsc::channel();
    let finish_thread = thread::spawn(move || {
        let _ = finish_tx.send(fixture.finish());
    });
    let early_result = finish_rx.recv_timeout(Duration::from_millis(750));
    let completed_before_client_timeout = early_result.is_ok();
    let finish_result = early_result.unwrap_or_else(|_| {
        finish_rx
            .recv_timeout(Duration::from_secs(3))
            .expect("fixture finish after client timeout")
    });
    let client_unblocked_by_shutdown = client_result_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("fixture client result");
    finish_thread.join().expect("fixture finish thread");

    finish_result.expect("finish fixture");
    assert!(
        completed_before_client_timeout,
        "fixture joined clients before requesting daemon shutdown"
    );
    assert!(
        client_unblocked_by_shutdown,
        "daemon shutdown did not unblock the fixture client"
    );
}

fn unique_temp_dir(name: &str) -> PathBuf {
    static NEXT_TEMP_ID: AtomicUsize = AtomicUsize::new(1);
    let mut dir = std::env::temp_dir();
    dir.push(format!(
        "loom-daemon-contract-{}-{}-{}",
        name,
        std::process::id(),
        NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed),
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn canonical_test_path(path: impl AsRef<Path>) -> PathBuf {
    fs::canonicalize(path).expect("canonicalize test path")
}

fn test_daemon_runtime_from_config(
    control_plane_root: &Path,
    mut config: DaemonConfig,
) -> DaemonRuntime {
    let config_root = config
        .configuration_root
        .take()
        .unwrap_or_else(|| control_plane_root.join("config"));
    let run_store: Box<dyn RunEvidenceStore> = match config.run_store {
        RunStoreConfig::Memory => Box::new(InMemoryRunEvidenceStore::default()),
        RunStoreConfig::Sqlite(path) => {
            Box::new(SqliteRunEvidenceStore::open(path).expect("open test sqlite run store"))
        }
    };
    let run_store_status = run_store.status();
    let brain_planner = build_brain_planner(config.brain_planner).expect("build test planner");
    let settings_base_url = std::env::var("LOOM_SETTINGS_BASE_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:0/settings".to_owned());
    let mcp_servers = Arc::new(Mutex::new(load_persisted_mcp_servers(control_plane_root)));
    let tool_registry = ToolRegistry::new(control_plane_root.join("tools"));
    let workflow_store = WorkflowStore::new(control_plane_root.join("workflows"));
    let hook_bridge = Arc::new(Mutex::new(HookBridgeRuntime::new(
        control_plane_root.join("workflows"),
    )));
    let surface_instances = Arc::new(Mutex::new(
        SurfaceInstanceStore::new(
            control_plane_root
                .join("surface-instances")
                .join("instances.json"),
        )
        .expect("open test Surface instance store"),
    ));
    let surface_resources = Arc::new(Mutex::new(
        SurfaceResourceStore::new(control_plane_root.join("surface-resources"))
            .expect("open test Surface resource store"),
    ));
    let surface_actions = Arc::new(
        SurfaceActionExecutor::new(
            Arc::clone(&mcp_servers),
            tool_registry.clone(),
            workflow_store.clone(),
            FrameworkRegistry::new(control_plane_root),
            control_plane_root.to_path_buf(),
            Arc::clone(&surface_instances),
            Arc::clone(&surface_resources),
            Arc::clone(&hook_bridge),
        )
        .expect("start test Surface action executor"),
    );
    DaemonRuntime {
        hook_settings: config.hook_settings,
        run_store: Arc::new(Mutex::new(run_store)),
        auth_token: config
            .auth_token
            .unwrap_or_else(|| TEST_DAEMON_AUTH_TOKEN.to_owned()),
        config_registry: Arc::new(built_in_registry()),
        config_store: FileDocumentStore::new(config_root),
        mcp_servers,
        tool_registry,
        workflow_store,
        canvas_workflow_root: control_plane_root.join("canvas-workflows"),
        framework_registry: FrameworkRegistry::new(&control_plane_root),
        control_plane_root: control_plane_root.to_path_buf(),
        bundled_art_sha256_allowlist: config.bundled_art_sha256_allowlist,
        hook_bridge,
        device_registry: Arc::new(Mutex::new(
            DeviceRegistryStore::new(
                control_plane_root.join("settings").join("devices.json"),
                "127.0.0.1:0".parse().expect("test daemon address"),
            )
            .expect("open test device registry"),
        )),
        surface_instances,
        surface_actions,
        surface_resources,
        settings: Arc::new(Mutex::new(LoomSettingsStore::new(
            control_plane_root.join("settings").join("settings.json"),
        ))),
        shared_images: Arc::new(Mutex::new(SharedImageStore::new())),
        ocr_provider: Arc::new(Mutex::new(OcrProvider::from_env())),
        settings_base_url,
        mcp_registry_endpoint: config.mcp_registry_endpoint,
        brain_planner,
        run_store_status,
        request_executor_status: config.request_executor.status(),
        serialized_route_lock: Mutex::new(()),
        #[cfg(test)]
        serialized_route_observer: None,
        #[cfg(test)]
        request_submission_observer: None,
        #[cfg(test)]
        shutdown_observer: None,
        #[cfg(test)]
        connection_accept_observer: None,
    }
}

fn test_daemon_runtime(control_plane_root: &Path, auth_token: Option<&str>) -> DaemonRuntime {
    let mut config = DaemonConfig::localhost(0);
    if let Some(token) = auth_token {
        config = config.with_bearer_token(token);
    }
    test_daemon_runtime_from_config(control_plane_root, config)
}

fn parsed_request(
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<&str>,
) -> ParsedHttpRequest {
    let mut headers = headers
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect::<Vec<_>>();
    if !headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("host"))
    {
        headers.push(("Host".to_owned(), "127.0.0.1:8765".to_owned()));
    }
    if body.is_some()
        && !headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("content-type"))
    {
        headers.push(("Content-Type".to_owned(), "application/json".to_owned()));
    }
    if !headers.iter().any(|(name, _)| {
        name.eq_ignore_ascii_case("authorization") || name.eq_ignore_ascii_case("cookie")
    }) {
        headers.push((
            "Authorization".to_owned(),
            format!("Bearer {TEST_DAEMON_AUTH_TOKEN}"),
        ));
    }
    ParsedHttpRequest {
        method: method.to_owned(),
        path: path.to_owned(),
        headers,
        body: body.unwrap_or_default().to_owned(),
    }
}

fn expect_text_route_response(response: RouteResponse, expected_status: u16) -> String {
    match response {
        RouteResponse::Text { status, body }
        | RouteResponse::TextWithHeaders { status, body, .. } => {
            assert_eq!(status, expected_status, "route response body: {body}");
            body
        }
        RouteResponse::Binary { .. } => {
            panic!("expected text response with status {expected_status}")
        }
    }
}

fn expect_json_text_route_response(
    response: RouteResponse,
    expected_status: u16,
) -> serde_json::Value {
    let body = expect_text_route_response(response, expected_status);
    serde_json::from_str(&body).expect("json route body")
}

fn expect_json_result_response(
    response: Result<(u16, String)>,
    expected_status: u16,
) -> serde_json::Value {
    let (status, body) = response.expect("route result");
    assert_eq!(status, expected_status);
    serde_json::from_str(&body).expect("json result body")
}

fn start_test_hook_bridge(runtime: &DaemonRuntime, body: &str) -> serde_json::Value {
    expect_json_result_response(
        start_hook_bridge(
            body,
            &runtime.hook_bridge,
            &runtime.mcp_servers,
            &runtime.tool_registry,
            &runtime.workflow_store,
            &runtime.settings,
            &runtime.shared_images,
            &runtime.ocr_provider,
            &runtime.framework_registry,
            &runtime.control_plane_root,
            &runtime.run_store,
            &runtime.surface_instances,
            &runtime.surface_actions,
        ),
        200,
    )
}

fn stop_test_hook_bridge(runtime: &DaemonRuntime) -> serde_json::Value {
    expect_json_result_response(
        stop_hook_bridge(&runtime.hook_bridge, &runtime.shared_images),
        200,
    )
}

fn hook_bridge_status_value(runtime: &DaemonRuntime) -> serde_json::Value {
    expect_json_result_response(hook_bridge_status(&runtime.hook_bridge), 200)
}

fn run_hook_bridge_text(runtime: &DaemonRuntime, request: &str) -> serde_json::Value {
    let workflow_root = runtime
        .hook_bridge
        .lock()
        .expect("lock hook bridge runtime for test")
        .workflow_root
        .clone();
    let result = handle_hook_bridge_websocket_text(
        request,
        &runtime.mcp_servers,
        &runtime.tool_registry,
        &runtime.workflow_store,
        &runtime.settings,
        &runtime.shared_images,
        &runtime.ocr_provider,
        &runtime.framework_registry,
        &runtime.control_plane_root,
        &workflow_root,
        &runtime.run_store,
    );
    serde_json::from_str(&result.response).expect("hook bridge json response")
}

fn run_hook_bridge_text_with_intermediate(
    runtime: &DaemonRuntime,
    request: &str,
) -> (Vec<serde_json::Value>, serde_json::Value) {
    let workflow_root = runtime
        .hook_bridge
        .lock()
        .expect("lock hook bridge runtime for test")
        .workflow_root
        .clone();
    let mut intermediate = Vec::new();
    let result = handle_hook_bridge_websocket_text_with_intermediate(
        request,
        &runtime.mcp_servers,
        &runtime.tool_registry,
        &runtime.workflow_store,
        &runtime.settings,
        &runtime.shared_images,
        &runtime.ocr_provider,
        &runtime.framework_registry,
        &runtime.control_plane_root,
        &workflow_root,
        &runtime.run_store,
        Some(&runtime.surface_actions),
        &mut |message| {
            intermediate
                .push(serde_json::from_str(&message).expect("intermediate hook bridge json"));
        },
    );
    (
        intermediate,
        serde_json::from_str(&result.response).expect("hook bridge json response"),
    )
}

fn read_hook_terminal_response(
    socket: &mut tungstenite::WebSocket<TcpStream>,
    request_id: &str,
) -> Value {
    loop {
        let message = read_hook_bridge_json(socket);
        if message["requestId"] == request_id
            && message.get("status").is_some()
            && message.get("method").is_none()
        {
            return message;
        }
    }
}

fn formal_art_execute_request(
    request_id: &str,
    node_id: &str,
    art_id: &str,
    input: Option<Value>,
    parameters: Value,
) -> String {
    let mut inputs = serde_json::Map::new();
    if let Some(input) = input {
        inputs.insert("input".to_owned(), input);
    }
    json!({
        "method": loom_protocol::HOOK_METHOD_ART_EXECUTE,
        "params": {
            "protocolVersion": loom_protocol::HOOK_PROTOCOL_VERSION,
            "requestId": request_id,
            "nodeId": node_id,
            "artId": art_id,
            "generation": 1,
            "outputTransports": ["shared_memory"],
            "inputs": inputs,
            "parameters": parameters,
            "disabledParameters": [],
        }
    })
    .to_string()
}

fn inline_art_input(data_url: &str) -> Value {
    let (header, data_base64) = data_url
        .strip_prefix("data:")
        .and_then(|value| value.split_once(','))
        .expect("test image data URL");
    let mime = header
        .strip_suffix(";base64")
        .expect("base64 test image data URL");
    json!({
        "kind": "inline_resource",
        "mime": mime,
        "dataBase64": data_base64,
    })
}

fn remove_test_dir(path: &Path) {
    let mut last_error = None;
    for _ in 0..20 {
        match fs::remove_dir_all(path) {
            Ok(()) => return,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(error) => {
                last_error = Some(error);
                thread::sleep(Duration::from_millis(25));
            }
        }
    }
    panic!(
        "cleanup test directory `{}` failed: {}",
        path.display(),
        last_error.expect("cleanup error")
    );
}
