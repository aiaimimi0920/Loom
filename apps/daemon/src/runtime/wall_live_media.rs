// Wall viewers reuse the source frame buffer without becoming Surface viewers/controllers.
struct WallLiveGrant {
    endpoint_id: String,
    lease_id: String,
    revision: u64,
    session_id: String,
    device_id: String,
    token_hash: String,
    profile: WallMediaProfile,
}

impl LiveSessionStore {
    fn wall_source_active(&self, session_id: &str) -> bool {
        self.state.lock().ok().is_some_and(|sessions| {
            sessions.get(session_id).is_some_and(|record| {
                !record.closed
                    && record.source_connected
                    && record
                        .frames
                        .back()
                        .is_none_or(|frame| frame.received_at.elapsed() < Duration::from_secs(5))
            })
        })
    }
}

impl WallLiveGrant {
    fn authorize(
        &self,
        walls: &SharedWallStore,
        devices: &SharedDeviceRegistryStore,
    ) -> std::result::Result<(), LiveRuntimeError> {
        // The handshake nonce is consumed once. Revalidation never creates new request nonces.
        let valid_device = devices.lock().ok().is_some_and(|store| {
            store.sessions.get(&self.token_hash).is_some_and(|session| {
                session.device_id == self.device_id
                    && session.expires_at_ms > unix_time_millis()
                    && store.devices.get(&self.device_id).is_some_and(|device| {
                        device.enabled
                            && device.approval == "approved"
                            && device.session_epoch == session.session_epoch
                    })
            })
        });
        if !valid_device {
            return Err(LiveRuntimeError::new(
                403,
                "wall_live_denied",
                "paired device session is no longer valid",
            ));
        }
        let endpoint = walls
            .authorize_live(
                &self.endpoint_id,
                &self.device_id,
                &self.lease_id,
                self.revision,
                &self.session_id,
            )
            .map_err(|error| LiveRuntimeError::new(error.status, error.code, error.message))?;
        if !self.profile.admitted(&endpoint) {
            return Err(LiveRuntimeError::new(
                409,
                "wall_live_codec_unavailable",
                "requested media format is not advertised by this endpoint",
            ));
        }
        Ok(())
    }
}

fn prepare_wall_live_upgrade(
    request: &ParsedHttpRequest,
    runtime: &DaemonRuntime,
) -> std::result::Result<(WallLiveGrant, String), LiveRuntimeError> {
    enforce_request_security(request).map_err(|(status, _)| {
        LiveRuntimeError::new(
            status,
            "wall_live_denied",
            "request security validation failed",
        )
    })?;
    validate_live_websocket_headers(request, WALL_MEDIA_PROTOCOL_VERSION)?;
    let device_id = authenticate_http_device_session(request, &runtime.device_registry)
        .map_err(|error| LiveRuntimeError::new(error.status, error.code, error.message))?
        .ok_or_else(|| {
            LiveRuntimeError::new(
                401,
                "wall_live_pairing_required",
                "a paired device session is required",
            )
        })?;
    let identifier = |name: &'static str| -> std::result::Result<String, LiveRuntimeError> {
        let value = request
            .query_parameter(name)
            .ok_or_else(|| invalid_live_upgrade(format!("{name} is required")))?;
        loom_protocol::validate_live_identifier(&value, name)
            .map_err(|error| invalid_live_upgrade(error.to_string()))?;
        Ok(value)
    };
    let grant = WallLiveGrant {
        endpoint_id: identifier("endpointId")?,
        lease_id: identifier("leaseId")?,
        session_id: identifier("sessionId")?,
        revision: parse_live_query_u64(request, "revision")?
            .ok_or_else(|| invalid_live_upgrade("revision is required"))?,
        device_id,
        token_hash: sha256_bytes(
            request
                .authorization_credential("Device")
                .unwrap_or_default()
                .as_bytes(),
        ),
        profile: WallMediaProfile::read(request)?,
    };
    grant.authorize(&runtime.walls, &runtime.device_registry)?;
    let session = runtime.live_sessions.get(&grant.session_id).map_err(|_| {
        LiveRuntimeError::new(
            404,
            "wall_live_source_missing",
            "source must republish after restart",
        )
    })?;
    if session.closed {
        return Err(LiveRuntimeError::new(
            410,
            "wall_live_source_closed",
            "source session has been explicitly closed",
        ));
    }
    if !runtime.live_sessions.wall_source_active(&grant.session_id) {
        return Err(LiveRuntimeError::new(
            409,
            "wall_live_source_unavailable",
            "Live source is disconnected or stalled",
        ));
    }
    let key = request
        .header("sec-websocket-key")
        .ok_or_else(|| invalid_live_upgrade("key is required"))?;
    Ok((
        grant,
        tungstenite::handshake::derive_accept_key(key.as_bytes()),
    ))
}

