// Dual native Applications using real Surface/Device/LAN endpoints; the parent gate observes authoritative state only without injecting controller or input.
#[test]
#[ignore = "explicit native viewer and Source menus over real Loom LAN HTTPS/WSS"]
fn live_viewer_source_native_menu_input_and_release() {
    let executable = PathBuf::from(std::env::var_os("OTTER_NATIVE_TEST_EXE").unwrap());
    assert!(executable.is_absolute() && executable.is_file());
    let f = Fixture::build_package(
        &surface_art_package_zip(
            "local-counter",
            "1.0.0",
            &json!({
                "protocolVersion":"loom.surface.v1",
                "scene":{"id":"root","type":"column","children":[
                    {"id":"label","type":"text","props":{"text":"Viewer host"}}
                ]}
            }),
            "independent",
        ),
        false,
    );
    let source_key = ed25519_dalek::SigningKey::generate(&mut OsRng);
    let (source_device, _) =
        super::wall_http::pair_with_key(f.port, "Joint native Source", &source_key);
    let viewer_key = ed25519_dalek::SigningKey::generate(&mut OsRng);
    let (viewer_device, _) =
        super::wall_http::pair_with_key(f.port, "Joint native viewer", &viewer_key);
    let lan = live_lan_update(&f, true);
    let (_, info) = admin(f.port, "GET", "/v1/otter/identity", None);
    let identity = |device: &str, key: &ed25519_dalek::SigningKey| {
        json!({
            "schemaVersion":1,"deviceId":device,"privateKey":BASE64.encode(key.to_bytes()),
            "publicKey":BASE64.encode(key.verifying_key().to_bytes())
        })
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
        joint::Otter(
            Command::new(&executable)
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
        "live_source::joint::source_input_menu_real_loom_lan",
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
    assert_eq!(ready["menu"], true);
    assert_eq!(ready["application"], true);
    let session = ready["session"].as_str().unwrap();
    let frame = f
        .live_sessions
        .wait_for_frame(session, 0, 0, Duration::from_secs(3))
        .unwrap()
        .unwrap();
    // Expected values come from fixed protocol bytes and independent pixel constants rather than any new renderer or message mapper.
    assert_eq!(frame.bytes.len(), 80);
    assert_eq!(&frame.bytes[..8], b"NLLV\x01\x01\x00\x40");
    assert_eq!(&frame.bytes[40..48], &[0, 0, 0, 2, 0, 0, 0, 2]);
    assert_eq!(&frame.bytes[64..], &[17, 83, 211, 255].repeat(4));
    let before = f.live_sessions.get(session).unwrap();
    assert!(before.source_connected && !before.session.interaction_capabilities.is_empty());
    assert_eq!(before.session.source_device_id, source_device);
    let viewer_input = f.root.join("viewer-input.json");
    fs::write(&viewer_input, serde_json::to_vec(&json!({"origin":lan["endpoint"],"certificate":info["certificate"],
        "identity":identity(&viewer_device,&viewer_key),"session":session,"artId":"publisher.test/local-counter"})).unwrap()).unwrap();
    let mut viewer = spawn(
        "live_viewer::joint::viewer_menu_real_loom_source_input_and_release",
        "OTTER_LIVE_VIEWER_JOINT_INPUT",
        &viewer_input,
        "viewer",
    );
    live_lan_phase(&f, &mut viewer, "viewer-ready.json", deadline);
    let host = read("viewer-ready.json");
    assert_eq!(host["realSurface"], true);
    assert_eq!(host["pixel"], json!([17, 83, 211, 255]));
    assert!(!host["instance"].as_str().unwrap().is_empty());
    assert!(!host["attachment"].as_str().unwrap().is_empty());
    let surface_path = format!(
        "/v1/surfaces/instances/{}",
        host["instance"].as_str().unwrap()
    );
    let (status, surface) = admin(f.port, "GET", &surface_path, None);
    assert_eq!(status, 200, "{surface}");
    let attachment = &surface["attachments"][host["attachment"].as_str().unwrap()];
    assert_eq!(attachment["descriptor"]["deviceId"], viewer_device);
    assert_eq!(attachment["lifecycle"], "active");
    assert_eq!(attachment["lifecycleRevision"], 2);
    let cursor = |controller: Option<&str>, control, input| {
        let snapshot = f
            .live_sessions
            .get_for_member(session, &viewer_device)
            .unwrap();
        assert!(!snapshot.closed && snapshot.source_connected);
        assert_eq!(snapshot.session.controller_device.as_deref(), controller);
        assert_eq!(snapshot.viewer_connections.get(&viewer_device), Some(&1));
        let actor = snapshot.requester_control.unwrap();
        assert_eq!(actor.device_id, viewer_device);
        assert_eq!(
            (actor.epoch, actor.control_sequence, actor.input_sequence),
            (1, control, input)
        );
    };
    for (index, code) in [65, 66].into_iter().enumerate() {
        let (applied, released) = if index == 0 {
            (
                "source-input-applied.json",
                "source-input-released-live.json",
            )
        } else {
            (
                "source-input-applied-again.json",
                "source-input-released-live-again.json",
            )
        };
        live_lan_phase(&f, &mut source, applied, deadline);
        live_lan_phase(
            &f,
            &mut viewer,
            &format!("viewer-sent-{}.json", index + 1),
            deadline,
        );
        assert_eq!(
            read(applied),
            json!({"actualKeyDown":code,"actualMouseDown":true,"originalWorker":true})
        );
        cursor(
            Some(&viewer_device),
            if index == 0 { 4 } else { 8 },
            if index == 0 { 2 } else { 4 },
        );
        fs::write(
            f.root.join(format!("viewer-release-{}", index + 1)),
            b"release",
        )
        .unwrap();
        live_lan_phase(&f, &mut source, released, deadline);
        let report = format!("viewer-released-{}.json", index + 1);
        live_lan_phase(&f, &mut viewer, &report, deadline);
        assert_eq!(
            read(released),
            json!({"actualKeyUp":code,"actualMouseUp":true,"originalWorkerIdle":true,"sourceStillPublishing":true})
        );
        assert_eq!(
            read(&report),
            json!({"stillViewing":true,"release":if index==0 {"menu"}else{"focus_loss"}})
        );
        cursor(
            None,
            if index == 0 { 5 } else { 9 },
            if index == 0 { 2 } else { 4 },
        );
        if index == 0 {
            fs::write(f.root.join("viewer-phase-2"), b"continue").unwrap();
        }
    }
    fs::write(f.root.join("viewer-stop"), b"stop").unwrap();
    live_lan_phase(&f, &mut viewer, "viewer-complete.json", deadline);
    viewer_source_reap(&mut viewer, deadline, &f.root);
    assert_eq!(
        read("viewer-complete.json"),
        json!({"closed":true,"ownersReclaimed":true,"editorUnchanged":true,
        "realSurfaceInactive":true,"ordinaryDualProduct":false,"physicalCrossDevice":false})
    );
    let (status, surface) = admin(f.port, "GET", &surface_path, None);
    assert_eq!(status, 200, "{surface}");
    let attachment = &surface["attachments"][host["attachment"].as_str().unwrap()];
    assert_eq!(attachment["lifecycle"], "inactive");
    assert_eq!(attachment["lifecycleRevision"], 3);
    fs::write(f.root.join("source-stop"), b"stop").unwrap();
    live_lan_phase(&f, &mut source, "source-complete.json", deadline);
    viewer_source_reap(&mut source, deadline, &f.root);
    // Client process exit is not a server-side socket Drop barrier. Keep the original
    // total deadline and require both actual role retirement and permit reclamation.
    loop {
        let after = f.live_sessions.get(session).unwrap();
        let permits = f.live_sessions.media_connections.load(Ordering::SeqCst);
        if after.closed
            && !after.source_connected
            && after.viewer_connections.is_empty()
            && permits == 0
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "server media did not retire: closed={}, source={}, viewers={:?}, permits={permits}",
            after.closed,
            after.source_connected,
            after.viewer_connections
        );
        thread::sleep(Duration::from_millis(5));
    }
    if let Some(output) = std::env::var_os("OTTER_LIVE_VIEWER_SOURCE_EVIDENCE_DIR") {
        let output = PathBuf::from(output);
        assert!(output.is_absolute());
        fs::create_dir(&output).unwrap();
        // Export only credential-free receipts; inputs, private keys, profiles, and service databases never enter the evidence bundle.
        for name in [
            "source-ready.json",
            "source-input-applied.json",
            "source-input-applied-again.json",
            "source-input-released-live.json",
            "source-input-released-live-again.json",
            "source-input-released.json",
            "source-complete.json",
            "source.stdout.log",
            "source.stderr.log",
            "viewer-ready.json",
            "viewer-sent-1.json",
            "viewer-sent-2.json",
            "viewer-released-1.json",
            "viewer-released-2.json",
            "viewer-complete.json",
            "viewer.stdout.log",
            "viewer.stderr.log",
        ] {
            fs::copy(f.root.join(name), output.join(name)).unwrap();
        }
        fs::write(output.join("joint-oracle.json"),serde_json::to_vec(&json!({"width":2,"height":2,"rawBgra":[17,83,211,255],
            "receivedBinaryBytes":80,"publicSurfaceAttachment":true,"publicViewerControlInput":true,
            "sourceActualReleaseWhilePublishing":2,"finalControlSequence":9,"finalInputSequence":4,
            "closed":true,"mediaConnections":0,"ordinaryDualProduct":false,"physicalCrossDevice":false})).unwrap()).unwrap();
    }
}
fn viewer_source_reap(child: &mut joint::Otter, deadline: Instant, root: &std::path::Path) {
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(
                status.success(),
                "native joint child failed: {}",
                root.display()
            );
            return;
        }
        assert!(
            Instant::now() < deadline,
            "native joint child reap: {}",
            root.display()
        );
        thread::sleep(Duration::from_millis(5));
    }
}
