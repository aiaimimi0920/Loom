// Public LAN links keep the shared source record authoritative across pinned Loom peers.
#[test]
fn shared_peer_link_import_updates_restarts_and_preserves_one_receiver() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ra = ProjectionRoot::new();
    let rb = ProjectionRoot::new();
    let (a, mut sa) = start(&ra.0);
    let (b, mut sb) = start(&rb.0);
    let va = peer_view(a);
    let vb = peer_view(b);
    trust_peer(a, &vb, b);
    trust_peer(b, &va, a);
    let mut source = Identity::pair(a, "Source");
    let mut target = Identity::pair(b, "Receiver");
    let local = Identity::pair(a, "Local competitor");
    let outsider = Identity::pair(b, "Other receiver");
    let snapshot = png(21);
    let envelope = source.invitation(&snapshot, unix_time_millis() + 270_000);
    assert_eq!(
        source
            .post(
                a,
                "create",
                json!({"envelope":envelope,"snapshot":snapshot})
            )
            .0,
        200
    );
    let import = json!({"sourceLoomId":va["identity"]["peerId"],"envelope":envelope});
    assert_eq!(
        target
            .offline(
                b,
                "import",
                json!({"sourceLoomId":format!("loom-{}", "f".repeat(64)),"envelope":envelope})
            )
            .0,
        403
    );
    let preview = target.offline(b, "import", import.clone());
    assert_eq!(preview.0, 200, "{}", preview.1);
    assert_eq!(preview.1["snapshot"], json!(snapshot));
    assert_eq!(preview.1["route"], "offline_peer");
    assert_eq!(preview.1["delivery"]["targetDeviceId"], target.id);
    assert_eq!(
        local.post(a, "inspect", json!({"envelope":envelope})).0,
        200,
        "preview cannot consume invitation"
    );
    let accepted = target.offline(b, "accept", acceptance(&envelope, "received-unit"));
    assert_eq!(accepted.0, 200, "{}", accepted.1);
    assert_eq!(
        target
            .offline(b, "accept", acceptance(&envelope, "received-unit"))
            .0,
        200
    );
    assert_eq!(
        local
            .post(a, "accept", acceptance(&envelope, "local-unit"))
            .0,
        409
    );
    assert_eq!(outsider.offline(b, "import", import.clone()).0, 403);
    assert_eq!(
        target
            .offline(
                b,
                "receipt",
                json!({"projectionId":envelope.projection_id,"status":"displayed"})
            )
            .0,
        200
    );
    thread::sleep(Duration::from_millis(550));
    assert_eq!(
        source.post(a, "update", update(&envelope, 1, &png(88))).0,
        200
    );
    let read = json!({"projectionId":envelope.projection_id,"knownRevision":1});
    assert_eq!(
        target.offline(b, "read", read.clone()).1["snapshot"],
        json!(png(88))
    );
    sa.finish().unwrap();
    sb.finish().unwrap();
    let mut sa = restart_offline(&ra.0, a);
    let mut sb = restart_offline(&rb.0, b);
    source.session(a);
    target.session(b);
    let restored = target.offline(b, "import", import);
    assert_eq!(restored.0, 200, "{}", restored.1);
    assert_eq!(restored.1["receiverUnitId"], "received-unit");
    assert_eq!(
        target.offline(b, "read", read.clone()).1["snapshot"],
        json!(png(88))
    );
    assert_eq!(
        source
            .post(a, "unlink", json!({"projectionId":envelope.projection_id}))
            .0,
        200
    );
    assert_eq!(target.offline(b, "read", read).0, 410);
    sa.finish().unwrap();
    sb.finish().unwrap();
}

