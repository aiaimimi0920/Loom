// 本叶在原Tile联合fixture内编译：真实Source菜单/媒体到混合Tile，独立像素和权威状态oracle。
#[test]
#[ignore = "explicit real Source LAN WSS to mixed native Tile raw/PNG freeze black resume stop"]
fn real_lan_native_tile_live_mixed_frames_and_cleanup() {
    live_run(false);
}
#[test]
#[ignore = "explicit real Source LAN WSS to mixed native Tile revocation cleanup"]
fn real_lan_native_tile_live_revocation_cannot_publish_late_frames() {
    live_run(true);
}
fn live_run(revoked: bool) {
    let (f, tile_device, mut server, sessions) = setup();
    let (mut source, ready, session, original_frame) = live_source(&f, &sessions);
    let session = session.as_str();
    let mut output = f.child("live_and_cleanup", revoked);
    let registered = f.until(&mut output, |s| {
        s["endpoints"]
            .as_array()
            .is_some_and(|a| a.len() == 1 && a[0]["online"] == true)
    });
    let endpoint = &registered["endpoints"][0]["endpoint"];
    assert_eq!(
        endpoint["renderModes"],
        json!(["image", "raw_bgra", "surface_v1"])
    );
    assert_eq!(endpoint["pixelSize"], json!({"width":4,"height":2}));
    let (status, image) = admin(
        f.port,
        "POST",
        "/v1/surfaces/resources",
        Some(
            json!({"kind":"image","mime":"application/x-neuro-rgba8","width":2,"height":2,"dataBase64":BASE64.encode([255,0,0,255,0,255,0,255,0,0,255,255,255,255,255,255])}),
        ),
    );
    assert_eq!(status, 201, "{image}");
    let layout = json!({"protocolVersion":"loom.wall.v1","wallId":"native-image-wall","revision":2,"bounds":{"x":0,"y":0,"width":4,"height":2},
        "tiles":[{"tileId":"native-tile","endpointId":endpoint["endpointId"],"rect":{"x":0,"y":0,"width":4,"height":2},"rotation":"deg0"}],
        "placements":[{"placementId":"picture","source":{"kind":"image","id":image["resource"]["resourceId"]},"rect":{"x":0,"y":0,"width":2,"height":2},"sourceCrop":{"x":0,"y":0,"width":1,"height":1},"zIndex":0,"interactive":false},
        {"placementId":"live","source":{"kind":"live","id":session},"rect":{"x":2,"y":0,"width":2,"height":2},"sourceCrop":{"x":0,"y":0,"width":1,"height":1},"zIndex":1,"interactive":false}]});
    let (status, scheduled) = admin(
        f.port,
        "PUT",
        "/v1/walls/layouts",
        Some(json!({"baseRevision":1,"layout":layout,"activationDelayMs":300})),
    );
    assert_eq!(status, 200, "{scheduled}");
    let first = f.phase(&mut output, "tile-live-first.json");
    let updated = f.phase(&mut output, "tile-live-updated.json");
    assert_eq!(first["rawPbgra"], json!([17, 83, 211, 255]));
    assert_eq!(first["pngPbgra"], json!([17, 83, 211, 255]));
    let current = sessions.get(session).unwrap();
    assert!(current.source_connected && !current.closed);
    assert!(current.session.viewer_devices.is_empty());
    assert!(current.viewer_connections.is_empty());
    assert!(current.session.controller_device.is_none());
    assert!(current.last_frame_id > original_frame);
    let mut proof = json!({"registered":registered,"first":first,"updated":updated,"sourceMenu":ready,"sourceId":session,
        "noSurfaceMembership":true,"noController":true,"rawAndPngDecoded":true,"nativePresentConfirmed":true,"finalSwapChainReadback":false});
    if revoked {
        assert_eq!(admin(f.port,"PUT",&format!("/v1/devices/{tile_device}"),Some(json!({"name":"Tile isolated","kind":"computer","address":"127.0.0.1","enabled":false}))).0,200);
    } else {
        proof["frozen"] = f.present(&mut output, "frozen");
        f.phase(&mut output, "tile-live-frozen.json");
        // 冻结已停Tile媒体，但Source仍发布；不能把保留旧图算作新帧。
        let start = Instant::now();
        while sessions.media_connections.load(Ordering::SeqCst) != 1 {
            f.alive(&mut output);
            assert!(start.elapsed() < Duration::from_secs(3));
            thread::sleep(Duration::from_millis(5));
        }
        proof["black"] = f.present(&mut output, "black");
        f.phase(&mut output, "tile-live-black.json");
        proof["resumed"] = f.present(&mut output, "running");
        f.phase(&mut output, "tile-live-resumed.json");
        fs::write(f.root.join("tile-stop"), b"stop").unwrap();
    }
    f.joined(&mut output);
    let complete: Value =
        serde_json::from_slice(&fs::read(f.root.join("tile-output-complete.json")).unwrap())
            .unwrap();
    assert_eq!(
        complete,
        json!({"live":true,"revoked":revoked,"pixelsReleased":true,"threadsJoined":true,"ordinaryOutputExe":false,"finalSwapChainReadback":false})
    );
    fs::write(f.root.join("source-stop"), b"stop").unwrap();
    f.phase(&mut source, "source-complete.json");
    f.joined(&mut source);
    while sessions.media_connections.load(Ordering::SeqCst) != 0 {
        assert!(Instant::now() < f.deadline);
        thread::sleep(Duration::from_millis(5));
    }
    assert!(sessions.get(session).unwrap().closed);
    assert_eq!(f.state()["endpoints"][0]["online"], false);
    proof["cleanup"] = complete;
    proof["mediaConnections"] = json!(0);
    server.finish().unwrap();
    if let Some(dir) = std::env::var_os("OTTER_TILE_JOINT_EVIDENCE_DIR") {
        let dir = PathBuf::from(dir);
        assert!(dir.is_absolute());
        fs::create_dir(&dir).unwrap();
        fs::write(
            dir.join("tile-live-service-oracle.json"),
            serde_json::to_vec_pretty(&proof).unwrap(),
        )
        .unwrap();
        for name in [
            "live-source.stdout.log",
            "live-source.stderr.log",
            "live_and_cleanup.stdout.log",
            "live_and_cleanup.stderr.log",
        ] {
            fs::copy(f.root.join(name), dir.join(name)).unwrap();
        }
    }
}
fn live_source(f: &Joint, sessions: &SharedLiveSessionStore) -> (Native, Value, String, u64) {
    let key = SigningKey::generate(&mut OsRng);
    let (source_device, _) = pair_with_key(f.port, "Tile Live Source", &key);
    let (_, info) = admin(f.port, "GET", "/v1/otter/identity", None);
    let input = f.root.join("source-input.json");
    fs::write(&input,serde_json::to_vec(&json!({"origin":f.origin,"certificate":info["certificate"],
        "identity":{"schemaVersion":1,"deviceId":source_device,"privateKey":BASE64.encode(key.to_bytes()),"publicKey":BASE64.encode(key.verifying_key().to_bytes())}})).unwrap()).unwrap();
    let mut source=Native(Command::new(std::env::var_os("OTTER_NATIVE_TEST_EXE").unwrap()).args([
        "desktop::application::remote_live::resources::tests::live_source::joint::source_tile_menu_real_loom_lan","--exact","--ignored","--test-threads=1","--nocapture"])
        .env("OTTER_LIVE_SOURCE_JOINT_INPUT",&input)
        .stdout(fs::File::create(f.root.join("live-source.stdout.log")).unwrap())
        .stderr(fs::File::create(f.root.join("live-source.stderr.log")).unwrap()).spawn().unwrap());
    let ready = f.phase(&mut source, "source-ready.json");
    let session = ready["session"].as_str().unwrap();
    let original = sessions
        .wait_for_frame(session, 0, 0, Duration::from_secs(3))
        .unwrap()
        .unwrap();
    assert_eq!(&original.bytes[64..], &[17, 83, 211, 255].repeat(4));
    (source, ready.clone(), session.to_owned(), original.frame_id)
}
