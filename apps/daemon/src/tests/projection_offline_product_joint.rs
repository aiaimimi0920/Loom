// Real ordinary Otter products own menus/lifecycle; this parent audits actual receiver PNGs.
use super::*;
mod process {
    include!("projection_offline_product_process.rs");
}

fn spawn_product(exe: &Path, root: &Path) -> Otter {
    let file = |name| {
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join(name))
            .unwrap()
    };
    Otter(Command::new(exe)
        .args(["offline_joint::ordinary_product_offline_two_loom_send_receive_reset_flip_reopen_and_stop",
            "--exact", "--ignored", "--test-threads=1", "--nocapture"])
        .env("OTTER_OFFLINE_PRODUCT_INPUT_DIR", root)
        .stdout(file("otter.stdout.log")).stderr(file("otter.stderr.log")).spawn().unwrap())
}
fn audit_raster(
    root: &Path,
    name: &str,
    stage: u64,
    value: &Value,
    event: &Value,
    peer: &str,
    target: &str,
) {
    let binding = &event["binding"]["link"];
    assert_eq!(value["envelope"], binding["envelope"]);
    assert_eq!(value["revision"], stage);
    assert_eq!(value["digest"], binding["digest"]);
    assert_eq!(value["route"], "offline_peer");
    assert_eq!(value["peerId"], peer);
    assert_eq!(value["receiverDeviceId"], target);
    assert_eq!(event["received"]["envelope"], binding["envelope"]);
    assert_eq!(event["received"]["revision"], stage);
    assert_eq!(event["received"]["digest"], binding["digest"]);
    let bytes = BASE64
        .decode(value["snapshot"]["imageBase64"].as_str().unwrap())
        .unwrap();
    assert!(bytes.len() <= 64 * 1024);
    assert_eq!(sha256_bytes(&bytes), value["digest"]);
    let (width, height) = match stage {
        1 => (16, 8),
        2 => (32, 16),
        _ => (320, 160),
    };
    assert_eq!(value["snapshot"]["width"], width);
    assert_eq!(value["snapshot"]["height"], height);
    let image = image::load_from_memory(&bytes).unwrap().to_rgba8();
    assert_eq!(image.dimensions(), (width, height));
    // Independent fixed bands, not the Otter renderer or its fixture's expected() implementation.
    let blue_columns = if stage == 1 { 12 } else { 16 };
    for (x, _, pixel) in image.enumerate_pixels() {
        if stage == 3 {
            // The restored Editor samples 32 columns with 10x bilinear upscaling; after materializing flip, that output becomes the active source rather than the prior retained buffer.
            let edge = [
                [242, 0, 13, 255],
                [217, 0, 38, 255],
                [191, 0, 64, 255],
                [166, 0, 89, 255],
                [140, 0, 115, 255],
                [115, 0, 140, 255],
                [89, 0, 166, 255],
                [64, 0, 191, 255],
                [38, 0, 217, 255],
                [13, 0, 242, 255],
            ];
            let expected = if x < 155 {
                [255, 0, 0, 255]
            } else if x >= 165 {
                [0, 0, 255, 255]
            } else {
                edge[(x - 155) as usize]
            };
            assert_eq!(pixel.0, expected, "{name} x={x}");
            continue;
        }
        let blue = x < blue_columns;
        assert_eq!(
            pixel.0,
            if blue {
                [0, 0, 255, 255]
            } else {
                [255, 0, 0, 255]
            },
            "{name} x={x}"
        );
    }
    write_once(&root.join(format!("{name}.png")), &bytes);
    write_once(&root.join(format!("{name}-raster.json")), &serde_json::to_vec(&json!({
        "width":width,"height":height,"digest":value["digest"],"revision":stage,"independentBands":true})).unwrap());
}

