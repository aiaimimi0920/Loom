// Phase 4 live relay state, authority, recovery, and bounded-media contracts.
fn live_start_envelope(session_id: &str, nonce: &str) -> LiveControlEnvelope {
    let now = unix_time_millis();
    LiveControlEnvelope {
        protocol_version: loom_protocol::LIVE_PROTOCOL_VERSION.to_owned(),
        session_id: session_id.to_owned(),
        epoch: 1,
        sequence: 1,
        message: LiveControlMessage::SessionStart(loom_protocol::LiveSessionStart {
            session: LiveScreenshotSession {
                protocol_version: loom_protocol::LIVE_PROTOCOL_VERSION.to_owned(),
                session_id: session_id.to_owned(),
                source_device_id: "device-source".to_owned(),
                source_hook_id: "hook-node:source".to_owned(),
                source_kind: loom_protocol::LiveSourceKind::Window,
                source_window_identity: loom_protocol::LiveWindowIdentity {
                    window_id: "window:fixture".to_owned(),
                    process_id: 42,
                    process_started_at_ms: Some(now.saturating_sub(1)),
                    title: Some("Live fixture".to_owned()),
                },
                source_region: loom_protocol::LiveRect {
                    x: 0,
                    y: 0,
                    width: 2,
                    height: 2,
                },
                region_anchor: loom_protocol::LiveRegionAnchor::Window,
                frame_stream: loom_protocol::LiveFrameStreamDescriptor {
                    stream_id: format!("stream:{session_id}"),
                    transport: loom_protocol::LiveMediaTransport::WebsocketBinary,
                    endpoint: Some("/v1/live/media".to_owned()),
                    codec: loom_protocol::LiveCodec::RawBgra,
                    color_space: loom_protocol::LiveColorSpace::Srgb,
                    width: 2,
                    height: 2,
                    target_fps: 12,
                    max_buffered_frames: 2,
                    keyframe_interval: 1,
                },
                interaction_capabilities: Vec::new(),
                observation_capabilities: Vec::new(),
                trigger_bindings: Vec::new(),
                viewer_devices: Vec::new(),
                controller_device: None,
                visibility_state: LiveVisibilityState::Visible,
                capture_strategy: loom_protocol::LiveCaptureStrategy::PersistentWindowWgc,
                render_preservation_strategy:
                    loom_protocol::LiveRenderPreservationStrategy::Visible,
                revision: 1,
                created_at_ms: now,
                last_seen_at_ms: now,
            },
            requested_by_device_id: "device-source".to_owned(),
            request_nonce: nonce.to_owned(),
        }),
    }
}

fn live_viewer_envelope(session_id: &str, viewer: &str) -> LiveControlEnvelope {
    LiveControlEnvelope {
        protocol_version: loom_protocol::LIVE_PROTOCOL_VERSION.to_owned(),
        session_id: session_id.to_owned(),
        epoch: 1,
        sequence: 1,
        message: LiveControlMessage::SessionAck(loom_protocol::LiveSessionAck {
            accepted: true,
            reason: None,
            responder_device_id: viewer.to_owned(),
        }),
    }
}

fn encoded_live_frame(id: u64) -> Vec<u8> {
    LiveBinaryFrame {
        epoch: 1,
        metadata: loom_protocol::LiveFrameMetadata {
            frame_id: id,
            capture_timestamp_ms: id,
            encode_timestamp_ms: id,
            width: 2,
            height: 2,
            keyframe: true,
            dropped_frames: 0,
            color_space: loom_protocol::LiveColorSpace::Srgb,
            codec: loom_protocol::LiveCodec::RawBgra,
        },
        payload: vec![id as u8; 16],
    }
    .encode()
    .expect("encode live fixture frame")
}

#[test]
fn live_session_creation_is_idempotent_and_media_ring_is_bounded() {
    let store = LiveSessionStore::new();
    let start = live_start_envelope("live:fixture", "nonce:fixture");
    let (created, first) = store
        .create("device-source", start.clone())
        .expect("create live session");
    assert!(created);
    let (created, duplicate) = store
        .create("device-source", start)
        .expect("repeat identical session start");
    assert!(!created);
    assert_eq!(duplicate.session.session_id, first.session.session_id);

    for id in 1..=4 {
        store
            .publish_frame("live:fixture", "device-source", encoded_live_frame(id))
            .expect("publish ordered frame");
    }
    let snapshot = store.get("live:fixture").expect("live snapshot");
    assert_eq!(snapshot.buffered_frames, 2);
    assert_eq!(snapshot.last_frame_id, 4);
    assert_eq!(snapshot.published_frames, 4);
    assert_eq!(snapshot.relay_dropped_frames, 2);
    assert_eq!(
        store
            .wait_for_frame("live:fixture", 1, 1, Duration::ZERO)
            .expect("recover latest frame")
            .expect("latest frame")
            .frame_id,
        4
    );
    let stale = store
        .publish_frame("live:fixture", "device-source", encoded_live_frame(4))
        .expect_err("stale frame must fail closed");
    assert_eq!(stale.code, "live_frame_sequence_invalid");
}

