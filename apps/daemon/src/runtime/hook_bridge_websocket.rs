// Hook overlay files, bridge lifecycle, and WebSocket connection ownership.
fn load_hook_live_workflow_document() -> Option<hook_canvas::HookCanvasDocument> {
    let snapshots = hook_live_workflow_snapshots().lock().ok()?;
    let snapshot = snapshots.get(HOOK_LIVE_WORKFLOW_ID)?;
    Some(hook_canvas::HookCanvasDocument::from_serialized_root(
        &snapshot.source_path,
        snapshot.bytes.clone(),
        snapshot.root.clone(),
        snapshot.updated_at.clone(),
    ))
}

fn hook_canvas_overlay_revision(snapshot: &hook_canvas::HookCanvasSnapshot) -> Option<String> {
    let statuses = hook_canvas_runtime_statuses().lock().ok()?;
    let mut tokens = snapshot
        .nodes
        .iter()
        .filter_map(|node| {
            statuses.get(&node.id).map(|state| {
                format!(
                    "{}:{}:{:?}:{:?}:{:?}:{:?}",
                    node.id,
                    state.status,
                    state.error_message,
                    state.preview_cache_token,
                    state.selected_result_index,
                    state
                        .result_candidates
                        .iter()
                        .map(|candidate| (&candidate.index, &candidate.image_url))
                        .collect::<Vec<_>>()
                )
            })
        })
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return None;
    }
    tokens.sort();
    let mut hasher = DefaultHasher::new();
    for token in tokens {
        token.hash(&mut hasher);
    }
    Some(format!("{:016x}", hasher.finish()))
}

fn apply_hook_canvas_runtime_overlays(document: &mut hook_canvas::HookCanvasDocument) {
    let Some(statuses) = hook_canvas_runtime_statuses().lock().ok() else {
        return;
    };
    for node in &mut document.snapshot.nodes {
        let Some(state) = statuses.get(&node.id) else {
            continue;
        };
        node.status = state.status.clone();
        node.error_message = state.error_message.clone();
        node.result_candidates = state.result_candidates.clone();
        node.selected_result_index = state.selected_result_index;
    }
    let preview_overrides = statuses
        .iter()
        .filter_map(|(node_id, state)| {
            state.preview_data_url.as_ref().map(|data_url| {
                (
                    node_id.clone(),
                    data_url.clone(),
                    state.preview_cache_token.clone(),
                )
            })
        })
        .collect::<Vec<_>>();
    drop(statuses);
    for (node_id, data_url, cache_token) in preview_overrides {
        document.override_preview_source(
            &node_id,
            hook_canvas::HookCanvasPreviewSource::DataUrl(data_url),
            cache_token.as_deref(),
        );
    }
    if let Some(overlay_revision) = hook_canvas_overlay_revision(&document.snapshot) {
        document.snapshot.revision =
            format!("{}-rt-{overlay_revision}", document.snapshot.revision);
    }
}

fn load_active_hook_canvas_document() -> Result<hook_canvas::HookCanvasDocument> {
    let mut document = match load_hook_live_workflow_document() {
        Some(document) => document,
        None => hook_canvas::HookCanvasDocument::read(&hook_session_path())?,
    };
    apply_hook_canvas_runtime_overlays(&mut document);
    Ok(document)
}

fn hook_session_path() -> PathBuf {
    // An explicit full-path override wins so isolated smokes and advanced setups
    // can point Loom at a specific session file.
    if let Some(path) = std::env::var_os("LOOM_HOOK_SESSION_PATH") {
        let path = PathBuf::from(path);
        if !path.as_os_str().is_empty() {
            return path;
        }
    }
    let appdata = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    // An explicit identifier-directory override still resolves `session.json`
    // beneath it.
    if let Some(dir) = std::env::var_os("LOOM_HOOK_APPDATA_DIR") {
        let dir = PathBuf::from(dir);
        if !dir.as_os_str().is_empty() {
            return dir.join("session.json");
        }
    }
    resolve_hook_session_path(&appdata)
}

fn resolve_hook_session_path(appdata: &Path) -> PathBuf {
    appdata.join("com.yamiyu.hook").join("session.json")
}