#[test]
fn shared_peer_link_local_and_remote_accept_compete_for_the_same_slot() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ra = ProjectionRoot::new();
    let rb = ProjectionRoot::new();
    let (a, mut sa) = start(&ra.0);
    let (b, mut sb) = start(&rb.0);
    let va = peer_view(a);
    let vb = peer_view(b);
    trust_peer(a, &vb, b);
    trust_peer(b, &va, a);
    let source = Identity::pair(a, "Source");
    let target = Identity::pair(b, "Receiver");
    let local = Identity::pair(a, "Local receiver");
    let snapshot = png(12);
    let envelope = source.invitation(&snapshot, unix_time_millis() + 270_000);
    assert_eq!(
        source
            .post(
                a,
                "create",
                json!({"envelope":envelope,"snapshot":snapshot})
            )
            .0,
        200
    );
    assert_eq!(
        target
            .offline(
                b,
                "import",
                json!({"sourceLoomId":va["identity"]["peerId"],"envelope":envelope})
            )
            .0,
        200
    );
    let results = thread::scope(|scope| {
        let remote = scope.spawn(|| target.offline(b, "accept", acceptance(&envelope, "remote")));
        let here = local.post(a, "accept", acceptance(&envelope, "local"));
        [here.0, remote.join().unwrap().0]
    });
    assert_eq!(
        results.iter().filter(|&&status| status == 200).count(),
        1,
        "{results:?}"
    );
    assert_eq!(
        results.iter().filter(|&&status| status == 409).count(),
        1,
        "{results:?}"
    );
    sa.finish().unwrap();
    sb.finish().unwrap();
}

#[test]
fn shared_peer_link_source_revocation_is_terminal_and_local_import_recovers() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ra = ProjectionRoot::new();
    let rb = ProjectionRoot::new();
    let (a, mut sa) = start(&ra.0);
    let (b, mut sb) = start(&rb.0);
    let va = peer_view(a);
    let vb = peer_view(b);
    trust_peer(a, &vb, b);
    trust_peer(b, &va, a);
    let source = Identity::pair(a, "Source");
    let target = Identity::pair(b, "Receiver");
    let local = Identity::pair(a, "Local receiver");
    let snapshot = png(12);
    let envelope = source.invitation(&snapshot, unix_time_millis() + 270_000);
    assert_eq!(
        source
            .post(
                a,
                "create",
                json!({"envelope":envelope,"snapshot":snapshot})
            )
            .0,
        200
    );
    let import = json!({"sourceLoomId":va["identity"]["peerId"],"envelope":envelope});
    assert_eq!(local.offline(a, "import", import.clone()).0, 200);
    assert_eq!(
        local.post(a, "accept", acceptance(&envelope, "local")).0,
        200
    );
    let restored = local.offline(a, "import", import);
    assert_eq!(restored.0, 200, "{}", restored.1);
    assert_eq!(restored.1["receiverUnitId"], "local");
    let remote = source.invitation(&snapshot, unix_time_millis() + 270_000);
    assert_eq!(
        source
            .post(a, "create", json!({"envelope":remote,"snapshot":snapshot}))
            .0,
        200
    );
    assert_eq!(
        target
            .offline(
                b,
                "import",
                json!({"sourceLoomId":va["identity"]["peerId"],"envelope":remote})
            )
            .0,
        200
    );
    assert_eq!(
        target.offline(b, "accept", acceptance(&remote, "remote")).0,
        200
    );
    assert_eq!(
        response(http_request(
            a,
            "PUT",
            &format!("/v1/devices/{}", source.id),
            Some(
                &json!({
                    "name":"Source", "kind":"computer", "address":"127.0.0.1", "enabled":false
                })
                .to_string()
            )
        ))
        .0,
        200
    );
    let revoked = target.offline(
        b,
        "read",
        json!({"projectionId":remote.projection_id,"knownRevision":0}),
    );
    assert_eq!(revoked.0, 403, "{}", revoked.1);
    assert_eq!(error_code(&revoked), "projection_source_revoked");
    let inbox = target.offline(b, "inbox", json!({"policy":"confirm"}));
    assert_eq!(inbox.0, 200, "{}", inbox.1);
    assert!(inbox.1["invitations"].as_array().unwrap().is_empty());
    sa.finish().unwrap();
    sb.finish().unwrap();
}
