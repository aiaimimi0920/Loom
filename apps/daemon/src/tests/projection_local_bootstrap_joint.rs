// Fresh native profile uses only Loom's local manifest; no injected identity or CA.
use super::*;
pub(super) fn manifest(root: &Path, port: u16) -> PathBuf {
    let path = root.join("loom.json");
    let token = test_bound_daemon_token(port).expect("bound daemon control token");
    let value = json!({
        "schemaVersion":1, "appId":"loom", "displayName":"Loom", "version":"0.2.2",
        "pid":std::process::id(), "startedAt":unix_time_millis(),
        "transport": {"type":"http", "baseUrl":format!("http://127.0.0.1:{port}"), "auth":"bearer", "authToken":token},
        "capabilities":["ai.chat.v1","loom.runtime.probe.v1"]
    });
    write_once(&path, &serde_json::to_vec(&value).unwrap());
    path
}
#[test]
#[ignore = "explicit current native test EXE and unused isolated evidence directory"]
fn projection_native_local_manifest_bootstrap_enrolls_sends_reopens_and_cancels() {
    run(false);
}
#[test]
#[ignore = "explicit current native test EXE and unused isolated evidence directory"]
fn projection_native_remote_config_bootstrap_enrolls_sends_reopens_and_cancels() {
    run(true);
}
fn run(remote: bool) {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let exe = fixture::exe();
    let root = PathBuf::from(std::env::var_os("OTTER_PROJECTION_JOINT_DIR").unwrap());
    assert!(
        root.is_absolute()
            && root
                .parent()
                .unwrap()
                .canonicalize()
                .unwrap()
                .starts_with(std::env::temp_dir().canonicalize().unwrap())
    );
    fs::create_dir(&root).unwrap();
    let data = root.join("loom");
    fs::create_dir(&data).unwrap();
    let (port, mut server) = start(&data);
    let target = Identity::pair(port, "Bootstrap independent target");
    assert_eq!(
        target.post(port, "inbox", json!({"policy":"confirm"})).0,
        200
    );
    let https = Https::enable(port);
    let initial = response(http_request(port, "GET", "/v1/devices", None)).1;
    let manifest = manifest(&root, port);
    if remote {
        let encoded = https.certificate.as_str().unwrap();
        let pem = format!("-----BEGIN CERTIFICATE-----\n{encoded}\n-----END CERTIFICATE-----\n");
        let info = response(http_request(port, "GET", "/v1/otter/identity", None)).1;
        write_once(
            &root.join("remote-config.json"),
            &serde_json::to_vec(&json!({
                "schemaVersion":1,"origin":https.origin,"loomId":info["loomId"],"certificatePem":pem
            }))
            .unwrap(),
        );
    }
    let output = |name| {
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join(name))
            .unwrap()
    };
    let mut child = Otter(Command::new(exe)
        .args(["desktop::application::projection_sender::tests::local_connection::native_local_loom_bootstrap_and_sender","--exact","--ignored","--test-threads=1","--nocapture"])
        .env("OTTER_LOOM_BOOTSTRAP_ROOT",&root).env("LOOM_MANIFEST_PATH",if remote {root.join("no-local-loom.json")} else {manifest})
        .env("OTTER_NATIVE_PROFILE_DIR",root.join("profile")).env("OTTER_LOOM_BOOTSTRAP_TARGET",&target.id)
        .env_remove("OTTER_LOOM_TLS_ORIGIN").env_remove("OTTER_LOOM_TLS_CA_FILE")
        .env_remove("HOOK_LOOM_TLS_ORIGIN").env_remove("HOOK_LOOM_TLS_CA_FILE")
        .stdout(output("otter.stdout.log")).stderr(output("otter.stderr.log")).spawn().unwrap());
    if remote {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let registry = response(http_request(port, "GET", "/v1/devices", None)).1;
            if let Some(device) = registry["pending"].as_array().unwrap().first() {
                let id = device["id"].as_str().unwrap();
                assert_eq!(
                    response(http_request(
                        port,
                        "POST",
                        &format!("/v1/devices/{id}/approve"),
                        Some("{}")
                    ))
                    .0,
                    200
                );
                break;
            }
            assert!(
                std::time::Instant::now() < until,
                "remote pairing was not requested"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    let created = wait(&root, "created.json", &mut child);
    assert_eq!(created["productionBootstrap"], true);
    assert_eq!(created["automaticTargets"], true);
    assert_eq!(created["automaticInbox"], true);
    let (_, directory) = target.post(port, "targets", json!({}));
    assert!(
        directory["targets"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["deviceId"] == created["deviceId"]),
        "native receiver was not automatically discoverable: {directory}"
    );
    let current = response(http_request(port, "GET", "/v1/devices", None)).1;
    assert_eq!(
        current["devices"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|d| d["isLocal"] != true && d["online"] == true)
            .count(),
        2
    );
    let (status, value) = https.post(
        Some(&target),
        "/v1/projections/inspect",
        json!({"envelope":created["envelope"]}),
    );
    assert_eq!(status, 200);
    assert_eq!(value["digest"], created["digest"]);
    let png = BASE64
        .decode(value["snapshot"]["imageBase64"].as_str().unwrap())
        .unwrap();
    let raster = image::load_from_memory(&png).unwrap().to_rgba8();
    assert_eq!(raster.dimensions(), (32, 32));
    assert!(raster.pixels().all(|pixel| pixel.0 == [0, 0, 255, 255]));
    write_once(&root.join("received.png"), &png);
    write_once(&root.join("read-verified"), b"independent-raster-confirmed");
    assert_eq!(
        wait(&root, "finished.json", &mut child)["sameDeviceAfterReopen"],
        true
    );
    fixture::finish(&mut child);
    let registry = response(http_request(port, "GET", "/v1/devices", None)).1;
    assert_eq!(
        registry["devices"].as_array().unwrap().len(),
        initial["devices"].as_array().unwrap().len() + 1
    );
    assert!(registry["devices"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["id"] == created["deviceId"]));
    assert!(registry["pending"].as_array().unwrap().is_empty());
    server.finish().unwrap();
    write_once(&root.join("result.json"),&serde_json::to_vec(&json!({"productionBootstrap":true,"freshIdentity":true,
        "realHttps":true,"independentPng":true,"sameDeviceAfterReopen":true,"explicitCancel":true,"ordinaryProduct":false,"crossMachine":false})).unwrap());
}