#[test]
fn live_media_rejects_an_invalid_internal_buffer_capacity_without_mutation() {
    let store = LiveSessionStore::new();
    let mut start = live_start_envelope("live:invalid-buffer", "nonce:invalid-buffer");
    let LiveControlMessage::SessionStart(message) = &mut start.message else {
        panic!("fixture must contain session_start");
    };
    message.session.frame_stream.max_buffered_frames = 0;
    store
        .create("device-source", start)
        .expect("create deliberately invalid internal fixture");

    let error = store
        .publish_frame(
            "live:invalid-buffer",
            "device-source",
            encoded_live_frame(1),
        )
        .expect_err("invalid capacity must fail closed");
    assert_eq!(error.code, "live_frame_buffer_invalid");
    let snapshot = store
        .get("live:invalid-buffer")
        .expect("read unchanged session");
    assert_eq!(snapshot.last_frame_id, 0);
    assert_eq!(snapshot.published_frames, 0);
    assert_eq!(snapshot.buffered_frames, 0);
}

#[test]
fn live_viewer_resume_does_not_duplicate_session_and_controller_is_single() {
    let store = LiveSessionStore::new();
    store
        .create(
            "device-source",
            live_start_envelope("live:authority", "nonce:authority"),
        )
        .expect("create live session");
    for viewer in ["device-viewer-a", "device-viewer-b"] {
        store
            .attach_viewer(viewer, live_viewer_envelope("live:authority", viewer))
            .expect("attach viewer");
    }
    let acquire_a = LiveControlLeaseRequest {
        protocol_version: loom_protocol::LIVE_PROTOCOL_VERSION.to_owned(),
        surface_instance_id: "surface:fixture".to_owned(),
        attachment_id: "attachment:a".to_owned(),
        action: LiveControlLeaseAction::Acquire,
        sequence: 2,
        epoch: 1,
        lease_duration_ms: Some(30_000),
    };
    store
        .change_controller("device-viewer-a", "live:authority", &acquire_a)
        .expect("first viewer acquires controller");
    let acquire_b = LiveControlLeaseRequest {
        attachment_id: "attachment:b".to_owned(),
        ..acquire_a
    };
    let conflict = store
        .change_controller("device-viewer-b", "live:authority", &acquire_b)
        .expect_err("second controller must be rejected");
    assert_eq!(conflict.code, "live_controller_conflict");
    let snapshot = store.get("live:authority").expect("authority snapshot");
    assert_eq!(snapshot.session.viewer_devices.len(), 2);
    assert_eq!(
        snapshot.session.controller_device.as_deref(),
        Some("device-viewer-a")
    );

    let resume = LiveControlEnvelope {
        protocol_version: loom_protocol::LIVE_PROTOCOL_VERSION.to_owned(),
        session_id: "live:authority".to_owned(),
        epoch: 1,
        sequence: 2,
        message: LiveControlMessage::ResumeRequest(loom_protocol::LiveResumeRequest {
            last_control_sequence: 2,
            last_frame_id: 0,
            last_input_sequence: 0,
            requester_device_id: "device-viewer-b".to_owned(),
        }),
    };
    let resumed = store
        .resume("device-viewer-b", resume)
        .expect("resume existing viewer");
    assert_eq!(resumed.session.viewer_devices.len(), 2);
    assert_eq!(store.list().expect("list sessions").len(), 1);
}

#[test]
fn live_control_events_are_separate_from_surface_stream_media() {
    let store = LiveSessionStore::new();
    store
        .create(
            "device-source",
            live_start_envelope("live:events", "nonce:events"),
        )
        .expect("create live session");
    store
        .attach_viewer(
            "device-viewer",
            live_viewer_envelope("live:events", "device-viewer"),
        )
        .expect("attach viewer");
    store
        .publish_frame("live:events", "device-source", encoded_live_frame(1))
        .expect("publish binary media");
    let (_, events) = store.events_after("live:events", 0).expect("read events");
    assert_eq!(events.len(), 2);
    let serialized = serde_json::to_vec(&events).expect("serialize control events");
    assert!(!serialized.windows(4).any(|window| window == b"NLLV"));
    let status = store.status();
    assert_eq!(
        status.protocol_version,
        loom_protocol::LIVE_PROTOCOL_VERSION
    );
    assert_eq!(status.buffered_frames, 1);
}