fn handle_wall_live_upgrade(
    mut stream: TcpStream,
    request: ParsedHttpRequest,
    runtime: &DaemonRuntime,
) {
    let (grant, accept_key) = match prepare_wall_live_upgrade(&request, runtime) {
        Ok(value) => value,
        Err(error) => {
            if let Ok((status, body)) = live_error_response(error) {
                write_response_safely(stream, status, &body);
            }
            return;
        }
    };
    let Some(permit) = reserve_live_media_connection(&runtime.live_sessions) else {
        if let Ok((status, body)) = live_error_response(LiveRuntimeError::new(
            503,
            "live_media_busy",
            "media connection limit reached",
        )) {
            write_response_safely(stream, status, &body);
        }
        return;
    };
    // Upgrade and subsequent writes have the same bounded socket deadline.
    let _ = stream.set_read_timeout(Some(LIVE_VIEWER_CONTROL_TIMEOUT));
    let _ = stream.set_write_timeout(Some(LIVE_MEDIA_SOCKET_TIMEOUT));
    let response = format!("HTTP/1.1 101 Switching Protocols\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Accept: {accept_key}\r\nSec-WebSocket-Protocol: {WALL_MEDIA_PROTOCOL_VERSION}\r\n\r\n");
    if stream.write_all(response.as_bytes()).is_err() || stream.flush().is_err() {
        return;
    }
    let sessions = Arc::clone(&runtime.live_sessions);
    let walls = Arc::clone(&runtime.walls);
    let devices = Arc::clone(&runtime.device_registry);
    let worker = thread::Builder::new()
        .name("loom-wall-live".into())
        .spawn(move || {
            let _permit = permit;
            // Viewers may send only liveness control frames, never a media payload.
            let config = tungstenite::protocol::WebSocketConfig {
                max_message_size: Some(125),
                max_frame_size: Some(125),
                ..Default::default()
            };
            let mut socket = tungstenite::WebSocket::from_raw_socket(
                stream,
                tungstenite::protocol::Role::Server,
                Some(config),
            );
            run_wall_live_socket(&mut socket, &sessions, &walls, &devices, &grant);
            let _ = socket.close(None);
        });
    match worker {
        Ok(worker) => runtime.live_sessions.track_media_worker(worker),
        Err(error) => runtime_log_warn(format!("spawn wall media worker failed: {error}")),
    }
}

fn run_wall_live_socket(
    socket: &mut tungstenite::WebSocket<TcpStream>,
    sessions: &SharedLiveSessionStore,
    walls: &SharedWallStore,
    devices: &SharedDeviceRegistryStore,
    grant: &WallLiveGrant,
) {
    let (mut epoch, mut frame_id) = (0, 0);
    let mut last_ping = Instant::now();
    let mut next_frame_at = Instant::now();
    while !sessions.media_cancelled.load(Ordering::SeqCst)
        && grant.authorize(walls, devices).is_ok()
        && sessions.wall_source_active(&grant.session_id)
    {
        // Coalesce to the newest source frame after a bounded wait; never queue per viewer.
        if let Some(delay) = next_frame_at.checked_duration_since(Instant::now()) {
            thread::sleep(delay.min(Duration::from_millis(100)));
            continue;
        }
        let frame = sessions.wait_for_frame(
            &grant.session_id,
            epoch,
            frame_id,
            LIVE_MEDIA_SOCKET_TIMEOUT,
        );
        // Recheck after the wait. No catalog/registry lock is held during network I/O.
        if grant.authorize(walls, devices).is_err()
            || !sessions.wall_source_active(&grant.session_id)
        {
            break;
        }
        match frame {
            Ok(Some(frame)) => {
                epoch = frame.epoch;
                frame_id = frame.frame_id;
                next_frame_at =
                    Instant::now() + Duration::from_micros(1_000_000 / grant.profile.fps);
                match encode_wall_media_frame(&frame, walls, grant.profile) {
                    Ok(Some(bytes)) => {
                        // Encoding holds no registry lock; revoke before sending if ownership changed.
                        if grant.authorize(walls, devices).is_err()
                            || !sessions.wall_source_active(&grant.session_id)
                        {
                            break;
                        }
                        if socket.send(tungstenite::Message::Binary(bytes)).is_err() {
                            break;
                        }
                        last_ping = Instant::now();
                    }
                    Ok(None) => {}
                    Err(code) => {
                        let _ = socket.close(Some(tungstenite::protocol::CloseFrame {
                            code: tungstenite::protocol::frame::coding::CloseCode::Unsupported,
                            reason: code.into(),
                        }));
                        break;
                    }
                }
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
        // Wall media accepts only WebSocket liveness messages, never input or publication.
        if !service_live_viewer_control_messages(socket) {
            break;
        }
    }
}
