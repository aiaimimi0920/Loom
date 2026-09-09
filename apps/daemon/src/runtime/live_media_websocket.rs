// Authenticated binary WebSocket upgrade and per-connection source/viewer media loops.
const LIVE_MEDIA_SOCKET_TIMEOUT: Duration = Duration::from_millis(250);
const LIVE_VIEWER_CONTROL_TIMEOUT: Duration = Duration::from_millis(5);

fn live_media_websocket_config() -> tungstenite::protocol::WebSocketConfig {
    let max_message_size =
        loom_protocol::LIVE_BINARY_HEADER_LEN.saturating_add(loom_protocol::LIVE_MAX_FRAME_PAYLOAD);
    tungstenite::protocol::WebSocketConfig {
        max_message_size: Some(max_message_size),
        max_frame_size: Some(max_message_size),
        ..Default::default()
    }
}

fn is_live_media_websocket_request(request: &ParsedHttpRequest) -> bool {
    request.method == "GET"
        && request
            .path
            .split('?')
            .next()
            .is_some_and(|path| path == "/v1/live/media")
        && request
            .header("upgrade")
            .is_some_and(|value| value.eq_ignore_ascii_case("websocket"))
}

fn handle_live_media_websocket_upgrade(
    mut stream: TcpStream,
    request: ParsedHttpRequest,
    runtime: &DaemonRuntime,
) {
    let result = prepare_live_media_upgrade(&request, runtime);
    let (device_id, session_id, role, after_epoch, after_frame_id, accept_key) = match result {
        Ok(value) => value,
        Err(error) => {
            if let Ok((status, body)) = live_error_response(error) {
                write_response_safely(stream, status, &body);
            }
            return;
        }
    };
    let Some(connection_permit) = reserve_live_media_connection(&runtime.live_sessions) else {
        let error = LiveRuntimeError::new(
            503,
            "live_media_busy",
            "the live media connection limit has been reached",
        );
        if let Ok((status, body)) = live_error_response(error) {
            write_response_safely(stream, status, &body);
        }
        return;
    };
    let response = format!(
        "HTTP/1.1 101 Switching Protocols\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Accept: {accept_key}\r\nSec-WebSocket-Protocol: {}\r\n\r\n",
        loom_protocol::LIVE_PROTOCOL_VERSION
    );
    if stream.write_all(response.as_bytes()).is_err() || stream.flush().is_err() {
        return;
    }
    let _ = stream.set_read_timeout(Some(LIVE_MEDIA_SOCKET_TIMEOUT));
    let _ = stream.set_write_timeout(Some(LIVE_MEDIA_SOCKET_TIMEOUT));
    let sessions = Arc::clone(&runtime.live_sessions);
    let worker = thread::Builder::new()
        .name(format!("loom-live-media-{role:?}"))
        .spawn(move || {
            let socket = tungstenite::WebSocket::from_raw_socket(
                stream,
                tungstenite::protocol::Role::Server,
                Some(live_media_websocket_config()),
            );
            run_live_media_socket(
                socket,
                sessions,
                session_id,
                device_id,
                role,
                after_epoch,
                after_frame_id,
                connection_permit,
            );
        });
    match worker {
        Ok(worker) => runtime.live_sessions.track_media_worker(worker),
        Err(error) => runtime_log_warn(format!("spawn live media worker failed: {error}")),
    }
}

