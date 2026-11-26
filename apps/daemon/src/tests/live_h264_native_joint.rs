// Real Source/Viewer menus, system codec, and LAN chain; the extra local Viewer socket sends only real video control messages.
#[test]
#[ignore = "explicit native H264 Source and viewer menus over real Loom LAN"]
fn live_h264_native_source_viewer_current_keyframes() {
    h264_native_joint(false, false, false, false);
}

#[test]
#[ignore = "explicit native media disconnect recovery over real Loom LAN"]
fn live_h264_native_source_viewer_media_disconnect_recovery() {
    h264_native_joint(true, false, false, false);
}

#[test]
#[ignore = "explicit native control GET disconnect recovery over real Loom LAN"]
fn live_h264_native_source_viewer_control_disconnect_recovery() {
    h264_native_joint(false, true, false, false);
}

#[test]
#[ignore = "explicit native Source media disconnect recovery over real Loom LAN"]
fn live_h264_native_source_viewer_source_disconnect_recovery() {
    h264_native_joint(false, false, true, false);
}

#[test]
#[ignore = "explicit real WGC source through LAN H264 to native viewer and resize"]
fn live_h264_native_wgc_source_resize_and_pixels() {
    h264_native_joint(false, false, false, true);
}

fn h264_native_joint(reconnect: bool, control_reconnect: bool, source_reconnect: bool, wgc: bool) {
    use tungstenite::{client::IntoClientRequest, http::HeaderValue, Message};
    let executable = PathBuf::from(std::env::var_os("OTTER_NATIVE_TEST_EXE").unwrap());
    assert!(executable.is_absolute() && executable.is_file());
    let f = Fixture::build_package(
        &surface_art_package_zip(
            "local-counter",
            "1.0.0",
            &json!({"protocolVersion":"loom.surface.v1","scene":{"id":"root","type":"text","props":{"text":"Video host"}}}),
            "independent",
        ),
        false,
    );
    let source_key = ed25519_dalek::SigningKey::generate(&mut OsRng);
    let (source_device, _) = super::wall_http::pair_with_key(f.port, "H264 Source", &source_key);
    let viewer_key = ed25519_dalek::SigningKey::generate(&mut OsRng);
    let (viewer_device, _) = super::wall_http::pair_with_key(f.port, "H264 viewer", &viewer_key);
    let lan = live_lan_update(&f, true);
    let (_, info) = admin(f.port, "GET", "/v1/otter/identity", None);
    let identity = |device: &str, key: &ed25519_dalek::SigningKey| {
        json!({"schemaVersion":1,
        "deviceId":device,"privateKey":BASE64.encode(key.to_bytes()),"publicKey":BASE64.encode(key.verifying_key().to_bytes())})
    };
    let source_input = f.root.join("source-input.json");
    fs::write(
        &source_input,
        serde_json::to_vec(
            &json!({"origin":lan["endpoint"],"certificate":info["certificate"],
        "identity":identity(&source_device,&source_key)}),
        )
        .unwrap(),
    )
    .unwrap();
    let spawn = |test: &str, variable: &str, input: &PathBuf, prefix: &str| {
        let mut command = Command::new(&executable);
        if wgc {
            command.env("OTTER_LIVE_WGC_JOINT", "1");
        }
        if reconnect {
            command.env("OTTER_LIVE_MEDIA_RECOVERY_JOINT", "1");
        }
        if control_reconnect {
            command.env("OTTER_LIVE_CONTROL_RECOVERY_JOINT", "1");
        }
        if source_reconnect {
            command.env("OTTER_LIVE_SOURCE_RECOVERY_JOINT", "1");
        }
        joint::Otter(
            command
                .args([
                    &format!("desktop::application::remote_live::resources::tests::{test}"),
                    "--exact",
                    "--ignored",
                    "--test-threads=1",
                    "--nocapture",
                ])
                .env(variable, input)
                .stdout(fs::File::create(f.root.join(format!("{prefix}.stdout.log"))).unwrap())
                .stderr(fs::File::create(f.root.join(format!("{prefix}.stderr.log"))).unwrap())
                .spawn()
                .unwrap(),
        )
    };
    let mut source = spawn(
        "live_source::joint::source_h264_menu_real_loom_lan",
        "OTTER_LIVE_SOURCE_JOINT_INPUT",
        &source_input,
        "source",
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    live_lan_phase(&f, &mut source, "source-ready.json", deadline);
    let read = |name: &str| -> Value {
        serde_json::from_slice(&fs::read(f.root.join(name)).unwrap()).unwrap()
    };
    let ready = read("source-ready.json");
    let session = ready["session"].as_str().unwrap();
    // Child readiness proves only local send; HTTP 101 precedes the server media worker startup and is not a remote receive barrier.
    let raw = f
        .live_sessions
        .wait_for_frame(session, 0, 0, Duration::from_secs(3))
        .unwrap()
        .unwrap();
    assert_eq!(&raw.bytes[64..], &[17, 83, 211, 255].repeat(64 * 64));
    let before = f.live_sessions.get(session).unwrap();
    assert!(before.source_connected);
    assert_eq!(
        before.session.frame_stream.codec,
        loom_protocol::LiveCodec::H264
    );
    let viewer_input = f.root.join("viewer-input.json");
    fs::write(&viewer_input,serde_json::to_vec(&json!({"origin":lan["endpoint"],"certificate":info["certificate"],
        "identity":identity(&viewer_device,&viewer_key),"session":session,"artId":"publisher.test/local-counter"})).unwrap()).unwrap();
    let mut viewer = spawn(
        "live_viewer::joint::viewer_h264_real_loom_source_policy_and_pixels",
        "OTTER_LIVE_VIEWER_JOINT_INPUT",
        &viewer_input,
        "viewer",
    );
    live_lan_phase(&f, &mut viewer, "viewer-ready.json", deadline);
    h264_native_wire(
        &f,
        session,
        raw.frame_id,
        loom_protocol::LiveCodec::H264,
        "viewer-h264",
        deadline,
    );
    live_lan_phase(&f, &mut viewer, "viewer-h264.json", deadline);
    let mut recovered_frame = None;
    if reconnect {
        let before = f.live_sessions.get(session).unwrap();
        let original = read("viewer-h264.json")["frameId"].as_u64().unwrap();
        fs::write(f.root.join("viewer-disconnect"), b"disconnect").unwrap();
        live_lan_phase(&f, &mut viewer, "viewer-disconnected.json", deadline);
        recovered_frame = Some(h264_native_wire(
            &f,
            session,
            original,
            loom_protocol::LiveCodec::H264,
            "viewer-media-resumed",
            deadline,
        ));
        live_lan_phase(&f, &mut viewer, "viewer-media-resumed.json", deadline);
        let after = f.live_sessions.get(session).unwrap();
        assert_eq!(before.epoch, after.epoch);
        assert_eq!(before.session.revision, after.session.revision);
        assert_eq!(before.session.viewer_devices, after.session.viewer_devices);
        assert!(after.source_connected && !after.closed);
    }
    if source_reconnect {
        let original = read("viewer-h264.json")["frameId"].as_u64().unwrap();
        let before = f
            .live_sessions
            .get_for_member(session, &source_device)
            .unwrap();
        fs::write(
            f.root.join("source-disconnect"),
            b"disconnect owned Source WSS",
        )
        .unwrap();
        live_lan_phase(&f, &mut source, "source-disconnected.json", deadline);
        live_lan_phase(&f, &mut source, "source-resumed.json", deadline);
        recovered_frame = Some(h264_native_wire(
            &f,
            session,
            original,
            loom_protocol::LiveCodec::H264,
            "viewer-source-resumed",
            deadline,
        ));
        live_lan_phase(&f, &mut viewer, "viewer-source-resumed.json", deadline);
        let after = f
            .live_sessions
            .get_for_member(session, &source_device)
            .unwrap();
        assert_eq!(before.epoch, after.epoch);
        assert_eq!(
            before.requester_control.unwrap().control_sequence,
            after.requester_control.unwrap().control_sequence
        );
        assert!(after.source_connected && !after.closed);
        assert_eq!(after.viewer_connections.get(&viewer_device), Some(&1));
    }
    // Extra local connection uses standard Device/nonce authentication and member admission without directly mutating video negotiation state.
    let (probe_device, probe_token) = pair(f.port, "H264 control probe");
    let attach_probe = || {
        f.live_sessions.attach_viewer(&probe_device,serde_json::from_value(json!({"protocolVersion":"loom.live.v1",
        "sessionId":session,"epoch":1,"sequence":1,"messageType":"session_ack","payload":{"accepted":true,"responderDeviceId":probe_device}})).unwrap()).unwrap()
    };
    if control_reconnect {
        let before = f
            .live_sessions
            .get_for_member(session, &viewer_device)
            .unwrap();
        fs::write(f.root.join("viewer-control-disconnect"), b"disconnect GET").unwrap();
        live_lan_phase(
            &f,
            &mut viewer,
            "viewer-control-disconnected.json",
            deadline,
        );
        attach_probe();
        let (_, events) = f.live_sessions.events_after(session, 0).unwrap();
        let sequence = events.last().unwrap().sequence;
        let path = f.root.join("wire-viewer-control-resumed.json");
        fs::write(
            path.with_extension("tmp"),
            serde_json::to_vec(&json!({"after":sequence})).unwrap(),
        )
        .unwrap();
        fs::rename(path.with_extension("tmp"), path).unwrap();
        live_lan_phase(&f, &mut viewer, "viewer-control-resumed.json", deadline);
        let after = f
            .live_sessions
            .get_for_member(session, &viewer_device)
            .unwrap();
        assert_eq!(before.epoch, after.epoch);
        let a = before.requester_control.unwrap();
        let b = after.requester_control.as_ref().unwrap();
        assert_eq!(
            (&a.device_id, a.epoch, a.control_sequence, a.input_sequence),
            (&b.device_id, b.epoch, b.control_sequence, b.input_sequence)
        );
        assert_eq!(after.viewer_connections.get(&viewer_device), Some(&1));
        assert!(after.source_connected && !after.closed);
    } else {
        attach_probe();
    }
    let connect = |profile: &'static str| {
        let mut request = format!(
            "ws://127.0.0.1:{}/v1/live/media?sessionId={session}&role=viewer",
            f.port
        )
        .into_client_request()
        .unwrap();
        request.headers_mut().insert(
            "authorization",
            HeaderValue::from_str(&format!("Device {probe_token}")).unwrap(),
        );
        request.headers_mut().insert(
            "x-loom-device-nonce",
            HeaderValue::from_str(&Uuid::new_v4().to_string()).unwrap(),
        );
        request
            .headers_mut()
            .insert("sec-websocket-protocol", HeaderValue::from_static(profile));
        let (mut socket, response) = tungstenite::connect(request).unwrap();
        assert_eq!(response.headers()["sec-websocket-protocol"], profile);
        let tungstenite::stream::MaybeTlsStream::Plain(tcp) = socket.get_mut() else {
            panic!("loopback socket")
        };
        tcp.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        socket
    };
    let mut probe = connect("loom.live.h264.v1");
    let mut latest =
        recovered_frame.unwrap_or_else(|| read("viewer-h264.json")["frameId"].as_u64().unwrap());
    let mut frames = Vec::new();
    for request in 0..2 {
        if request == 1 {
            probe
                .send(Message::Text(
                    r#"{"type":"keyframe_request","epoch":1}"#.into(),
                ))
                .unwrap();
        }
        loop {
            assert!(Instant::now() < deadline);
            match probe.read().unwrap() {
                Message::Binary(bytes) => {
                    let (_, m) = LiveBinaryFrame::decode_header(&bytes).unwrap();
                    if m.codec == loom_protocol::LiveCodec::H264
                        && m.keyframe
                        && m.frame_id > latest
                    {
                        latest = m.frame_id;
                        frames.push(
                            json!({"frameId":m.frame_id,"keyframe":true,"bytes":bytes.len()}),
                        );
                        break;
                    }
                }
                Message::Ping(bytes) => probe.send(Message::Pong(bytes)).unwrap(),
                other => panic!("unexpected video probe: {other:?}"),
            }
        }
    }
    probe.close(None).unwrap();
    let _ = probe.read();
    drop(probe);
    if wgc {
        fs::write(
            f.root.join("source-wgc-change"),
            b"repaint and resize owned WGC source",
        )
        .unwrap();
        let mut cursor = latest;
        loop {
            assert!(
                Instant::now() < deadline,
                "WGC resize wire frame: {}",
                f.root.display()
            );
            let Some(frame) = f
                .live_sessions
                .wait_for_frame(session, 1, cursor, Duration::from_millis(100))
                .unwrap()
            else {
                continue;
            };
            cursor = frame.frame_id;
            let (_, metadata) = LiveBinaryFrame::decode_header(&frame.bytes).unwrap();
            if (metadata.width, metadata.height) != (96, 80) {
                continue;
            }
            assert_eq!(metadata.codec, loom_protocol::LiveCodec::H264);
            assert!(metadata.keyframe);
            let path = f.root.join("wire-viewer-wgc-updated.json");
            fs::write(
                path.with_extension("tmp"),
                serde_json::to_vec(
                    &json!({"frameId":cursor,"codec":"h264","width":96,"height":80}),
                )
                .unwrap(),
            )
            .unwrap();
            fs::rename(path.with_extension("tmp"), path).unwrap();
            break;
        }
        live_lan_phase(&f, &mut viewer, "viewer-wgc-updated.json", deadline);
    }
    fs::write(f.root.join("viewer-stop"), b"stop").unwrap();
    live_lan_phase(&f, &mut viewer, "viewer-complete.json", deadline);
    viewer_source_reap(&mut viewer, deadline, &f.root);
    fs::write(f.root.join("source-stop"), b"stop").unwrap();
    live_lan_phase(&f, &mut source, "source-complete.json", deadline);
    viewer_source_reap(&mut source, deadline, &f.root);
    loop {
        let after = f.live_sessions.get(session).unwrap();
        if after.closed
            && !after.source_connected
            && after.viewer_connections.is_empty()
            && f.live_sessions.media_connections.load(Ordering::SeqCst) == 0
        {
            break;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    if let Some(output) = std::env::var_os("OTTER_LIVE_H264_EVIDENCE_DIR") {
        let output = PathBuf::from(output);
        assert!(output.is_absolute());
        fs::create_dir(&output).unwrap();
        for name in [
            "source-ready.json",
            "source-complete.json",
            "source.stdout.log",
            "source.stderr.log",
            "viewer-ready.json",
            "viewer-h264.json",
            "viewer-complete.json",
            "viewer.stdout.log",
            "viewer.stderr.log",
            "wire-viewer-h264.json",
        ] {
            fs::copy(f.root.join(name), output.join(name)).unwrap();
        }
        if reconnect {
            for name in [
                "viewer-disconnected.json",
                "viewer-media-resumed.json",
                "wire-viewer-media-resumed.json",
            ] {
                fs::copy(f.root.join(name), output.join(name)).unwrap();
            }
        }
        if control_reconnect {
            for name in [
                "viewer-control-disconnected.json",
                "viewer-control-resumed.json",
                "wire-viewer-control-resumed.json",
            ] {
                fs::copy(f.root.join(name), output.join(name)).unwrap();
            }
        }
        if source_reconnect {
            for name in [
                "source-disconnected.json",
                "source-resumed.json",
                "wire-viewer-source-resumed.json",
                "viewer-source-resumed.json",
            ] {
                fs::copy(f.root.join(name), output.join(name)).unwrap();
            }
        }
        if wgc {
            for name in [
                "source-wgc-complete.json",
                "wire-viewer-wgc-updated.json",
                "viewer-wgc-updated.json",
            ] {
                fs::copy(f.root.join(name), output.join(name)).unwrap();
            }
        }
        fs::write(output.join("oracle.json"),serde_json::to_vec_pretty(&json!({"keyframeRequests":frames,
            "staticGpuSource":!wgc,"realWgcSource":wgc,"sourceAndViewerMenus":true,"endpoint":lan["endpoint"],"currentH264Only":true,"nativeDecoderAndUpload":true,
            "mediaPermits":0,"mediaDisconnectRecovery":reconnect,"controlDisconnectRecovery":control_reconnect,"sourceDisconnectRecovery":source_reconnect,"ordinaryDualProduct":false,"physicalCrossDevice":false})).unwrap()).unwrap();
    }
}

fn h264_native_wire(
    f: &Fixture,
    session: &str,
    after: u64,
    codec: loom_protocol::LiveCodec,
    name: &str,
    deadline: Instant,
) -> u64 {
    let mut cursor = after;
    loop {
        assert!(
            Instant::now() < deadline,
            "wire phase {name}: {}",
            f.root.display()
        );
        let Some(frame) = f
            .live_sessions
            .wait_for_frame(session, 1, cursor, Duration::from_millis(100))
            .unwrap()
        else {
            continue;
        };
        cursor = frame.frame_id;
        let (_, metadata) = LiveBinaryFrame::decode_header(&frame.bytes).unwrap();
        if metadata.codec != codec {
            continue;
        }
        assert!(metadata.keyframe);
        assert_eq!((metadata.width, metadata.height), (64, 64));
        if codec == loom_protocol::LiveCodec::RawBgra {
            assert_eq!(&frame.bytes[64..], &[17, 83, 211, 255].repeat(64 * 64));
        }
        let path = f.root.join(format!("wire-{name}.json"));
        let temporary = path.with_extension("tmp");
        fs::write(&temporary,serde_json::to_vec(&json!({"frameId":cursor,"codec":if codec==loom_protocol::LiveCodec::H264 {"h264"} else {"raw_bgra"},"keyframe":true})).unwrap()).unwrap();
        fs::rename(temporary, path).unwrap();
        return cursor;
    }
}
