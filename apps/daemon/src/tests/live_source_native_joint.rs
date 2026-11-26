// Actual native Application source menu through LAN HTTPS and the original media worker.
#[test]
#[ignore = "explicit native Source menu and real Loom LAN media"]
fn live_source_native_menu_frame_and_close() {
    source_native_joint(0);
}
#[test]
#[ignore = "explicit native Source control over real LAN HTTPS events"]
fn live_source_native_input_and_release() {
    source_native_joint(1);
}
#[test]
#[ignore = "explicit native Source UIA through actual LAN observations route"]
fn live_source_native_observation_stable_change_and_close() {
    source_native_joint(2);
}
#[test]
#[ignore = "explicit native Source media recovery while holding real fixture input"]
fn live_source_native_media_recovery_releases_input() {
    source_native_joint(4);
}
#[test]
#[ignore = "explicit native Source control GET recovery while holding fixture input"]
fn live_source_native_control_recovery_releases_input() {
    source_native_joint(5);
}
fn source_native_joint(mode: u8) {
    let interactive = matches!(mode, 1 | 4 | 5);
    let executable = PathBuf::from(std::env::var_os("OTTER_NATIVE_TEST_EXE").unwrap());
    assert!(executable.is_absolute() && executable.is_file());
    let f = Fixture::new();
    let key = ed25519_dalek::SigningKey::generate(&mut OsRng);
    let (device, _) = super::wall_http::pair_with_key(f.port, "Native Source joint", &key);
    let lan = live_lan_update(&f, true);
    let (_, info) = admin(f.port, "GET", "/v1/otter/identity", None);
    let input = f.root.join("source-input.json");
    fs::write(&input,serde_json::to_vec(&json!({"origin":lan["endpoint"],"certificate":info["certificate"],
        "identity":{"schemaVersion":1,"deviceId":device,"privateKey":BASE64.encode(key.to_bytes()),"publicKey":BASE64.encode(key.verifying_key().to_bytes())}})).unwrap()).unwrap();
    let stdout = fs::File::create(f.root.join("source.stdout.log")).unwrap();
    let stderr = fs::File::create(f.root.join("source.stderr.log")).unwrap();
    let test = if interactive {
        "source_input_menu_real_loom_lan"
    } else if mode == 2 {
        "source_observation_menu_real_loom_lan"
    } else {
        "source_menu_real_loom_lan"
    };
    let mut child=joint::Otter(Command::new(executable).args([
        &format!("desktop::application::remote_live::resources::tests::live_source::joint::{test}"),"--exact","--ignored","--test-threads=1","--nocapture"])
        .env("OTTER_LIVE_SOURCE_JOINT_INPUT",&input).env("OTTER_LIVE_SOURCE_RECOVERY_JOINT",match mode {4=>"1",5=>"control",_=>"0"}).stdout(stdout).stderr(stderr).spawn().unwrap());
    let deadline = Instant::now() + Duration::from_secs(30);
    live_lan_phase(&f, &mut child, "source-ready.json", deadline);
    let ready: Value =
        serde_json::from_slice(&fs::read(f.root.join("source-ready.json")).unwrap()).unwrap();
    assert_eq!(ready["menu"], true);
    assert_eq!(ready["application"], true);
    let session = ready["session"].as_str().unwrap();
    let frame = f
        .live_sessions
        .wait_for_frame(session, 0, 0, Duration::from_secs(3))
        .unwrap()
        .unwrap();
    assert_eq!(frame.bytes.len(), 80);
    assert_eq!(&frame.bytes[..8], b"NLLV\x01\x01\x00\x40");
    assert_eq!(&frame.bytes[40..48], &[0, 0, 0, 2, 0, 0, 0, 2]);
    assert_eq!(&frame.bytes[64..], &[17, 83, 211, 255].repeat(4));
    let before = f.live_sessions.get(session).unwrap();
    assert!(before.source_connected);
    assert_eq!(before.session.source_device_id, device);
    assert_eq!(
        !before.session.interaction_capabilities.is_empty(),
        interactive
    );
    if interactive {
        // Only the controller fixture uses the authoritative store directly; Source reception still goes over real LAN HTTPS without spoofing viewer public HTTP/UI.
        let (viewer, _) = pair(f.port, "Input joint controller");
        let envelope = |sequence, kind: &str, payload: Value| {
            serde_json::from_value(json!({"protocolVersion":"loom.live.v1","sessionId":session,"epoch":1,"sequence":sequence,"messageType":kind,"payload":payload})).unwrap()
        };
        f.live_sessions
            .attach_viewer(
                &viewer,
                envelope(
                    1,
                    "session_ack",
                    json!({"accepted":true,"responderDeviceId":viewer}),
                ),
            )
            .unwrap();
        f.live_sessions.change_controller(&viewer,session,&serde_json::from_value(json!({"protocolVersion":"loom.live.v1","action":"acquire","epoch":1,"sequence":2})).unwrap()).unwrap();
        for (sequence, kind) in [
            (
                1,
                json!({"kind":"key","data":{"code":"vk:65","state":"pressed","modifiers":0}}),
            ),
            (
                2,
                json!({"kind":"mouse_button","data":{"x":0.5,"y":0.5,"button":"left","state":"pressed","clickCount":1}}),
            ),
        ] {
            f.live_sessions.forward_input(&viewer,session,envelope(sequence+2,"input_event",json!({"inputSequence":sequence,"issuedAtMs":1,"sourceDeviceId":viewer,"kind":kind}))).unwrap();
        }
        live_lan_phase(&f, &mut child, "source-input-applied.json", deadline);
        assert_eq!(
            fs::read(f.root.join("source-input-applied.json")).unwrap(),
            br#"{"actualKeyDown":65,"actualMouseDown":true,"originalWorker":true}"#
        );
        if mode == 4 || mode == 5 {
            fs::write(
                f.root.join("source-disconnect"),
                b"disconnect owned Source WSS",
            )
            .unwrap();
            live_lan_phase(&f, &mut child, "source-disconnected.json", deadline);
            live_lan_phase(&f, &mut child, "source-resumed.json", deadline);
            live_lan_phase(&f, &mut child, "source-input-released-live.json", deadline);
            let current = f.live_sessions.get(session).unwrap();
            assert!(current.source_connected && !current.closed && current.epoch == 1);
            assert!(current.last_frame_id > frame.frame_id);
            assert!(current.session.controller_device.is_none());
            let latest = f
                .live_sessions
                .wait_for_frame(session, 1, frame.frame_id, Duration::from_secs(1))
                .unwrap()
                .unwrap();
            assert_eq!(&latest.bytes[64..], &[17, 83, 211, 255].repeat(4));
            // Recovery does not regain prior control; only a fresh explicit acquire and new input reach the owned window again.
            f.live_sessions.change_controller(&viewer,session,&serde_json::from_value(json!({"protocolVersion":"loom.live.v1","action":"acquire","epoch":1,"sequence":5})).unwrap()).unwrap();
            for (input_sequence, kind) in [
                (
                    3,
                    json!({"kind":"key","data":{"code":"vk:66","state":"pressed","modifiers":0}}),
                ),
                (
                    4,
                    json!({"kind":"mouse_button","data":{"x":0.5,"y":0.5,"button":"left","state":"pressed","clickCount":1}}),
                ),
            ] {
                f.live_sessions.forward_input(&viewer,session,envelope(input_sequence+3,"input_event",json!({"inputSequence":input_sequence,"issuedAtMs":1,"sourceDeviceId":viewer,"kind":kind}))).unwrap();
            }
            live_lan_phase(&f, &mut child, "source-input-applied-again.json", deadline);
            f.live_sessions.change_controller(&viewer,session,&serde_json::from_value(json!({"protocolVersion":"loom.live.v1","action":"release","epoch":1,"sequence":8})).unwrap()).unwrap();
            live_lan_phase(
                &f,
                &mut child,
                "source-input-released-live-again.json",
                deadline,
            );
        }
    }
    let mut observation_oracle = None;
    if mode == 2 {
        let mut stages = Vec::new();
        for expected in ["value-seven", "value-eight"] {
            if expected == "value-eight" {
                fs::write(f.root.join("source-change"), b"change owned fixture").unwrap();
            }
            loop {
                let snapshot = f.live_sessions.get(session).unwrap();
                let values = serde_json::to_value(&snapshot.observations).unwrap();
                assert!(!values.to_string().contains("password-secret"));
                if let Some(value) = values.as_array().unwrap().iter().find(|v| {
                    v["state"] == "stable"
                        && v["locator"]["controlType"] == "Edit"
                        && v["value"]["value"]["text"] == expected
                }) {
                    assert_eq!(value["source"], "ui_automation");
                    assert_eq!(value["confidence"], "exact");
                    assert_eq!(value["value"]["value"]["readOnly"], false);
                    assert!(value["sequence"].as_u64().unwrap() >= 2);
                    assert!(
                        value["stableSinceMs"].as_u64().unwrap()
                            <= value["observedAtMs"].as_u64().unwrap()
                    );
                    stages.push(value.clone());
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "UIA observation joint timeout: {}",
                    f.root.display()
                );
                thread::sleep(Duration::from_millis(5));
            }
        }
        assert_eq!(stages[0]["observationId"], stages[1]["observationId"]);
        assert!(stages[1]["sequence"].as_u64().unwrap() > stages[0]["sequence"].as_u64().unwrap());
        observation_oracle = Some(
            json!({"samples":stages,"passwordExcluded":true,"actualLanObservationsRoute":true,"nativeSourceMenu":true,"ordinaryDualProduct":false,"physicalCrossDevice":false}),
        );
    }
    fs::write(f.root.join("source-stop"), b"stop").unwrap();
    live_lan_phase(&f, &mut child, "source-complete.json", deadline);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(
                status.success(),
                "native Source failure: {}",
                f.root.display()
            );
            break;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    let after = f.live_sessions.get(session).unwrap();
    assert!(after.closed);
    assert!(!after.source_connected);
    assert_eq!(f.live_sessions.media_connections.load(Ordering::SeqCst), 0);
    if let Some(output) = std::env::var_os("OTTER_LIVE_SOURCE_EVIDENCE_DIR") {
        let output = PathBuf::from(output);
        assert!(output.is_absolute());
        fs::create_dir(&output).unwrap();
        for name in [
            "source-ready.json",
            "source-complete.json",
            "source.stdout.log",
            "source.stderr.log",
        ] {
            fs::copy(f.root.join(name), output.join(name)).unwrap();
        }
        if interactive {
            for name in ["source-input-applied.json", "source-input-released.json"] {
                fs::copy(f.root.join(name), output.join(name)).unwrap();
            }
        }
        if mode == 4 || mode == 5 {
            for name in [
                "source-disconnected.json",
                "source-resumed.json",
                "source-input-released-live.json",
                "source-input-applied-again.json",
                "source-input-released-live-again.json",
            ] {
                fs::copy(f.root.join(name), output.join(name)).unwrap();
            }
        }
        if let Some(oracle) = observation_oracle {
            fs::write(
                output.join("source-observation-oracle.json"),
                serde_json::to_vec_pretty(&oracle).unwrap(),
            )
            .unwrap();
        }
        fs::write(output.join("source-oracle.json"),serde_json::to_vec(&json!({"width":2,"height":2,"rawBgra":[17,83,211,255],"receivedBinaryBytes":frame.bytes.len(),"closed":true,"mediaConnections":0,"sourceDisconnectRecovery":mode==4,"sourceControlRecovery":mode==5})).unwrap()).unwrap();
    }
}
