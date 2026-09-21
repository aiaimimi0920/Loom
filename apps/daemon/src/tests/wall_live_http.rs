type WallTestSocket = tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>;

fn wall_socket(
    port: u16,
    credential: &str,
    lease: &str,
    revision: u64,
) -> std::result::Result<WallTestSocket, tungstenite::Error> {
    wall_socket_profile(port, credential, lease, revision, "raw_bgra")
}

fn wall_socket_profile(
    port: u16,
    credential: &str,
    lease: &str,
    revision: u64,
    format: &str,
) -> std::result::Result<WallTestSocket, tungstenite::Error> {
    use tungstenite::{client::IntoClientRequest, http::HeaderValue};
    let mut request = format!("ws://127.0.0.1:{port}/v1/walls/live/media?endpointId=endpoint-left&leaseId={lease}&revision={revision}&sessionId=live-1&format={format}")
        .into_client_request().unwrap();
    request
        .headers_mut()
        .insert("authorization", HeaderValue::from_str(credential).unwrap());
    request.headers_mut().insert(
        "x-loom-device-nonce",
        HeaderValue::from_str(&Uuid::new_v4().to_string()).unwrap(),
    );
    request.headers_mut().insert(
        "sec-websocket-protocol",
        HeaderValue::from_static("loom.wall.media.v1"),
    );
    let (mut socket, response) = tungstenite::connect(request)?;
    assert_eq!(
        response.headers()["sec-websocket-protocol"],
        "loom.wall.media.v1"
    );
    if let tungstenite::stream::MaybeTlsStream::Plain(tcp) = socket.get_mut() {
        tcp.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    }
    Ok(socket)
}

fn assert_wall_raw(bytes: &[u8], frame_id: u64) -> WallMediaMetadata {
    let frame = WallMediaFrame::decode(bytes).unwrap();
    let expected = encoded_live_frame(frame_id);
    let source = LiveBinaryFrame::decode(&expected).unwrap();
    assert_eq!(
        (frame.metadata.epoch, frame.metadata.frame_id),
        (source.epoch, frame_id)
    );
    assert_eq!(frame.metadata.codec, WallMediaCodec::RawBgra);
    assert_eq!(frame.payload, &expected[64..]);
    assert!(frame.metadata.sent_timestamp_ms >= frame.metadata.received_timestamp_ms);
    frame.metadata
}

fn wall_binary(socket: &mut WallTestSocket) -> Vec<u8> {
    loop {
        match socket.read().unwrap() {
            tungstenite::Message::Binary(bytes) => return bytes,
            tungstenite::Message::Ping(bytes) => {
                socket.send(tungstenite::Message::Pong(bytes)).unwrap()
            }
            _ => panic!("expected binary frame"),
        }
    }
}

fn wall_closed(socket: &mut WallTestSocket) {
    loop {
        match socket.read() {
            Ok(tungstenite::Message::Close(_)) | Err(tungstenite::Error::ConnectionClosed) => break,
            Ok(tungstenite::Message::Ping(bytes)) => {
                let _ = socket.send(tungstenite::Message::Pong(bytes));
            }
            _ => panic!("revoked wall media must close without more frames"),
        }
    }
}