#[test]
fn live_media_websocket_config_accepts_at_most_one_protocol_frame() {
    let config = live_media_websocket_config();
    let expected = loom_protocol::LIVE_BINARY_HEADER_LEN + loom_protocol::LIVE_MAX_FRAME_PAYLOAD;
    assert_eq!(config.max_message_size, Some(expected));
    assert_eq!(config.max_frame_size, Some(expected));
    assert!(!config.accept_unmasked_frames);
}

#[test]
fn live_media_websocket_fans_out_and_resumes_without_duplicate_frames() {
    use tungstenite::client::IntoClientRequest;
    use tungstenite::http::{header::AUTHORIZATION, header::SEC_WEBSOCKET_PROTOCOL, HeaderValue};

    let root = unique_temp_dir("live-media-websocket");
    let daemon = LoomDaemon::bind(
        DaemonConfig::localhost(0)
            .with_bearer_token("live-media-test-token")
            .with_control_plane_root(&root),
    )
    .expect("bind live media daemon");
    let port = daemon.local_addr().expect("live daemon address").port();
    daemon
        .runtime
        .live_sessions
        .create(
            "device-source",
            live_start_envelope("live:websocket", "nonce:websocket"),
        )
        .expect("create WebSocket session");
    for viewer in ["device-viewer-a", "device-viewer-b"] {
        daemon
            .runtime
            .live_sessions
            .attach_viewer(viewer, live_viewer_envelope("live:websocket", viewer))
            .expect("attach WebSocket viewer");
    }
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve live daemon"));

    let connect = |role: &str, device_id: &str, after_frame_id: u64| {
        let url = format!(
            "ws://127.0.0.1:{port}/v1/live/media?sessionId=live%3Awebsocket&role={role}&afterEpoch=1&afterFrameId={after_frame_id}&deviceId={device_id}"
        );
        let mut request = url.into_client_request().expect("live WebSocket request");
        request.headers_mut().insert(
            AUTHORIZATION,
            HeaderValue::from_static("Bearer live-media-test-token"),
        );
        request.headers_mut().insert(
            SEC_WEBSOCKET_PROTOCOL,
            HeaderValue::from_static(loom_protocol::LIVE_PROTOCOL_VERSION),
        );
        let (mut socket, response) = tungstenite::connect(request).expect("connect live WebSocket");
        assert_eq!(
            response
                .headers()
                .get(SEC_WEBSOCKET_PROTOCOL)
                .and_then(|value| value.to_str().ok()),
            Some(loom_protocol::LIVE_PROTOCOL_VERSION)
        );
        if let tungstenite::stream::MaybeTlsStream::Plain(stream) = socket.get_mut() {
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .expect("set live client timeout");
        }
        socket
    };
    let read_binary = |socket: &mut tungstenite::WebSocket<_>| loop {
        match socket.read().expect("read relayed live message") {
            tungstenite::Message::Binary(bytes) => break bytes,
            tungstenite::Message::Ping(bytes) => socket
                .send(tungstenite::Message::Pong(bytes))
                .expect("answer live ping"),
            message => panic!("unexpected live WebSocket message: {message:?}"),
        }
    };

    let mut source = connect("source", "device-source", 0);
    let mut viewer_a = connect("viewer", "device-viewer-a", 0);
    let mut viewer_b = connect("viewer", "device-viewer-b", 0);
    let latency_probe = b"phase-nine-rtt".to_vec();
    viewer_a
        .send(tungstenite::Message::Ping(latency_probe.clone()))
        .expect("send viewer latency probe");
    loop {
        match viewer_a.read().expect("read viewer latency response") {
            tungstenite::Message::Pong(bytes) if bytes == latency_probe => break,
            tungstenite::Message::Ping(bytes) => viewer_a
                .send(tungstenite::Message::Pong(bytes))
                .expect("answer server latency probe"),
            message => panic!("unexpected latency response: {message:?}"),
        }
    }
    source
        .send(tungstenite::Message::Binary(encoded_live_frame(1)))
        .expect("publish first live frame");
    for viewer in [&mut viewer_a, &mut viewer_b] {
        let decoded =
            LiveBinaryFrame::decode(&read_binary(viewer)).expect("decode fanned-out frame");
        assert_eq!(decoded.metadata.frame_id, 1);
    }

    viewer_b.close(None).expect("close viewer before resume");
    drop(viewer_b);
    let mut resumed_b = connect("viewer", "device-viewer-b", 1);
    source
        .send(tungstenite::Message::Binary(encoded_live_frame(2)))
        .expect("publish resumed live frame");
    let decoded =
        LiveBinaryFrame::decode(&read_binary(&mut resumed_b)).expect("decode resumed frame");
    assert_eq!(decoded.metadata.frame_id, 2);
    assert_eq!(
        daemon_live_session_count_for_test(port, "live-media-test-token"),
        1
    );

    let _ = source.close(None);
    let _ = viewer_a.close(None);
    let _ = resumed_b.close(None);
    shutdown_tx.send(()).expect("stop live daemon");
    server.join().expect("join live daemon");
    fs::remove_dir_all(root).expect("cleanup live media root");
}

