#[test]
fn projection_http_revocation_epoch_cannot_restore_old_binding() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let root = ProjectionRoot::new();
    let (port, mut server) = start(&root.0);
    let mut a = Identity::pair(port, "Epoch A");
    let mut b = Identity::pair(port, "Epoch B");
    let snapshot = png(10);
    let envelope = a.invitation(&snapshot, unix_time_millis() + 300_000);
    assert_eq!(
        a.post(
            port,
            "create",
            json!({"envelope": envelope, "snapshot": snapshot})
        )
        .0,
        200
    );
    assert_eq!(
        b.post(port, "accept", acceptance(&envelope, "receiver")).0,
        200
    );
    let set_enabled = |identity: &Identity, enabled: bool| {
        let raw = http_request(port, "PUT", &format!("/v1/devices/{}", identity.id), Some(&json!({
            "name": "Epoch device", "kind": "computer", "address": "127.0.0.1", "enabled": enabled,
        }).to_string()));
        assert_eq!(response(raw).0, 200);
    };
    let read = json!({"projectionId": envelope.projection_id, "knownRevision": 1});
    set_enabled(&b, false);
    assert_eq!(b.post(port, "read", read.clone()).0, 401);
    set_enabled(&b, true);
    b.session(port);
    assert_eq!(
        error_code(&b.post(port, "read", read.clone())),
        "projection_access_denied"
    );
    assert_eq!(
        b.post(port, "accept", acceptance(&envelope, "receiver")).0,
        409
    );
    assert_eq!(
        b.post(
            port,
            "unlink",
            json!({"projectionId": envelope.projection_id})
        )
        .0,
        403
    );
    set_enabled(&a, false);
    set_enabled(&a, true);
    a.session(port);
    assert_eq!(
        error_code(&a.post(port, "read", read)),
        "projection_source_revoked"
    );
    assert_eq!(
        a.post(port, "update", update(&envelope, 1, &png(20))).0,
        403
    );
    server.finish().unwrap();
}

#[test]
fn projection_store_bounds_source_count_and_preserves_acceptance_after_invite_expiry() {
    let root = ProjectionRoot::new();
    let path = root.0.join("records");
    let mut store = ProjectionStore::open(path.clone()).unwrap();
    let identity = Identity {
        id: "source".to_owned(),
        key: SigningKey::generate(&mut OsRng),
        token: String::new(),
    };
    let snapshot = png(10);
    let mut first = None;
    for _ in 0..8 {
        let envelope = identity.invitation(&snapshot, 300_001);
        store
            .create(envelope.clone(), snapshot.clone(), 1, 1, None)
            .unwrap();
        first.get_or_insert(envelope);
    }
    assert_eq!(
        store
            .create(
                identity.invitation(&snapshot, 300_001),
                snapshot.clone(),
                1,
                1,
                None
            )
            .unwrap_err()
            .code,
        "projection_source_limit"
    );
    let envelope = first.unwrap();
    store
        .accept(
            &envelope,
            "receiver",
            2,
            "target",
            1,
            &envelope.content.digest,
            2,
        )
        .unwrap();
    let mut restored = ProjectionStore::open(path).unwrap();
    assert_eq!(restored.records.len(), 8);
    assert!(restored
        .accept(
            &envelope,
            "receiver",
            2,
            "target",
            1,
            &envelope.content.digest,
            400_000
        )
        .is_ok());
    assert_eq!(
        restored
            .unlink(&envelope.projection_id, "receiver", 3)
            .unwrap_err()
            .code,
        "projection_access_denied"
    );
    restored
        .create(
            identity.invitation(&snapshot, 600_000),
            snapshot,
            1,
            400_000,
            None,
        )
        .unwrap();
    assert_eq!(restored.records.len(), 2);
}

#[test]
fn projection_http_rejects_oversized_declared_body_without_allocating_it() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let root = ProjectionRoot::new();
    let (port, mut server) = start(&root.0);
    let raw = http_request_with_declared_content_length(
        port,
        "POST",
        "/v1/projections/create",
        loom_protocol::projection::MAX_PROJECTION_HTTP_BYTES + 1,
        None,
    );
    assert_eq!(response(raw).0, 413);
    server.finish().unwrap();
}