fn prepare_live_media_upgrade(
    request: &ParsedHttpRequest,
    runtime: &DaemonRuntime,
) -> std::result::Result<(String, String, LiveDeviceRole, u64, u64, String), LiveRuntimeError> {
    enforce_request_security(request).map_err(|(status, body)| {
        let message = serde_json::from_str::<Value>(&body)
            .ok()
            .and_then(|value| {
                value
                    .pointer("/error/message")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| "live media request security validation failed".to_owned());
        LiveRuntimeError::new(status, "live_media_request_denied", message)
    })?;
    validate_live_websocket_headers(request)?;
    let session_id = request
        .query_parameter("sessionId")
        .ok_or_else(|| invalid_live_upgrade("sessionId is required"))?;
    loom_protocol::validate_live_identifier(&session_id, "session_id")
        .map_err(|error| invalid_live_upgrade(error.to_string()))?;
    let role = match request.query_parameter("role").as_deref() {
        Some("source") => LiveDeviceRole::Source,
        Some("viewer") => LiveDeviceRole::Viewer,
        _ => return Err(invalid_live_upgrade("role must be source or viewer")),
    };
    let after_epoch = parse_live_query_u64(request, "afterEpoch")?.unwrap_or(0);
    let after_frame_id = parse_live_query_u64(request, "afterFrameId")?.unwrap_or(0);
    let admin_authenticated = request.has_admin_credential(&runtime.auth_token);
    let authenticated = authenticate_http_device_session(request, &runtime.device_registry)
        .map_err(|error| LiveRuntimeError::new(error.status, error.code, error.message))?;
    let device_id = match (authenticated, admin_authenticated) {
        (Some(device_id), _) => device_id,
        (None, true) => request
            .query_parameter("deviceId")
            .unwrap_or_else(|| "device-000-local".to_owned()),
        (None, false) => {
            return Err(LiveRuntimeError::new(
                401,
                "live_media_unauthorized",
                "a Loom administrator or device session credential is required",
            ))
        }
    };
    loom_protocol::validate_live_identifier(&device_id, "device_id")
        .map_err(|error| invalid_live_upgrade(error.to_string()))?;
    runtime
        .live_sessions
        .authorize_media(&session_id, &device_id, role)?;
    let key = request
        .header("sec-websocket-key")
        .ok_or_else(|| invalid_live_upgrade("Sec-WebSocket-Key is required"))?;
    let accept_key = tungstenite::handshake::derive_accept_key(key.as_bytes());
    Ok((
        device_id,
        session_id,
        role,
        after_epoch,
        after_frame_id,
        accept_key,
    ))
}

fn validate_live_websocket_headers(
    request: &ParsedHttpRequest,
) -> std::result::Result<(), LiveRuntimeError> {
    if request.header_count("upgrade") != 1
        || !request
            .header("upgrade")
            .is_some_and(|value| value.eq_ignore_ascii_case("websocket"))
        || request.header_count("connection") != 1
        || !request.header("connection").is_some_and(|value| {
            value
                .split(',')
                .any(|token| token.trim().eq_ignore_ascii_case("upgrade"))
        })
        || request.header_count("sec-websocket-version") != 1
        || request.header("sec-websocket-version") != Some("13")
        || request.header_count("sec-websocket-key") != 1
        || request.header_count("sec-websocket-protocol") != 1
        || !request
            .header("sec-websocket-protocol")
            .is_some_and(|value| {
                value
                    .split(',')
                    .any(|protocol| protocol.trim() == loom_protocol::LIVE_PROTOCOL_VERSION)
            })
    {
        return Err(invalid_live_upgrade(
            "the WebSocket upgrade headers or loom.live.v1 subprotocol are invalid",
        ));
    }
    Ok(())
}

fn parse_live_query_u64(
    request: &ParsedHttpRequest,
    name: &str,
) -> std::result::Result<Option<u64>, LiveRuntimeError> {
    request
        .query_parameter(name)
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|_| invalid_live_upgrade(format!("{name} must be an unsigned integer")))
        })
        .transpose()
}

fn run_live_media_socket(
    mut socket: tungstenite::WebSocket<TcpStream>,
    sessions: SharedLiveSessionStore,
    session_id: String,
    device_id: String,
    role: LiveDeviceRole,
    after_epoch: u64,
    after_frame_id: u64,
    _connection_permit: LiveMediaConnectionPermit,
) {
    if sessions
        .set_media_connected(&session_id, &device_id, role, true)
        .is_err()
    {
        let _ = socket.close(None);
        return;
    }
    let _role_guard = LiveMediaRoleGuard {
        sessions: Arc::clone(&sessions),
        session_id: session_id.clone(),
        device_id: device_id.clone(),
        role,
    };
    match role {
        LiveDeviceRole::Source => {
            run_live_source_socket(&mut socket, &sessions, &session_id, &device_id)
        }
        LiveDeviceRole::Viewer => run_live_viewer_socket(
            &mut socket,
            &sessions,
            &session_id,
            after_epoch,
            after_frame_id,
        ),
        LiveDeviceRole::Controller => {}
    }
    let _ = socket.close(None);
}

