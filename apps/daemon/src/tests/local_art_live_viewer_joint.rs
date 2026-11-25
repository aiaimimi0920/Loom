// Actual HTTPS admission against Otter's native proof client; full media remains a separate gate.
#[test]
#[ignore = "explicit cross-repository HTTPS native Art viewer admission"]
fn local_art_viewer_joint_real_otter_https_and_revocation() {
    let executable = PathBuf::from(std::env::var_os("OTTER_NATIVE_TEST_EXE").unwrap());
    assert!(executable.is_absolute() && executable.is_file());
    let mut f = Fixture::new();
    let key = ed25519_dalek::SigningKey::generate(&mut OsRng);
    let (device_id, token) =
        super::wall_http::pair_with_key(f.port, "Native Art viewer joint", &key);
    f.device = device_id;
    f.token = token;
    f.request["binding"]["deviceId"] = json!(f.device);
    let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = reservation.local_addr().unwrap().port();
    drop(reservation);
    let (status, lan) = admin(
        f.port,
        "PUT",
        "/v1/otter/connections/lan",
        Some(json!({"enabled":true,"host":"127.0.0.1","port":port})),
    );
    assert_eq!(status, 200, "{lan}");
    assert_eq!(lan["listening"], true);
    f.request["binding"]["origin"] = lan["endpoint"].clone();
    assert_eq!(f.approve(0, true).0, 200);
    let (_, info) = admin(f.port, "GET", "/v1/otter/identity", None);
    art_control_session(&f, "live:art-joint", true);
    let (_, snapshot) = admin(f.port, "GET", "/v1/live/sessions/live:art-joint", None);
    let input = f.root.join("viewer-input.json");
    fs::write(&input,serde_json::to_vec(&json!({"request":f.request,"issuer":f.issuer,"certificate":info["certificate"],"snapshot":snapshot,
        "identity":{"schemaVersion":1,"deviceId":f.device,"privateKey":BASE64.encode(key.to_bytes()),"publicKey":BASE64.encode(key.verifying_key().to_bytes())}})).unwrap()).unwrap();
    let stdout = fs::File::create(f.root.join("viewer.stdout.log")).unwrap();
    let stderr = fs::File::create(f.root.join("viewer.stderr.log")).unwrap();
    let mut child=joint::Otter(Command::new(executable).args(["device_session_client::tests::live_control::art_prepare::joint::viewer_joint::art_live_viewer_joint_real_loom","--exact","--ignored","--test-threads=1","--nocapture"]).env("OTTER_LOCAL_ART_JOINT_INPUT",&input).stdout(stdout).stderr(stderr).spawn().unwrap());
    let deadline = Instant::now() + Duration::from_secs(30);
    while !f.root.join("viewer-ready.json").exists() {
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "child exited: {}",
            f.root.display()
        );
        assert!(
            Instant::now() < deadline,
            "child deadline: {}",
            f.root.display()
        );
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        fs::read(f.root.join("viewer-ready.json")).unwrap(),
        br#"{"nativeProof":true,"member":true,"https":true,"nativeControl":true,"inputAccepted":true}"#
    );
    let (status, joined) = device(
        f.port,
        &f.token,
        "GET",
        "/v1/live/sessions/live:art-joint",
        None,
    );
    assert_eq!(status, 200);
    assert_eq!(joined["session"]["viewerDevices"], json!([f.device]));
    assert_eq!(joined["session"]["controllerDevice"], json!(f.device));
    let current = f
        .live_sessions
        .get_for_member("live:art-joint", &f.device)
        .unwrap()
        .requester_control
        .unwrap();
    assert_eq!((current.control_sequence, current.input_sequence), (3, 1));
    assert_eq!(f.approve(1, false).0, 200);
    fs::write(f.root.join("viewer-revoked"), b"revoked").unwrap();
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success(), "child failed: {}", f.root.display());
            break;
        }
        assert!(
            Instant::now() < deadline,
            "child reap deadline: {}",
            f.root.display()
        );
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        fs::read(f.root.join("viewer-complete.json")).unwrap(),
        br#"{"revocationRejected":true,"releaseAfterRevocation":true,"mediaTested":false,"sourceExecutionTested":false}"#
    );
    let current = f
        .live_sessions
        .get_for_member("live:art-joint", &f.device)
        .unwrap();
    assert!(current.session.controller_device.is_none());
    let cursor = current.requester_control.unwrap();
    assert_eq!((cursor.control_sequence, cursor.input_sequence), (4, 1));
    if let Some(output) = std::env::var_os("OTTER_ART_VIEWER_EVIDENCE_DIR") {
        let output = PathBuf::from(output);
        assert!(output.is_absolute());
        fs::create_dir(&output).unwrap();
        for name in [
            "viewer-ready.json",
            "viewer-complete.json",
            "viewer.stdout.log",
            "viewer.stderr.log",
        ] {
            fs::copy(f.root.join(name), output.join(name)).unwrap();
        }
    }
}
