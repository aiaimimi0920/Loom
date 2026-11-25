// Two real Looms plus independent native sender/receiver consumers; no real profile/device or UI activation.
mod offline_native_joint {
    use super::native_joint::{wait, write_once, Otter};
    use super::*;
    mod fixture {
        include!("projection_offline_joint_fixture.rs");
    }
    mod proof {
        include!("projection_offline_joint_proof.rs");
    }
    use fixture::Https;
    mod tls {
        include!("projection_peer_tls_tests.rs");
    }
    #[cfg(windows)]
    mod local_bootstrap {
        include!("projection_local_bootstrap_joint.rs");
    }
    #[cfg(windows)]
    mod edit {
        include!("projection_edit_native_joint.rs");
    }
    #[cfg(windows)]
    mod application_edit {
        include!("projection_edit_application_joint.rs");
    }
    #[cfg(windows)]
    mod product {
        include!("projection_offline_product_joint.rs");
    }

    #[test]
    #[ignore = "explicit isolated two real Looms and native HTTPS sender/receiver children"]
    fn offline_native_two_loom_menu_frame_receipt_updates_reopen_and_stop() {
        run(false);
    }
    #[test]
    #[ignore = "explicit isolated two real Looms and native receiver peer-revocation child"]
    fn offline_native_two_loom_peer_revocation_preserves_last_saved_image() {
        run(true);
    }