#[test]
fn wall_live_binary_grant_revalidates_without_minting_surface_membership() {
    let root = Root::new();
    let daemon =
        LoomDaemon::bind(DaemonConfig::localhost(0).with_control_plane_root(&root.0)).unwrap();
    let port = daemon.local_addr().unwrap().port();
    let sessions = Arc::clone(&daemon.runtime.live_sessions);
    let registry = Arc::clone(&daemon.runtime.device_registry);
    let (tx, rx) = mpsc::channel();
    let worker = thread::spawn(move || daemon.serve_until(rx));
    let mut server = ConcurrencyTestFixture::new(tx, worker);
    let (owner, token) = pair(port, "Live output");
    let (_, outsider) = pair(port, "Unassigned Live output");
    let mut start = live_start_envelope("live-1", "wall-live-test");
    let LiveControlMessage::SessionStart(message) = &mut start.message else {
        panic!("start fixture")
    };
    message.session.source_device_id = owner.clone();
    message.requested_by_device_id = owner.clone();
    sessions.create(&owner, start).unwrap();
    sessions
        .set_media_connected("live-1", &owner, LiveDeviceRole::Source, true)
        .unwrap();
    sessions
        .publish_frame("live-1", &owner, encoded_live_frame(1))
        .unwrap();
    let mut fixture: Value = serde_json::from_str(include_str!(
        "../../../../protocol/fixtures/wall-geometry.v1.json"
    ))
    .unwrap();
    fixture["endpoints"][0]["deviceId"] = json!(owner);
    assert_eq!(
        device(
            port,
            &token,
            "POST",
            "/v1/walls/endpoints/register",
            Some(json!({"baseRevision": 0, "endpoint": fixture["endpoints"][0]}))
        )
        .0,
        200
    );
    let mut layout = fixture["layout"].clone();
    layout["revision"] = json!(2);
    layout["tiles"] = json!([fixture["layout"]["tiles"][0]]);
    assert_eq!(
        admin(
            port,
            "PUT",
            "/v1/walls/layouts",
            Some(json!({"baseRevision": 1, "layout": layout}))
        )
        .0,
        200
    );
    let (_, lease) = device(
        port,
        &token,
        "POST",
        "/v1/walls/connect",
        Some(json!({"endpointId": "endpoint-left"})),
    );
    let lease = lease["leaseId"].as_str().unwrap();
    let authorization = format!("Device {token}");
    for (credential, revision, expected) in [
        (format!("Device {outsider}"), 2, 403),
        (authorization.clone(), 1, 409),
        ("Bearer loom-local-token".into(), 2, 401),
    ] {
        match wall_socket(port, &credential, lease, revision) {
            Err(tungstenite::Error::Http(response)) => {
                assert_eq!(response.status().as_u16(), expected)
            }
            _ => panic!("invalid wall grant must fail before upgrade"),
        }
    }
    let mut viewer = wall_socket(port, &authorization, lease, 2).unwrap();
    assert_wall_raw(&wall_binary(&mut viewer), 1);
    sessions
        .publish_frame("live-1", &owner, encoded_live_frame(2))
        .unwrap();
    let raw = assert_wall_raw(&wall_binary(&mut viewer), 2);
    let mut compressed = wall_socket_profile(port, &authorization, lease, 2, "png").unwrap();
    let bytes = wall_binary(&mut compressed);
    let png = WallMediaFrame::decode(&bytes).unwrap();
    assert_eq!(png.metadata.codec, WallMediaCodec::Png);
    assert_eq!(
        png.metadata.received_timestamp_ms,
        raw.received_timestamp_ms
    );
    let decoded = image::load_from_memory_with_format(png.payload, image::ImageFormat::Png)
        .unwrap()
        .to_rgb8();
    assert_eq!(
        (decoded.width(), decoded.height()),
        (png.metadata.width, png.metadata.height)
    );
    let _ = compressed.close(None);
    let state = sessions.get("live-1").unwrap();
    assert!(state.session.viewer_devices.is_empty());
    assert!(state.session.controller_device.is_none());
    assert!(state.viewer_connections.is_empty());
    assert_eq!(sessions.list().unwrap().len(), 1);
    // Reassigning a layout invalidates already-open media, including when no new frames arrive.
    layout["revision"] = json!(3);
    layout["placements"][0]["rect"]["x"] = json!(0);
    layout["placements"][0]["rect"]["width"] = json!(100);
    assert_eq!(
        admin(
            port,
            "PUT",
            "/v1/walls/layouts",
            Some(json!({"baseRevision": 2, "layout": layout}))
        )
        .0,
        200
    );
    wall_closed(&mut viewer);
    assert!(wall_socket(port, &authorization, lease, 3).is_err());
    layout["revision"] = json!(4);
    layout["placements"][0]["rect"]["x"] = json!(-100);
    assert_eq!(
        admin(
            port,
            "PUT",
            "/v1/walls/layouts",
            Some(json!({"baseRevision": 3, "layout": layout}))
        )
        .0,
        200
    );
    let mut viewer = wall_socket(port, &authorization, lease, 4).unwrap();
    assert_wall_raw(&wall_binary(&mut viewer), 2);
    sessions
        .set_media_connected("live-1", &owner, LiveDeviceRole::Source, false)
        .unwrap();
    wall_closed(&mut viewer);
    assert!(wall_socket(port, &authorization, lease, 4).is_err());
    sessions
        .set_media_connected("live-1", &owner, LiveDeviceRole::Source, true)
        .unwrap();
    // A retry must not resurrect a buffered old frame as a newly received one.
    sessions
        .state
        .lock()
        .unwrap()
        .get_mut("live-1")
        .unwrap()
        .frames
        .back_mut()
        .unwrap()
        .received_at = Instant::now() - Duration::from_secs(6);
    assert!(wall_socket(port, &authorization, lease, 4).is_err());
    sessions
        .publish_frame("live-1", &owner, encoded_live_frame(3))
        .unwrap();
    let mut viewer = wall_socket(port, &authorization, lease, 4).unwrap();
    assert_wall_raw(&wall_binary(&mut viewer), 3);
    registry.lock().unwrap().revoke_device_sessions(&owner);
    wall_closed(&mut viewer);
    assert!(wall_socket(port, &authorization, lease, 4).is_err());
    server.finish().unwrap();
}
