#[test]
fn targeted_delivery_restart_rejects_corrupt_target_binding() {
    let root = ProjectionRoot::new();
    let path = root.0.join("records");
    let mut store = ProjectionStore::open(path.clone()).unwrap();
    let identity = Identity {
        id: "source".to_owned(),
        key: SigningKey::generate(&mut OsRng),
        token: String::new(),
    };
    let snapshot = png(10);
    let envelope = identity.invitation(&snapshot, 300_001);
    store
        .create(
            envelope.clone(),
            snapshot,
            1,
            1,
            Some(ProjectionDelivery {
                target_device_id: "receiver".to_owned(),
                target_epoch: 2,
                status: DeliveryStatus::AwaitingConfirmation,
            }),
        )
        .unwrap();
    assert!(ProjectionStore::open(path.clone()).is_ok());
    let record = store.get(&envelope.projection_id).unwrap();
    let original = serde_json::to_value(record).unwrap();
    for delivery in [
        json!({"targetDeviceId": "source", "targetEpoch": 2, "status": "awaiting_confirmation"}),
        json!({"targetDeviceId": "receiver", "targetEpoch": 2, "status": "accepted"}),
        json!({"targetDeviceId": "receiver", "targetEpoch": 2, "status": "rejected"}),
    ] {
        let mut corrupted = original.clone();
        corrupted["delivery"] = delivery;
        fs::write(
            store.record_path(&envelope.projection_id),
            serde_json::to_vec(&corrupted).unwrap(),
        )
        .unwrap();
        assert!(ProjectionStore::open(path.clone()).is_err());
    }
}

#[test]
fn targeted_delivery_receiver_epoch_cannot_be_reapproved_into_old_offer() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let root = ProjectionRoot::new();
    let (port, mut server) = start(&root.0);
    let source = Identity::pair(port, "Target source");
    let mut receiver = Identity::pair(port, "Target receiver");
    receiver.post(port, "inbox", json!({"policy": "auto"}));
    let snapshot = png(10);
    let envelope = source.invitation(&snapshot, unix_time_millis() + 300_000);
    assert_eq!(
        source
            .post(
                port,
                "create",
                json!({"envelope": envelope, "snapshot": snapshot, "targetDeviceId": receiver.id})
            )
            .0,
        200
    );
    for enabled in [false, true] {
        let raw = http_request(port, "PUT", &format!("/v1/devices/{}", receiver.id), Some(&json!({
            "name": "Target receiver", "kind": "computer", "address": "127.0.0.1", "enabled": enabled,
        }).to_string()));
        assert_eq!(response(raw).0, 200);
    }
    receiver.session(port);
    assert!(source.post(port, "targets", json!({})).1["targets"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(
        receiver.post(port, "inbox", json!({"policy": "auto"})).1["invitations"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        receiver
            .post(port, "accept", acceptance(&envelope, "receiver"))
            .0,
        403
    );
    assert_eq!(
        receiver
            .post(
                port,
                "receipt",
                json!({"projectionId": envelope.projection_id, "status": "rejected"})
            )
            .0,
        403
    );
    server.finish().unwrap();
}