fn run_live_source_socket(
    socket: &mut tungstenite::WebSocket<TcpStream>,
    sessions: &SharedLiveSessionStore,
    session_id: &str,
    device_id: &str,
) {
    while !sessions.media_cancelled.load(Ordering::SeqCst) {
        match socket.read() {
            Ok(tungstenite::Message::Binary(bytes)) => {
                if sessions
                    .publish_frame(session_id, device_id, bytes)
                    .is_err()
                {
                    break;
                }
            }
            Ok(tungstenite::Message::Ping(bytes)) => {
                if socket.send(tungstenite::Message::Pong(bytes)).is_err() {
                    break;
                }
            }
            Ok(tungstenite::Message::Pong(_)) => {}
            Ok(tungstenite::Message::Close(close)) => {
                let _ = socket.close(close);
                break;
            }
            Ok(_) => break,
            Err(error) if hook_bridge_read_timed_out(&error) => {}
            Err(_) => break,
        }
    }
}

fn run_live_viewer_socket(
    socket: &mut tungstenite::WebSocket<TcpStream>,
    sessions: &SharedLiveSessionStore,
    session_id: &str,
    mut after_epoch: u64,
    mut after_frame_id: u64,
) {
    let _ = socket
        .get_mut()
        .set_read_timeout(Some(LIVE_VIEWER_CONTROL_TIMEOUT));
    let mut last_ping = Instant::now();
    while !sessions.media_cancelled.load(Ordering::SeqCst) {
        match sessions.wait_for_frame(
            session_id,
            after_epoch,
            after_frame_id,
            LIVE_MEDIA_SOCKET_TIMEOUT,
        ) {
            Ok(Some(frame)) => {
                if socket
                    .send(tungstenite::Message::Binary(frame.bytes.as_ref().clone()))
                    .is_err()
                {
                    break;
                }
                after_epoch = frame.epoch;
                after_frame_id = frame.frame_id;
                last_ping = Instant::now();
            }
            Ok(None) if last_ping.elapsed() >= Duration::from_secs(2) => {
                if socket.send(tungstenite::Message::Ping(Vec::new())).is_err() {
                    break;
                }
                last_ping = Instant::now();
            }
            Ok(None) => {}
            Err(_) => break,
        }
        if !service_live_viewer_control_messages(socket) {
            break;
        }
    }
}

fn service_live_viewer_control_messages(socket: &mut tungstenite::WebSocket<TcpStream>) -> bool {
    for _ in 0..4 {
        match socket.read() {
            Ok(tungstenite::Message::Ping(bytes)) => {
                if socket.send(tungstenite::Message::Pong(bytes)).is_err() {
                    return false;
                }
            }
            Ok(tungstenite::Message::Pong(_)) => {}
            Ok(tungstenite::Message::Close(close)) => {
                let _ = socket.close(close);
                return false;
            }
            Ok(_) => return false,
            Err(error) if hook_bridge_read_timed_out(&error) => return true,
            Err(_) => return false,
        }
    }
    true
}

struct LiveMediaConnectionPermit {
    active: Arc<AtomicUsize>,
}

impl Drop for LiveMediaConnectionPermit {
    fn drop(&mut self) {
        let _ = self
            .active
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |value| {
                value.checked_sub(1)
            });
    }
}

fn reserve_live_media_connection(
    sessions: &SharedLiveSessionStore,
) -> Option<LiveMediaConnectionPermit> {
    let active = Arc::clone(&sessions.media_connections);
    let mut current = active.load(Ordering::SeqCst);
    loop {
        if current >= LIVE_MEDIA_CONNECTION_LIMIT {
            return None;
        }
        match active.compare_exchange(current, current + 1, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(_) => return Some(LiveMediaConnectionPermit { active }),
            Err(next) => current = next,
        }
    }
}

struct LiveMediaRoleGuard {
    sessions: SharedLiveSessionStore,
    session_id: String,
    device_id: String,
    role: LiveDeviceRole,
}

impl Drop for LiveMediaRoleGuard {
    fn drop(&mut self) {
        let _ =
            self.sessions
                .set_media_connected(&self.session_id, &self.device_id, self.role, false);
    }
}

fn invalid_live_upgrade(message: impl Into<String>) -> LiveRuntimeError {
    LiveRuntimeError::new(400, "live_websocket_invalid", message)
}
