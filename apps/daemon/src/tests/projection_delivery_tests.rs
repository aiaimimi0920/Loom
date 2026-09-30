#[test]
fn targeted_delivery_is_private_durable_and_receipted_by_receiver_only() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let root = ProjectionRoot::new();
    let (port, mut server) = start(&root.0);
    let mut source = Identity::pair(port, "Sender");
    let mut target = Identity::pair(port, "Receiver");
    let outsider = Identity::pair(port, "Outsider");
    let snapshot = png(55);
    let envelope = source.invitation(&snapshot, unix_time_millis() + 300_000);
    let create = json!({"envelope": envelope, "snapshot": snapshot, "targetDeviceId": target.id});
    assert_eq!(
        error_code(&source.post(port, "create", create.clone())),
        "projection_target_offline"
    );
    assert_eq!(
        target.post(port, "inbox", json!({"policy": "confirm"})).0,
        200
    );
    let targets = source.post(port, "targets", json!({}));
    assert_eq!(targets.0, 200);
    assert_eq!(targets.1["targets"][0]["deviceId"], target.id);
    assert_eq!(targets.1["capabilities"]["officialRelay"], false);
    assert_eq!(source.post(port, "create", create).0, 200);
    let inbox = target.post(port, "inbox", json!({"policy": "confirm"}));
    assert_eq!(inbox.1["invitations"][0]["envelope"], json!(envelope));
    assert!(inbox.1["invitations"][0].get("snapshot").is_none());
    assert_eq!(
        outsider
            .post(port, "inspect", json!({"envelope": envelope}))
            .0,
        403
    );
    assert_eq!(
        outsider
            .post(port, "accept", acceptance(&envelope, "receiver"))
            .0,
        403
    );
    assert!(
        outsider.post(port, "inbox", json!({"policy": "auto"})).1["invitations"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let displayed = json!({"projectionId": envelope.projection_id, "status": "displayed"});
    assert_eq!(source.post(port, "receipt", displayed.clone()).0, 403);
    assert_eq!(target.post(port, "receipt", displayed.clone()).0, 409);
    assert_eq!(
        target
            .post(port, "accept", acceptance(&envelope, "receiver"))
            .0,
        200
    );
    server.finish().unwrap();
    let (port, mut restarted) = start(&root.0);
    source.session(port);
    target.session(port);
    assert!(source.post(port, "targets", json!({})).1["targets"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(
        target
            .post(port, "accept", acceptance(&envelope, "receiver"))
            .0,
        200
    );
    assert_eq!(
        target.post(port, "inbox", json!({"policy": "confirm"})).1["invitations"][0]["delivery"]
            ["status"],
        "accepted"
    );
    assert_eq!(target.post(port, "receipt", displayed.clone()).0, 200);
    assert_eq!(target.post(port, "receipt", displayed).0, 200);
    assert!(
        target.post(port, "inbox", json!({"policy": "confirm"})).1["invitations"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let read = json!({"projectionId": envelope.projection_id, "knownRevision": 1});
    assert_eq!(
        source.post(port, "read", read).1["delivery"]["status"],
        "displayed"
    );
    let rejected = source.invitation(&snapshot, unix_time_millis() + 300_000);
    assert_eq!(
        source
            .post(
                port,
                "create",
                json!({"envelope": rejected, "snapshot": snapshot, "targetDeviceId": target.id})
            )
            .0,
        200
    );
    let receipt = json!({"projectionId": rejected.projection_id, "status": "rejected"});
    assert_eq!(target.post(port, "receipt", receipt.clone()).0, 200);
    assert_eq!(target.post(port, "receipt", receipt).0, 200);
    assert_eq!(
        target
            .post(port, "accept", acceptance(&rejected, "no-unit"))
            .0,
        409
    );
    assert_eq!(
        target.post(port, "inbox", json!({"policy": "disabled"})).0,
        200
    );
    assert!(source.post(port, "targets", json!({})).1["targets"]
        .as_array()
        .unwrap()
        .is_empty());
    restarted.finish().unwrap();
}
