#[test]
fn projection_edit_shared_and_offline_targets_use_one_authority_and_recover() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let ra = ProjectionRoot::new(); let rb = ProjectionRoot::new();
    let (a, mut sa) = start(&ra.0); let (b, mut sb) = start(&rb.0);
    let va = peer_view(a); let vb = peer_view(b); trust_peer(a, &vb, b); trust_peer(b, &va, a);
    let mut source = Identity::pair(a, "Edit source");
    let local = Identity::pair(a, "Local target");
    let mut remote = Identity::pair(b, "Remote target");
    let outsider = Identity::pair(b, "Other target");
    assert_eq!(remote.offline(b, "inbox", json!({"policy":"auto"})).0, 200);
    let snapshot = png(24);
    let shared = source.invitation(&snapshot, unix_time_millis() + 270_000);
    let peer = source.invitation(&snapshot, unix_time_millis() + 270_000);
    assert_eq!(source.post(a, "create", json!({"envelope":shared,"snapshot":snapshot})).0, 200);
    assert_eq!(local.post(a, "accept", acceptance(&shared, "local-unit")).0, 200);
    assert_eq!(source.offline(a, "create", offline_create(&source, &remote, &vb, &peer, &snapshot)).0, 200);
    let read = |e: &ProjectionEnvelope| json!({"operation":"read","projectionId":e.projection_id});
    assert_eq!(remote.offline(b, "edit", read(&peer)).0, 403);
    assert_eq!(remote.offline(b, "accept", acceptance(&peer, "remote-unit")).0, 200);
    let session = edit_session_id();
    let init = json!({"operation":"initialize","projectionId":peer.projection_id,"sessionId":session,
        "expectedDigest":peer.content.digest,"objects":{}});
    assert_eq!(source.offline(a, "edit", init.clone()).0, 200);
    assert_eq!(source.post(a, "edit", json!({"operation":"attach","projectionId":shared.projection_id,"sessionId":session})).0, 200);
    let mode = |op: &str, base: u64, value: &str| json!({"operation":"mode","projectionId":peer.projection_id,
        "sessionId":session,"opId":op,"baseModeRevision":base,"mode":value});
    assert_eq!(remote.offline(b, "edit", mode("unauthorized", 1, "two_way")).0, 403);
    assert_eq!(outsider.offline(b, "edit", read(&peer)).0, 403);
    assert_eq!(source.offline(a, "edit", mode("enable", 1, "two_way")).1["revision"], 2);
    let packet = edit_apply(&peer, "remote-edit", 2, 2, vec![edit_change("remote", 10)]);
    let first = remote.offline(b, "edit", packet.clone()); assert_eq!(first.0, 200, "{}", first.1);
    assert_eq!(first.1["revision"], 3);
    // Model a lost acknowledgement: identical packet must not produce another revision.
    assert_eq!(remote.offline(b, "edit", packet.clone()).1["revision"], 3);
    assert_eq!(local.post(a, "edit", edit_apply(&shared, "local-edit", 2, 2, vec![edit_change("local", 20)])).1["revision"], 4);
    assert_eq!(error_code(&remote.offline(b, "edit", edit_apply(&peer, "conflict", 2, 2, vec![edit_change("remote", 99)]))), "projection_edit_object_conflict");
    let view = remote.offline(b, "read", json!({"projectionId":peer.projection_id,"knownRevision":1}));
    assert_eq!(view.0, 200, "{}", view.1);
    assert_eq!(view.1["editing"]["objects"]["local"]["value"]["x"], 20);
    assert_eq!(local.post(a, "read", json!({"projectionId":shared.projection_id,"knownRevision":1})).1["editing"]["revision"], 4);
    let mut forged = packet.clone(); forged["projectionId"] = json!(shared.projection_id);
    assert_eq!(remote.offline(b, "edit", forged).0, 404);
    assert_eq!(error_code(&source.offline(a, "update", update(&peer, 1, &png(25)))), "projection_edit_snapshot_locked");
    assert_eq!(source.offline(a, "edit", mode("disable", 2, "one_way")).1["modeRevision"], 5);
    assert_eq!(error_code(&remote.offline(b, "edit", packet)), "projection_edit_mode_conflict");
    // An edit permission failure must not unlink the otherwise healthy projection.
    assert_eq!(remote.offline(b, "read", json!({"projectionId":peer.projection_id,"knownRevision":1})).0, 200);
    sa.finish().unwrap(); sb.finish().unwrap();
    let mut sa = restart_offline(&ra.0, a); let mut sb = restart_offline(&rb.0, b);
    source.session(a); remote.session(b);
    let restored = remote.offline(b, "edit", read(&peer)); assert_eq!(restored.0, 200, "{}", restored.1);
    assert_eq!(restored.1["objects"]["remote"]["value"]["x"], 10);
    assert_eq!(restored.1["mode"], "one_way");
    assert_eq!(source.offline(a, "edit", init).1["revision"], 5);
    assert_eq!(source.post(a, "unlink", json!({"projectionId":shared.projection_id})).0, 200);
    assert_eq!(remote.offline(b, "edit", read(&peer)).0, 200);
    for enabled in [false, true] {
        assert_eq!(response(http_request(b, "PUT", &format!("/v1/devices/{}", remote.id), Some(&json!({
            "name":"Remote target","kind":"computer","address":"127.0.0.1","enabled":enabled}).to_string()))).0, 200);
    }
    remote.session(b);
    assert_eq!(remote.offline(b, "edit", read(&peer)).0, 403);
    assert_eq!(source.offline(a, "unlink", json!({"projectionId":peer.projection_id})).0, 200);
    assert!(fs::read_dir(ra.0.join("settings").join("projection-edits")).unwrap().next().is_none());
    sa.finish().unwrap(); sb.finish().unwrap();
}

