// Real LAN HTTPS/WSS handoff to the original Live owner and Otter's native GPU consumer.
fn live_lan_phase(f: &Fixture, child: &mut joint::Otter, name: &str, deadline: Instant) {
    while !f.root.join(name).exists() {
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "LAN native child exited: {}",
            f.root.display()
        );
        assert!(
            Instant::now() < deadline,
            "LAN phase {name}: {}",
            f.root.display()
        );
        thread::sleep(Duration::from_millis(10));
    }
}
fn live_lan_update(f: &Fixture, enabled: bool) -> Value {
    let host = std::env::var("OTTER_LIVE_TEST_LAN_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let address: std::net::Ipv4Addr = host.parse().unwrap();
    assert!(address.is_private() || address.is_loopback());
    let reservation = std::net::TcpListener::bind((address, 0)).unwrap();
    let port = reservation.local_addr().unwrap().port();
    drop(reservation);
    let (status, result) = admin(
        f.port,
        "PUT",
        "/v1/otter/connections/lan",
        Some(json!({"enabled":enabled,"host":host,"port":port})),
    );
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["listening"], enabled);
    result
}
fn live_lan_zero(f: &Fixture) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while f.live_sessions.media_connections.load(Ordering::SeqCst) != 0 {
        assert!(Instant::now() < deadline, "LAN media permits leaked");
        thread::sleep(Duration::from_millis(10));
    }
    let snapshot = f.live_sessions.get("live:lan/part").unwrap();
    assert!(!snapshot.source_connected && snapshot.viewer_connections.is_empty());
}
#[test]
#[ignore = "explicit real LAN HTTPS/WSS and native GPU cross-repository gate"]
fn live_lan_native_joint_control_media_revocation_and_restart() {
    live_lan_native_joint(false);
}
#[test]
#[ignore = "explicit designated viewer through real LAN HTTPS/WSS and native GPU"]
fn live_direct_native_joint_updates_and_revocation() {
    live_lan_native_joint(true);
}
fn live_lan_native_joint(direct: bool) {
    let executable = PathBuf::from(std::env::var_os("OTTER_NATIVE_TEST_EXE").unwrap());
    assert!(executable.is_absolute() && executable.is_file());
    let mut f = Fixture::new();
    let key = ed25519_dalek::SigningKey::generate(&mut OsRng);
    let (viewer, viewer_token) = super::wall_http::pair_with_key(f.port, "LAN native viewer", &key);
    let (source, source_token) = pair(f.port, "LAN source");
    f.device = viewer;
    f.token = viewer_token.clone();
    f.request["binding"]["deviceId"] = json!(f.device);
    let lan = live_lan_update(&f, true);
    f.request["binding"]["origin"] = lan["endpoint"].clone();
    assert_eq!(f.approve(0, true).0, 200);
    let (_, info) = admin(f.port, "GET", "/v1/otter/identity", None);
    let mut start = live_start_envelope("live:lan/part");
    let LiveControlMessage::SessionStart(message) = &mut start.message else {
        panic!("source start");
    };
    message.session.source_device_id = source.clone();
    message.requested_by_device_id = source;
    let input = f.root.join("lan-input.json");
    fs::write(&input, serde_json::to_vec(&json!({"request":f.request,"issuer":f.issuer,"certificate":info["certificate"],"start":start,
        "sourceToken":source_token,"viewerToken":viewer_token,"direct":direct,
        "identity":{"schemaVersion":1,"deviceId":f.device,"privateKey":BASE64.encode(key.to_bytes()),"publicKey":BASE64.encode(key.verifying_key().to_bytes())}})).unwrap()).unwrap();
    let stdout = fs::File::create(f.root.join("lan.stdout.log")).unwrap();
    let stderr = fs::File::create(f.root.join("lan.stderr.log")).unwrap();
    let mut child = joint::Otter(Command::new(executable)
        .args(["device_session_client::tests::live_control::art_prepare::joint::viewer_joint::lan::live_lan_joint_real_loom", "--exact", "--ignored", "--test-threads=1", "--nocapture"])
        .env("OTTER_LOCAL_ART_JOINT_INPUT", &input).stdout(stdout).stderr(stderr).spawn().unwrap());
    let deadline = Instant::now() + Duration::from_secs(30);
    live_lan_phase(&f, &mut child, "lan-ready.json", deadline);
    assert_eq!(fs::read(f.root.join("lan-ready.json")).unwrap(), br#"{"httpsControl":true,"sourceWss":true,"viewers":5,"nativePixel":[17,83,211,255],"nonceReplayRejected":true}"#);
    if direct {
        assert_eq!(
            fs::read(f.root.join("lan-direct-update.json")).unwrap(),
            br#"{"frameId":2,"pixel":[211,83,17,255]}"#
        );
    }
    let snapshot = f.live_sessions.get("live:lan/part").unwrap();
    assert!(snapshot.source_connected);
    assert_eq!(snapshot.direct_view, direct);
    assert_eq!(snapshot.viewer_connections.get(&f.device), Some(&5));
    let (status, response) = admin(
        f.port,
        "PUT",
        &format!("/v1/devices/{}", f.device),
        Some(
            json!({"name":"LAN native viewer","kind":"computer","address":"127.0.0.1","enabled":false}),
        ),
    );
    assert_eq!(status, 200, "{response}");
    fs::write(f.root.join("lan-viewer-revoked"), b"revoked").unwrap();
    live_lan_phase(&f, &mut child, "lan-revoked.json", deadline);
    assert_eq!(
        fs::read(f.root.join("lan-revoked.json")).unwrap(),
        br#"{"policyClosed":true,"nativeReclaimed":true,"encodedBytes":0}"#
    );
    live_lan_update(&f, false);
    fs::write(f.root.join("lan-disabled"), b"disabled").unwrap();
    live_lan_phase(&f, &mut child, "lan-stopped.json", deadline);
    live_lan_zero(&f);
    let restarted = live_lan_update(&f, true);
    fs::write(
        f.root.join("lan-restarted.json"),
        serde_json::to_vec(&restarted["endpoint"]).unwrap(),
    )
    .unwrap();
    live_lan_phase(&f, &mut child, "lan-restart-ready.json", deadline);
    live_lan_update(&f, false);
    fs::write(f.root.join("lan-final-stop"), b"stopped").unwrap();
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success(), "LAN child failure: {}", f.root.display());
            break;
        }
        assert!(Instant::now() < deadline, "LAN child reap deadline");
        thread::sleep(Duration::from_millis(10));
    }
    live_lan_zero(&f);
    assert_eq!(
        fs::read(f.root.join("lan-complete.json")).unwrap(),
        br#"{"fullMediaTested":true,"physicalCrossDevice":false,"ordinaryDualProduct":false}"#
    );
    if let Some(output) = std::env::var_os("OTTER_LIVE_LAN_EVIDENCE_DIR") {
        let output = PathBuf::from(output);
        assert!(output.is_absolute());
        fs::create_dir(&output).unwrap();
        fs::write(output.join("oracle.json"), serde_json::to_vec_pretty(&json!({
            "endpoint":lan["endpoint"],"restartedEndpoint":restarted["endpoint"],
            "directViewer":direct,"rawWireFixture":true,"nativeGpuViewer":true,
            "revokedViewerRejectedAfterRestart":true,"mediaPermits":0,
            "ordinaryDualProduct":false,"physicalCrossDevice":false
        })).unwrap()).unwrap();
        if direct {
            fs::copy(
                f.root.join("lan-direct-update.json"),
                output.join("lan-direct-update.json"),
            )
            .unwrap();
        }
        for name in [
            "lan-ready.json",
            "lan-revoked.json",
            "lan-stopped.json",
            "lan-restart-ready.json",
            "lan-complete.json",
            "lan.stdout.log",
            "lan.stderr.log",
        ] {
            fs::copy(f.root.join(name), output.join(name)).unwrap();
        }
    }
}