fn start_hook_bridge(
    body: &str,
    hook_bridge: &SharedHookBridgeRuntime,
    mcp_servers: &SharedMcpServerStore,
    tool_registry: &ToolRegistry,
    workflow_store: &WorkflowStore,
    settings: &SharedLoomSettingsStore,
    shared_images: &SharedImageStoreHandle,
    ocr_provider: &OcrProviderHandle,
    framework_registry: &FrameworkRegistry,
    control_plane_root: &Path,
    run_store: &SharedRunStore,
    surface_instances: &SharedSurfaceInstanceStore,
    surface_actions: &SharedSurfaceActionExecutor,
) -> Result<(u16, String)> {
    let request_body = if body.trim().is_empty() { "{}" } else { body };
    let request = match serde_json::from_str::<StartHookBridgeRequest>(request_body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let requested_port = request.port.unwrap_or(HOOK_BRIDGE_PORT);
    let mut runtime = hook_bridge
        .lock()
        .map_err(|_| anyhow::anyhow!("lock hook bridge runtime"))?;
    if runtime.worker.is_some() {
        return structured_error(
            409,
            json!({
                "code": "hook_bridge_running",
                "message": "Hook bridge is already running",
            }),
        );
    }
    clear_hook_canvas_runtime_state(Some(shared_images));

    let listener = match TcpListener::bind(("127.0.0.1", requested_port)) {
        Ok(listener) => listener,
        Err(error) => {
            return structured_error(
                409,
                json!({
                    "code": "hook_bridge_bind_failed",
                    "message": error.to_string(),
                    "port": requested_port,
                }),
            );
        }
    };
    let assigned_port = listener
        .local_addr()
        .context("read hook bridge local address")?
        .port();
    listener
        .set_nonblocking(true)
        .context("set hook bridge listener nonblocking")?;
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let connected_clients = Arc::clone(&runtime.connected_clients);
    connected_clients.store(0, Ordering::SeqCst);
    runtime.broadcast_hub.clear();
    let broadcast_hub = runtime.broadcast_hub.clone();
    let worker_mcp_servers = Arc::clone(mcp_servers);
    let worker_tool_registry = tool_registry.clone();
    let worker_workflow_store = workflow_store.clone();
    let worker_settings = Arc::clone(settings);
    let worker_shared_images = Arc::clone(shared_images);
    let worker_ocr_provider = Arc::clone(ocr_provider);
    let worker_framework_registry = framework_registry.clone();
    let worker_control_plane_root = control_plane_root.to_path_buf();
    let worker_run_store = Arc::clone(run_store);
    let worker_surface_instances = Arc::clone(surface_instances);
    let worker_surface_actions = Arc::clone(surface_actions);
    let workflow_root = runtime.workflow_root.clone();
    let worker = thread::spawn(move || {
        run_hook_bridge_websocket_server(
            listener,
            shutdown_rx,
            connected_clients,
            broadcast_hub,
            worker_mcp_servers,
            worker_tool_registry,
            worker_workflow_store,
            worker_settings,
            worker_shared_images,
            worker_ocr_provider,
            worker_framework_registry,
            worker_control_plane_root,
            workflow_root,
            worker_run_store,
            worker_surface_instances,
            worker_surface_actions,
        );
    });
    runtime.shutdown_tx = Some(shutdown_tx);
    runtime.worker = Some(worker);
    runtime.port = Some(assigned_port);

    Ok((
        200,
        serde_json::to_string(&hook_bridge_status_json(&runtime))?,
    ))
}

fn stop_hook_bridge(
    hook_bridge: &SharedHookBridgeRuntime,
    shared_images: &SharedImageStoreHandle,
) -> Result<(u16, String)> {
    let mut runtime = hook_bridge
        .lock()
        .map_err(|_| anyhow::anyhow!("lock hook bridge runtime"))?;
    if let Some(shutdown_tx) = runtime.shutdown_tx.take() {
        let _ = shutdown_tx.send(());
    }
    if let Some(worker) = runtime.worker.take() {
        let _ = worker.join();
    }
    runtime.connected_clients.store(0, Ordering::SeqCst);
    runtime.broadcast_hub.clear();
    runtime.port = None;
    clear_hook_canvas_runtime_state(Some(shared_images));

    Ok((
        200,
        serde_json::to_string(&hook_bridge_status_json(&runtime))?,
    ))
}

fn hook_bridge_status_json(runtime: &HookBridgeRuntime) -> Value {
    let running = runtime.worker.is_some();
    let events = loom_protocol::HOOK_EVENT_METHODS
        .iter()
        .chain(loom_protocol::SURFACE_EVENT_METHODS.iter())
        .copied()
        .collect::<Vec<_>>();
    json!({
        "running": running,
        "port": runtime.port.unwrap_or(HOOK_BRIDGE_PORT),
        "connectedClients": runtime.connected_clients.load(Ordering::SeqCst),
        "subscribedClients": runtime.broadcast_hub.subscriber_count(),
        "protocol": loom_protocol::HOOK_PROTOCOL_VERSION,
        "methods": loom_protocol::HOOK_REQUEST_METHODS,
        "events": events,
    })
}

fn run_hook_bridge_websocket_server(
    listener: TcpListener,
    shutdown_rx: Receiver<()>,
    connected_clients: Arc<AtomicUsize>,
    broadcast_hub: HookBridgeBroadcastHub,
    mcp_servers: SharedMcpServerStore,
    tool_registry: ToolRegistry,
    workflow_store: WorkflowStore,
    settings: SharedLoomSettingsStore,
    shared_images: SharedImageStoreHandle,
    ocr_provider: OcrProviderHandle,
    framework_registry: FrameworkRegistry,
    control_plane_root: PathBuf,
    workflow_root: PathBuf,
    run_store: SharedRunStore,
    surface_instances: SharedSurfaceInstanceStore,
    surface_actions: SharedSurfaceActionExecutor,
) {
    loop {
        if shutdown_rx.try_recv().is_ok() {
            return;
        }

        match listener.accept() {
            Ok((stream, _)) => {
                let connected_clients = Arc::clone(&connected_clients);
                let broadcast_hub = broadcast_hub.clone();
                let mcp_servers = Arc::clone(&mcp_servers);
                let tool_registry = tool_registry.clone();
                let workflow_store = workflow_store.clone();
                let settings = Arc::clone(&settings);
                let shared_images = Arc::clone(&shared_images);
                let ocr_provider = Arc::clone(&ocr_provider);
                let framework_registry = framework_registry.clone();
                let control_plane_root = control_plane_root.clone();
                let workflow_root = workflow_root.clone();
                let run_store = Arc::clone(&run_store);
                let surface_instances = Arc::clone(&surface_instances);
                let surface_actions = Arc::clone(&surface_actions);
                thread::spawn(move || {
                    handle_hook_bridge_websocket_connection(
                        stream,
                        connected_clients,
                        broadcast_hub,
                        mcp_servers,
                        tool_registry,
                        workflow_store,
                        settings,
                        shared_images,
                        ocr_provider,
                        framework_registry,
                        control_plane_root,
                        workflow_root,
                        run_store,
                        surface_instances,
                        surface_actions,
                    );
                });
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(_) => return,
        }
    }
}

fn handle_hook_bridge_websocket_connection(
    stream: std::net::TcpStream,
    connected_clients: Arc<AtomicUsize>,
    broadcast_hub: HookBridgeBroadcastHub,
    mcp_servers: SharedMcpServerStore,
    tool_registry: ToolRegistry,
    workflow_store: WorkflowStore,
    settings: SharedLoomSettingsStore,
    shared_images: SharedImageStoreHandle,
    ocr_provider: OcrProviderHandle,
    framework_registry: FrameworkRegistry,
    control_plane_root: PathBuf,
    workflow_root: PathBuf,
    run_store: SharedRunStore,
    surface_instances: SharedSurfaceInstanceStore,
    surface_actions: SharedSurfaceActionExecutor,
) {
    let _ = stream.set_nonblocking(false);
    let Ok(mut websocket) = tungstenite::accept(stream) else {
        return;
    };
    let _ = websocket
        .get_mut()
        .set_read_timeout(Some(Duration::from_millis(100)));
    connected_clients.fetch_add(1, Ordering::SeqCst);
    let _guard = ConnectedClientGuard { connected_clients };
    let mut subscription_rx: Option<Receiver<String>> = None;
    let mut _subscription_guard: Option<HookBridgeSubscriptionGuard> = None;

    loop {
        if let Some(rx) = &subscription_rx {
            if !drain_hook_bridge_broadcasts(&mut websocket, rx) {
                break;
            }
        }

        let message = match websocket.read() {
            Ok(message) => message,
            Err(error) if hook_bridge_read_timed_out(&error) => continue,
            Err(_) => break,
        };
        match message {
            tungstenite::Message::Text(text) => {
                let mut intermediate_send_failed = false;
                let mut recovery_channels: Option<Vec<String>> = None;
                let mut emit_intermediate = |message: String| {
                    if websocket.send(tungstenite::Message::Text(message)).is_err() {
                        intermediate_send_failed = true;
                    }
                };
                let result = handle_hook_bridge_websocket_text_with_intermediate(
                    &text,
                    &mcp_servers,
                    &tool_registry,
                    &workflow_store,
                    &settings,
                    &shared_images,
                    &ocr_provider,
                    &framework_registry,
                    &control_plane_root,
                    &workflow_root,
                    &run_store,
                    Some(&surface_actions),
                    &mut emit_intermediate,
                );
                if intermediate_send_failed {
                    break;
                }
                if result.subscription_channels.is_some() && subscription_rx.is_none() {
                    recovery_channels = result.subscription_channels.clone();
                    let (rx, guard) = register_hook_bridge_subscription(
                        &broadcast_hub,
                        result.subscription_channels.clone().unwrap_or_default(),
                    );
                    subscription_rx = Some(rx);
                    _subscription_guard = Some(guard);
                }
                if websocket
                    .send(tungstenite::Message::Text(result.response))
                    .is_err()
                {
                    break;
                }
                if recovery_channels.as_ref().is_some_and(|channels| {
                    channels
                        .iter()
                        .any(|channel| channel_accepts_method(channel, SURFACE_EVENT_SNAPSHOT))
                }) {
                    let recovery = surface_snapshot_recovery_messages(&surface_instances);
                    for message in recovery {
                        if websocket.send(tungstenite::Message::Text(message)).is_err() {
                            return;
                        }
                    }
                }
                broadcast_hook_bridge_messages(&broadcast_hub, &result.broadcasts);
            }
            tungstenite::Message::Ping(data) => {
                let _ = websocket.send(tungstenite::Message::Pong(data));
            }
            tungstenite::Message::Close(close) => {
                let _ = websocket.close(close);
                break;
            }
            _ => {}
        }
    }
}