#[test]
fn projection_edit_checkpoint_rotates_history_and_fences_delayed_packets() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let root = ProjectionRoot::new(); let (port, mut server) = start(&root.0);
    let source = Identity::pair(port, "Checkpoint source"); let receiver = Identity::pair(port, "Checkpoint receiver");
    let snapshot = png(26); let envelope = source.invitation(&snapshot, unix_time_millis() + 270_000);
    assert_eq!(source.post(port, "create", json!({"envelope":envelope,"snapshot":snapshot})).0, 200);
    assert_eq!(receiver.post(port, "accept", acceptance(&envelope, "unit")).0, 200);
    assert_eq!(source.post(port, "edit", json!({"operation":"initialize","projectionId":envelope.projection_id,
        "sessionId":edit_session_id(),"expectedDigest":envelope.content.digest,"objects":{}})).0, 200);
    let mut revision = 1; let mut mode = 1;
    let delayed = edit_apply(&envelope, "very-old", 1, 1, vec![edit_change("old", 1)]);
    for cycle in 0..3 {
        for index in 0..200 {
            let request = edit_apply(&envelope, &format!("op-{cycle}-{index}"), revision, mode, vec![edit_change("object", index)]);
            let result = source.post(port, "edit", request); assert_eq!(result.0, 200, "{}", result.1);
            revision += 1;
        }
        let checkpoint = json!({"operation":"checkpoint","projectionId":envelope.projection_id,
            "sessionId":edit_session_id(),"expectedRevision":revision});
        assert_eq!(receiver.post(port, "edit", checkpoint.clone()).0, 403);
        let result = source.post(port, "edit", checkpoint.clone()); assert_eq!(result.0, 200, "{}", result.1);
        revision += 1; mode = revision;
        assert_eq!(result.1["receiptCount"], 0);
        assert_eq!(result.1["checkpointRevision"], revision);
        assert_eq!(source.post(port, "edit", checkpoint).1["revision"], revision);
    }
    assert_eq!(error_code(&source.post(port, "edit", delayed)), "projection_edit_mode_conflict");
    let mut changed_mode = edit_apply(&envelope, "stale-base", 1, mode, vec![edit_change("old", 2)]);
    assert_eq!(error_code(&source.post(port, "edit", changed_mode.clone())), "projection_edit_revision_conflict");
    changed_mode["baseRevision"] = json!(revision);
    assert_eq!(source.post(port, "edit", changed_mode).0, 200);
    server.finish().unwrap();
}
