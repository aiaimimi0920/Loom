// Two real services and native Application owners; independent service authority and PNG oracles.
use super::*;
mod oracle {
    include!("projection_edit_application_joint_oracle.rs");
}
fn spawn(exe: &Path, root: &Path, receiver: bool) -> Otter {
    let test = if receiver {
        "receiver::native_edit_application_receiver_automatic_readonly_resolution_and_reopen"
    } else {
        "source::native_edit_application_source_modes_automatic_checkpoint_and_reopen"
    };
    let test =
        format!("desktop::application::projection_sender::tests::joint::collaboration::{test}");
    let file = |name| {
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join(name))
            .unwrap()
    };
    Otter(
        Command::new(exe)
            .args([
                &test,
                "--exact",
                "--ignored",
                "--test-threads=1",
                "--nocapture",
            ])
            .env("OTTER_PROJECTION_JOINT_INPUT", root.join("input.json"))
            .stdout(file("otter.stdout.log"))
            .stderr(file("otter.stderr.log"))
            .spawn()
            .unwrap(),
    )
}
#[test]
#[ignore = "explicit isolated two real Looms and two native Application edit consumers"]
fn native_edit_applications_modes_automatic_checkpoint_reopen_and_readonly() {
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
    let source = Identity::pair(a, "Native Application edit source");
    let target = Identity::pair(b, "Native Application edit receiver");
    let ha = Https::enable(a);
    let hb = Https::enable(b);
    ha.boundary(&source);
    hb.boundary(&target);
    assert_eq!(target.post(b, "inbox", json!({"policy":"confirm"})).0, 200);
    let peer_a = va["identity"]["peerId"].as_str().unwrap();
    let peer_b = vb["identity"]["peerId"].as_str().unwrap();
    let composite = format!(
        "peer-target:{}",
        sha256_bytes(format!("{peer_b}\n{}", target.id).as_bytes())
    );
    let sender_root = root.join("sender");
    let receiver_root = root.join("receiver");
    ha.input(&sender_root, &source, &composite, peer_b, &ha.origin, false);
    hb.input(
        &receiver_root,
        &target,
        &target.id,
        peer_a,
        &ha.origin,
        false,
    );
    let mut sender = spawn(&exe, &sender_root, false);
    let created = wait(&sender_root, "created.json", &mut sender);
    let envelope: ProjectionEnvelope = serde_json::from_value(created["envelope"].clone()).unwrap();
    let mut receiver = spawn(&exe, &receiver_root, true);
    let displayed = wait(&receiver_root, "displayed.json", &mut receiver);
    assert_eq!(displayed["binding"]["envelope"], created["envelope"]);
    assert_eq!(displayed["realFrame"], true);
    assert_eq!(displayed["receiverNeverActivated"], true);
    write_once(&sender_root.join("accepted"), b"real-receiver-displayed");
    let initialized = wait(&sender_root, "initialized.json", &mut sender);
    let read = hb.offline(
        &target,
        "read",
        json!({"projectionId":envelope.projection_id,"knownRevision":0}),
    );
    assert_eq!(read.0, 200, "{read:?}");
    oracle::basis(&root, &read.1);
    oracle::document(
        (&ha, &source),
        (&hb, &target),
        &envelope,
        &initialized["document"],
        2,
        4.0,
    );
    write_once(
        &receiver_root.join("initialized"),
        b"basis-and-document-verified",
    );
    let attached = wait(&receiver_root, "initialized.json", &mut receiver);
    assert_eq!(attached["document"], initialized["document"]);
    write_once(
        &sender_root.join("receiver-initialized"),
        b"real-receiver-edit-attached",
    );
    let edited = wait(&sender_root, "source-edited.json", &mut sender);
    oracle::document(
        (&ha, &source),
        (&hb, &target),
        &envelope,
        &edited["document"],
        3,
        6.0,
    );
    write_once(
        &receiver_root.join("source-edited"),
        b"real-source-automatic-apply",
    );
    let returned = wait(&receiver_root, "receiver-edited.json", &mut receiver);
    assert_eq!(returned["reopened"], true);
    assert_eq!(returned["defaultCheckpoint"], true);
    oracle::document(
        (&ha, &source),
        (&hb, &target),
        &envelope,
        &returned["document"],
        4,
        9.0,
    );
    oracle::frame(&receiver_root.join("reopened.png"));
    write_once(
        &sender_root.join("receiver-edited"),
        b"real-receiver-automatic-apply-and-reopen",
    );
    let reopened = wait(&sender_root, "reopened.json", &mut sender);
    assert_eq!(reopened["document"], returned["document"]);
    assert_eq!(reopened["reopened"], true);
    oracle::frame(&sender_root.join("reopened.png"));
    write_once(&sender_root.join("switch-one-way"), b"both-reopened");
    let one_way = wait(&sender_root, "one-way.json", &mut sender);
    oracle::document(
        (&ha, &source),
        (&hb, &target),
        &envelope,
        &one_way["document"],
        5,
        9.0,
    );
    write_once(&receiver_root.join("one-way"), b"authoritative-one-way");
    let readonly = wait(&receiver_root, "readonly.json", &mut receiver);
    assert_eq!(readonly["document"], one_way["document"]);
    assert_eq!(readonly["localPreserved"], true);
    assert_eq!(readonly["remoteChosenAndReopened"], true);
    // A local receiver move must not mutate authority while one_way, even after read/resolution/reopen.
    oracle::document(
        (&ha, &source),
        (&hb, &target),
        &envelope,
        &one_way["document"],
        5,
        9.0,
    );
    write_once(
        &sender_root.join("receiver-readonly"),
        b"receiver-cannot-write",
    );
    assert_eq!(
        wait(&sender_root, "stopped.json", &mut sender)["stopped"],
        true
    );
    let stopped = hb.offline(
        &target,
        "read",
        json!({"projectionId":envelope.projection_id,"knownRevision":0}),
    );
    assert_eq!(stopped.0, 410);
    assert_eq!(error_code(&stopped), "projection_unlinked");
    write_once(&receiver_root.join("terminal"), b"real-source-unlink");
    let terminal = wait(&receiver_root, "terminal.json", &mut receiver);
    assert_eq!(terminal["document"], one_way["document"]);
    assert_eq!(terminal["lastContentPreserved"], true);
    oracle::frame(&receiver_root.join("terminal.png"));
    fixture::finish(&mut sender);
    fixture::finish(&mut receiver);
    sa.finish().unwrap();
    sb.finish().unwrap();
    write_once(&root.join("result.json"),&serde_json::to_vec(&json!({"twoRealLooms":true,"twoNativeApplications":true,
        "otterIngressHttps":true,"peerTransport":"signed-loopback-http","automaticSourceAndReceiverApply":true,
        "actualDefaultCheckpointAndAck":true,"bothReopened":true,"realReceiverFrame":true,"oneWayLocalPreserved":true,
        "explicitRemoteResolutionReopened":true,"independentDocumentAndPngOracle":true,"stoppedLastContentPreserved":true,
        "ordinaryProductExe":false,"physicalCrossMachine":false,"automaticServiceBudgetThreshold":false})).unwrap());
}