fn daemon_live_session_count_for_test(port: u16, token: &str) -> usize {
    let response = http_request_with_bearer(port, "GET", "/v1/live/sessions", None, token);
    response_json_body(&response)["sessions"]
        .as_array()
        .map(Vec::len)
        .unwrap_or_default()
}

#[test]
#[ignore = "started by the Phase 4 cross-process acceptance probe"]
fn phase_four_live_relay_acceptance_daemon() {
    let ready_path = PathBuf::from(
        std::env::var("LOOM_LIVE_PHASE4_READY").expect("LOOM_LIVE_PHASE4_READY is required"),
    );
    let stop_path = PathBuf::from(
        std::env::var("LOOM_LIVE_PHASE4_STOP").expect("LOOM_LIVE_PHASE4_STOP is required"),
    );
    let root = unique_temp_dir("phase-four-live-daemon");
    let daemon = LoomDaemon::bind(
        DaemonConfig::localhost(0)
            .with_bearer_token("phase-four-live-admin")
            .with_control_plane_root(&root),
    )
    .expect("bind Phase 4 live daemon");
    let port = daemon.local_addr().expect("Phase 4 daemon address").port();
    let (instance_id, attachments) = seed_phase_four_surface_bindings(&daemon.runtime);
    seed_phase_four_device_sessions(&daemon.runtime);
    fs::write(
        &ready_path,
        serde_json::to_vec_pretty(&json!({
            "schemaVersion": 1,
            "baseUrl": format!("http://127.0.0.1:{port}"),
            "surfaceInstanceId": instance_id,
            "sourceAttachmentId": attachments[0],
            "viewerAAttachmentId": attachments[1],
            "viewerBAttachmentId": attachments[2],
            "sourceDeviceId": "device-phase4-source",
            "viewerADeviceId": "device-phase4-viewer-a",
            "viewerBDeviceId": "device-phase4-viewer-b",
        }))
        .expect("serialize Phase 4 ready document"),
    )
    .expect("write Phase 4 ready document");

    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let watcher = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(300);
        while !stop_path.exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(100));
        }
        let _ = shutdown_tx.send(());
    });
    daemon
        .serve_until(shutdown_rx)
        .expect("serve Phase 4 live daemon");
    watcher.join().expect("join Phase 4 stop watcher");
    fs::remove_dir_all(root).expect("cleanup Phase 4 live daemon root");
}

fn seed_phase_four_surface_bindings(runtime: &DaemonRuntime) -> (String, [String; 3]) {
    let mut store = runtime
        .surface_instances
        .lock()
        .expect("lock Surface store");
    let instance = store
        .create(
            "neuro.official/live-screenshot",
            "1.0.0",
            &"4".repeat(64),
            1,
            SurfaceInstancePersistence::Temporary,
            SurfaceInstanceMode::Shared,
        )
        .expect("create Phase 4 Surface instance");
    let instance_id = instance.descriptor.instance_id;
    let bindings = [
        ("hook-node:phase4-source", "device-phase4-source"),
        ("hook-node:phase4-viewer-a", "device-phase4-viewer-a"),
        ("hook-node:phase4-viewer-b", "device-phase4-viewer-b"),
    ];
    let attachments = bindings.map(|(hook, device)| {
        store
            .attach(&instance_id, hook, device, None)
            .expect("attach Phase 4 Surface device")
            .descriptor
            .attachment_id
    });
    (instance_id, attachments)
}

fn seed_phase_four_device_sessions(runtime: &DaemonRuntime) {
    let now = unix_time_millis();
    let mut registry = runtime
        .device_registry
        .lock()
        .expect("lock device registry");
    for (device_id, token) in [
        ("device-phase4-source", "phase4-source-session-token"),
        ("device-phase4-viewer-a", "phase4-viewer-a-session-token"),
        ("device-phase4-viewer-b", "phase4-viewer-b-session-token"),
    ] {
        registry.devices.insert(
            device_id.to_owned(),
            ManagedDevice {
                id: device_id.to_owned(),
                name: device_id.to_owned(),
                kind: ManagedDeviceKind::Computer,
                address: "127.0.0.1".to_owned(),
                approval: "approved".to_owned(),
                created_at: now,
                last_seen_at: Some(now),
                is_local: false,
                enabled: true,
                public_key: None,
                key_fingerprint: None,
                session_epoch: 1,
            },
        );
        registry.sessions.insert(
            sha256_bytes(token.as_bytes()),
            ActiveDeviceSession {
                device_id: device_id.to_owned(),
                expires_at_ms: now.saturating_add(300_000),
                session_epoch: 1,
                used_nonces: BTreeSet::new(),
            },
        );
    }
}