#[test]
#[ignore = "explicit pinned ordinary EXEs, two isolated real Looms and owned foreground launcher"]
fn offline_native_two_loom_ordinary_products_menu_edit_reopen_stop() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let exe = fixture::exe();
    // The child pins and locks the ordinary product itself; both launch paths must identify identical bytes.
    for suffix in ["EXE", "SHA256"] {
        assert_eq!(
            std::env::var_os(format!("OTTER_NATIVE_TEST_PRODUCT_{suffix}")),
            std::env::var_os(format!("OTTER_ART_PRODUCT_{suffix}"))
        );
        assert!(std::env::var_os(format!("OTTER_ART_PRODUCT_{suffix}")).is_some());
    }
    let root = PathBuf::from(
        std::env::var_os("OTTER_PROJECTION_JOINT_DIR").expect("explicit unused product joint root"),
    );
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
    let da = root.join("loom-a");
    let db = root.join("loom-b");
    fs::create_dir(&da).unwrap();
    fs::create_dir(&db).unwrap();
    let (a, mut sa) = start(&da);
    let (b, mut sb) = start(&db);
    let va = peer_view(a);
    let vb = peer_view(b);
    trust_peer(a, &vb, b);
    trust_peer(b, &va, a);
    let source = Identity::pair(a, "Ordinary offline sender A");
    let target = Identity::pair(b, "Ordinary offline receiver B");
    assert_eq!(target.post(b, "inbox", json!({"policy":"confirm"})).0, 200);
    let ha = Https::enable(a);
    let hb = Https::enable(b);
    let peer_a = va["identity"]["peerId"].as_str().unwrap();
    let peer_b = vb["identity"]["peerId"].as_str().unwrap();
    let composite = format!(
        "peer-target:{}",
        sha256_bytes(format!("{peer_b}\n{}", target.id).as_bytes())
    );
    ha.input(
        &root.join("sender"),
        &source,
        &composite,
        peer_b,
        &ha.origin,
        false,
    );
    hb.input(
        &root.join("receiver"),
        &target,
        &target.id,
        peer_a,
        &ha.origin,
        false,
    );
    super::local_bootstrap::manifest(&root.join("sender"), a);
    super::local_bootstrap::manifest(&root.join("receiver"), b);
    let mut child = spawn_product(&exe, &root);
    assert_eq!(
        wait(&root, "started.json", &mut child)["ordinaryProducts"],
        true
    );
    let _products = process::Job::attach(&child.0);
    write_once(&root.join("parent-ready"), b"owned-job-ready");
    assert_eq!(
        wait(&root, "sender-authorized.json", &mut child)["deviceSession"],
        true
    );
    assert_eq!(
        wait(&root, "authorized.json", &mut child)["twoDeviceSessions"],
        true
    );
    let mut envelope = Value::Null;
    let mut last_digest = Value::Null;
    for (name, stage) in [("first", 1), ("reset", 2), ("flipped", 3)] {
        let event = wait(&root, &format!("{name}.json"), &mut child);
        if stage == 1 {
            envelope = event["binding"]["link"]["envelope"].clone();
        }
        assert_eq!(event["binding"]["link"]["envelope"], envelope);
        assert_eq!(envelope["serverOrigin"], ha.origin);
        assert_eq!(event["binding"]["target"]["peerId"], peer_b);
        let read = hb.offline(
            &target,
            "read",
            json!({"projectionId":envelope["projectionId"],"knownRevision":0}),
        );
        assert_eq!(read.0, 200, "{read:?}");
        audit_raster(&root, name, stage, &read.1, &event, peer_a, &target.id);
        last_digest = read.1["digest"].clone();
        if stage == 1 {
            let read = ha.offline(
                &source,
                "read",
                json!({"projectionId":envelope["projectionId"],"knownRevision":0}),
            );
            assert_eq!(read.0, 200, "{read:?}");
            assert_eq!(read.1["delivery"]["status"], "displayed");
            assert_eq!(read.1["digest"], last_digest);
            assert!(read.1["snapshot"].is_null());
        }
        write_once(
            &root.join(format!("{name}-audited")),
            b"receiver-pixels-and-digest",
        );
    }
    let reopened = wait(&root, "reopened.json", &mut child);
    assert_eq!(reopened["sameUnit"], true);
    assert_eq!(reopened["binding"]["envelope"], envelope);
    assert_eq!(reopened["binding"]["digest"], last_digest);
    let stopped = wait(&root, "stopped.json", &mut child);
    assert_eq!(stopped["lastImagePreserved"], true);
    assert_eq!(stopped["senderRemoved"], true);
    assert_eq!(stopped["received"]["envelope"], envelope);
    assert_eq!(stopped["received"]["digest"], last_digest);
    let gone = hb.offline(
        &target,
        "read",
        json!({"projectionId":envelope["projectionId"],"knownRevision":3}),
    );
    assert_eq!(gone.0, 410);
    assert_eq!(error_code(&gone), "projection_unlinked");
    write_once(&root.join("stop-audited"), b"durable-unlinked");
    let finished = wait(&root, "finished.json", &mut child);
    for key in [
        "ordinaryProducts",
        "realMenus",
        "cropReset",
        "flipAppliedOnce",
        "receiverReopened",
        "terminalReopened",
        "normalExit",
        "receiverVisible",
        "deliveryDidNotActivate",
    ] {
        assert_eq!(finished[key], true, "{key}");
    }
    fixture::finish(&mut child);
    sa.finish().unwrap();
    sb.finish().unwrap();
    write_once(&root.join("result.json"),&serde_json::to_vec(&json!({"ordinaryOtterExe":true,
        "twoRealLoomServices":true,"ordinaryLoomBusinessExe":false,"realMenus":true,
        "independentReceiverPixels":true,"otterIngressHttps":true,"peerTransport":"signed-loopback-http",
        "cropReset":true,"flipAppliedOnce":true,"receiverReopened":true,"terminalReopened":true,
        "normalExit":true,"crossMachine":false,"finalPackage":false})).unwrap());
}