    fn run(revoke: bool) {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let exe = fixture::exe();
        let root = PathBuf::from(
            std::env::var_os("OTTER_PROJECTION_JOINT_DIR").expect("explicit unused evidence root"),
        );
        assert!(root.is_absolute());
        assert!(root
            .parent()
            .unwrap()
            .canonicalize()
            .unwrap()
            .starts_with(std::env::temp_dir().canonicalize().unwrap()));
        fs::create_dir(&root).unwrap();
        let da = root.join("loom-a");
        let db = root.join("loom-b");
        fs::create_dir(&da).unwrap();
        fs::create_dir(&db).unwrap();
        let (a, mut sa) = start(&da);
        let (b, mut sb) = start(&db);
        let ha = Https::enable(a);
        let hb = Https::enable(b);
        let va = peer_view(a);
        let vb = peer_view(b);
        trust_peer(a, &vb, b);
        trust_peer(b, &va, a);
        let source = Identity::pair(a, "Native offline sender A");
        let target = Identity::pair(b, "Native offline receiver B");
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
        ha.input(
            &sender_root,
            &source,
            &composite,
            peer_b,
            &ha.origin,
            revoke,
        );
        hb.input(
            &receiver_root,
            &target,
            &target.id,
            peer_a,
            &ha.origin,
            revoke,
        );
        let mut sender = fixture::spawn(&exe, &sender_root, false);
        let created = wait(&sender_root, "created.json", &mut sender);
        let envelope: ProjectionEnvelope =
            serde_json::from_value(created["envelope"].clone()).unwrap();
        assert_eq!(envelope.server_origin, ha.origin);
        proof::verify(&root, &da, &hb, &va, &vb, &target.id, &envelope);
        let mut receiver = fixture::spawn(&exe, &receiver_root, true);
        let displayed = wait(&receiver_root, "displayed.json", &mut receiver);
        assert_eq!(displayed["binding"]["envelope"], created["envelope"]);
        assert_eq!(displayed["saved"], true);
        assert_eq!(displayed["realFrame"], true);
        let first = ha.offline(
            &source,
            "read",
            json!({"projectionId":envelope.projection_id,"knownRevision":0}),
        );
        assert_eq!(first.0, 200, "{first:?}");
        assert_eq!(first.1["delivery"]["status"], "displayed");
        assert_eq!(first.1["revision"], 1);
        assert_eq!(first.1["digest"], created["digest"]);
        // Source reads intentionally return metadata only; obtain the raster from the authorized receiver.
        assert!(first.1["snapshot"].is_null());
        let raster = hb.offline(
            &target,
            "read",
            json!({"projectionId":envelope.projection_id,"knownRevision":0}),
        );
        assert_eq!(raster.0, 200, "{raster:?}");
        assert_eq!(raster.1["digest"], created["digest"]);
        oracle(&root, "first", &raster.1, 32, 32, [0, 0, 255, 255]);
        write_once(&sender_root.join("accepted"), b"native-displayed");
        let updated = wait(&sender_root, "updated.json", &mut sender);
        assert_eq!(updated["envelope"], created["envelope"]);
        let read = hb.offline(
            &target,
            "read",
            json!({"projectionId":envelope.projection_id,"knownRevision":1}),
        );
        assert_eq!(read.0, 200, "{read:?}");
        assert_eq!(read.1["revision"], 2);
        assert_eq!(read.1["digest"], updated["digest"]);
        assert_eq!(read.1["route"], "offline_peer");
        assert_eq!(read.1["peerId"], peer_a);
        assert_eq!(read.1["receiverDeviceId"], target.id);
        oracle(&root, "updated", &read.1, 16, 8, [0, 255, 0, 255]);
        write_once(&receiver_root.join("updated"), b"source-revision-2");
        let received = wait(&receiver_root, "updated.json", &mut receiver);
        assert_eq!(received["reopened"], true);
        assert_eq!(received["binding"]["digest"], updated["digest"]);
        write_once(&sender_root.join("read-verified"), b"native-reopened");
        if revoke {
            let revoked = peer_admin(
                b,
                "DELETE",
                "/v1/projection-peers",
                json!({"expectedRevision":peer_view(b)["revision"],"peerId":peer_a}),
            );
            assert_eq!(revoked.0, 200);
            // Re-adding trust cannot revive this persisted transfer's old trust revision.
            trust_peer(b, &va, a);
            let rejected = hb.offline(
                &target,
                "read",
                json!({"projectionId":envelope.projection_id,"knownRevision":2}),
            );
            assert_eq!(rejected.0, 403);
            assert_eq!(error_code(&rejected), "projection_peer_revoked");
            write_once(&sender_root.join("peer-revoked"), b"old-transfer-revoked");
            let preserved = wait(&sender_root, "revoked.json", &mut sender);
            assert_eq!(preserved["localContentPreserved"], true);
            assert_eq!(preserved["revision"], 2);
        } else {
            let stopped = wait(&sender_root, "stopped.json", &mut sender);
            assert_eq!(stopped["restoredReceipt"], true);
            let gone = hb.offline(
                &target,
                "read",
                json!({"projectionId":envelope.projection_id,"knownRevision":2}),
            );
            assert_eq!(gone.0, 410);
            assert_eq!(error_code(&gone), "projection_unlinked");
        }
        write_once(&receiver_root.join("terminal"), b"remote-terminal");
        let terminal = wait(&receiver_root, "terminal.json", &mut receiver);
        assert_eq!(terminal["lastImagePreserved"], true);
        assert_eq!(terminal["reopened"], true);
        assert_eq!(terminal["binding"]["envelope"], created["envelope"]);
        assert_eq!(terminal["binding"]["revision"], 2);
        fixture::finish(&mut receiver);
        fixture::finish(&mut sender);
        sa.finish().unwrap();
        sb.finish().unwrap();
        write_once(&root.join("result.json"),&serde_json::to_vec(&json!({"twoRealLooms":true,"nativeSender":true,"nativeReceiver":true,
            "otterIngressHttps":true,"peerTransport":"signed-https-approved-root","realFrameAfterCheckpoint":true,"independentPngPixels":true,
            "receiverReopened":true,"peerRevoked":revoke,"ordinaryProductExe":false,"crossMachine":false})).unwrap());
    }
    fn oracle(root: &Path, name: &str, value: &Value, width: u32, height: u32, rgba: [u8; 4]) {
        let bytes = BASE64
            .decode(value["snapshot"]["imageBase64"].as_str().unwrap())
            .unwrap();
        assert_eq!(sha256_bytes(&bytes), value["digest"]);
        assert_eq!(value["snapshot"]["width"], width);
        assert_eq!(value["snapshot"]["height"], height);
        let image = image::load_from_memory(&bytes).unwrap().to_rgba8();
        assert_eq!(image.dimensions(), (width, height));
        assert!(image.pixels().all(|pixel| pixel.0 == rgba));
        write_once(&root.join(format!("{name}.png")), &bytes);
        write_once(
            &root.join(format!("{name}.json")),
            &serde_json::to_vec(
                &json!({"width":width,"height":height,"digest":value["digest"],"rgba":rgba}),
            )
            .unwrap(),
        );
    }
}
