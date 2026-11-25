// Real-daemon authority oracle for the native edit transaction; not a product UI or physical peer-TLS claim.
use super::*;
#[test]
#[ignore = "explicit native edit child with two real Looms and isolated pinned HTTPS"]
fn native_edit_transactions_shared_offline_replay_merge_checkpoint_and_conflict() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let exe = fixture::exe();
    let root = PathBuf::from(
        std::env::var_os("OTTER_PROJECTION_JOINT_DIR").expect("explicit unused evidence root"),
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
    let source = Identity::pair(a, "Native edit source");
    let local = Identity::pair(a, "Native edit local");
    let remote = Identity::pair(b, "Native edit remote");
    let ha = Https::enable(a);
    let hb = Https::enable(b);
    let snapshot = png(24);
    // The existing independent fixture is explicitly 2x2, not Otter's unrelated 2x1 PNG fixture.
    let raster = image::load_from_memory(&BASE64.decode(&snapshot.image_base64).unwrap())
        .unwrap()
        .to_rgba8();
    assert_eq!(raster.dimensions(), (2, 2));
    assert!(raster.pixels().all(|p| p.0 == [24, 20, 40, 255]));
    let shared = source.invitation(&snapshot, unix_time_millis() + 270_000);
    let offline = source.invitation(&snapshot, unix_time_millis() + 270_000);
    assert_eq!(
        source
            .post(a, "create", json!({"envelope":shared,"snapshot":snapshot}))
            .0,
        200
    );
    assert_eq!(
        local.post(a, "accept", acceptance(&shared, "local-unit")).0,
        200
    );
    assert_eq!(remote.offline(b, "inbox", json!({"policy":"auto"})).0, 200);
    assert_eq!(
        source
            .offline(
                a,
                "create",
                offline_create(&source, &remote, &vb, &offline, &snapshot)
            )
            .0,
        200
    );
    assert_eq!(
        remote
            .offline(b, "accept", acceptance(&offline, "remote-unit"))
            .0,
        200
    );
    // Only synthetic keys enter this bounded private fixture; no credentials enter command lines/receipts.
    let actor = |https: &Https, identity: &Identity| {
        json!({"origin":https.origin,"certificate":https.certificate,"identity":{
        "schemaVersion":1,"deviceId":identity.id,"privateKey":BASE64.encode(identity.key.to_bytes()),"publicKey":BASE64.encode(identity.key.verifying_key().to_bytes())}})
    };
    write_once(&root.join("input.json"),&serde_json::to_vec(&json!({"source":actor(&ha,&source),"local":actor(&ha,&local),"remote":actor(&hb,&remote),
        "shared":shared.projection_id,"offline":offline.projection_id,"digest":shared.content.digest})).unwrap());
    // Shared passes the common credential fence; offline has its own device-only fence.
    for (https, projection, path, status, code) in [
        (&ha, &shared, "/v1/projections/edit", 401, "unauthorized"),
        (
            &hb,
            &offline,
            "/v1/offline-projections/edit",
            403,
            "projection_device_required",
        ),
    ] {
        let rejected = https.post(
            None,
            path,
            json!({"operation":"read","projectionId":projection.projection_id}),
        );
        assert_eq!(rejected.0, status);
        assert_eq!(error_code(&rejected), code);
    }
    let file = |name| {
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join(name))
            .unwrap()
    };
    let mut child=Otter(Command::new(exe).args(["projection_edit::joint::native_edit_joint_shared_offline_replay_merge_checkpoint_and_conflict","--exact","--ignored","--test-threads=1","--nocapture"])
        .env("OTTER_PROJECTION_EDIT_JOINT_INPUT",root.join("input.json")).stdout(file("otter.stdout.log")).stderr(file("otter.stderr.log")).spawn().unwrap());
    let evidence = wait(&root, "completed.json", &mut child);
    fixture::finish(&mut child);
    assert_eq!(evidence["actualHttps"], true);
    assert_eq!(evidence["conflictRetained"], true);
    assert_eq!(evidence["sameRequestReplayed"], true);
    assert_eq!(evidence["applicationCheckpoint"], false);
    let read = |id: &str| json!({"operation":"read","projectionId":id});
    let actual = source.post(a, "edit", read(&shared.projection_id));
    let forwarded = remote.offline(b, "edit", read(&offline.projection_id));
    assert_eq!(actual.0, 200);
    assert_eq!(forwarded.0, 200);
    assert_eq!(actual.1, forwarded.1);
    assert_eq!(actual.1, evidence["document"]);
    // Independent fixed oracle: do not call native validators, merge, request preparation or expected renderer.
    assert_eq!(actual.1["revision"], 6);
    assert_eq!(actual.1["modeRevision"], 5);
    assert_eq!(actual.1["checkpointRevision"], 5);
    assert_eq!(actual.1["receiptCount"], 1);
    assert_eq!(actual.1["mode"], "two_way");
    assert_eq!(actual.1["objects"].as_object().unwrap().len(), 2);
    assert_eq!(
        actual.1["objects"]["shape-1"],
        json!({"revision":6,"value":null})
    );
    assert_eq!(actual.1["objects"]["local"]["value"]["x"], 20);
    assert_eq!(actual.1["objects"]["local"]["revision"], 4);
    assert_eq!(actual.1["basis"]["digest"], shared.content.digest);
    assert_eq!(actual.1["basis"]["width"], 2);
    assert_eq!(actual.1["basis"]["height"], 2);
    assert!(actual.1.get("receipts").is_none() && actual.1.get("bindings").is_none());
    assert_eq!(
        error_code(&source.post(a, "update", update(&shared, 1, &png(25)))),
        "projection_edit_snapshot_locked"
    );
    sa.finish().unwrap();
    sb.finish().unwrap();
    eprintln!("native-edit-joint: HTTPS-ingress=true real-daemons=2 native-six-operations=true replay=true merge=true tombstone=true conflict-retained=true product-ui=false peer-TLS=false");
}
