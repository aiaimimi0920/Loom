// Persistence projection, recovery validation, bounded I/O, and patch durability coverage.
#[test]
fn persistent_instance_round_trips_and_temporary_instance_does_not() {
    let path = temp_path("round-trip");
    let persistent_id;
    {
        let mut store = SurfaceInstanceStore::new(&path).expect("open store");
        persistent_id = create(&mut store).descriptor.instance_id;
        store
            .create(
                "neuro.official/temporary",
                "0.1.0",
                &"b".repeat(64),
                1,
                SurfaceInstancePersistence::Temporary,
                SurfaceInstanceMode::Independent,
            )
            .expect("create temporary instance");
        assert_eq!(store.list().len(), 2);
    }
    let store = SurfaceInstanceStore::new(&path).expect("reload store");
    assert!(store.get(&persistent_id).is_some());
    assert_eq!(store.list().len(), 1);
    let _ = fs::remove_dir_all(path.parent().expect("parent"));
}

#[test]
fn confirmation_is_persisted_identity_bound_and_queues_only_after_approval() {
    let path = temp_path("confirmation");
    let mut store = SurfaceInstanceStore::new(&path).expect("open store");
    let record = create(&mut store);
    let attachment = store
        .attach(
            &record.descriptor.instance_id,
            "hook-node:stock",
            "device-000-local",
            None,
        )
        .expect("attach Surface");
    store
        .put_snapshot(
            &record.descriptor.instance_id,
            snapshot(&record, &attachment.descriptor.attachment_id),
        )
        .expect("mount Surface");
    let event = SurfaceEvent {
        protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
        instance_id: record.descriptor.instance_id.clone(),
        attachment_id: attachment.descriptor.attachment_id.clone(),
        event_id: "event:confirm-one".to_owned(),
        node_id: "price".to_owned(),
        event: "click".to_owned(),
        action: Some("refresh".to_owned()),
        class: SurfaceEventClass::Discrete,
        generation: 0,
        base_revision: 1,
        payload: serde_json::json!({"symbol": "MSFT"}),
    };
    let (ack, request) = store
        .await_confirmation(
            &record.descriptor.instance_id,
            event.clone(),
            SurfaceActionRisk::High,
        )
        .expect("await confirmation");
    assert_eq!(ack.status, SurfaceActionStatus::AwaitingConfirmation);
    assert!(store.pending_events().is_empty());
    assert_eq!(request.device_id, "device-000-local");
    assert_eq!(request.hook_node_id, "hook-node:stock");
    drop(store);

    let mut store = SurfaceInstanceStore::new(&path).expect("reload confirmation store");
    assert_eq!(store.pending_confirmations(), vec![request.clone()]);
    let mismatched = SurfaceConfirmationDecision {
        protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
        confirmation_id: request.confirmation_id.clone(),
        instance_id: request.instance_id.clone(),
        attachment_id: request.attachment_id.clone(),
        device_id: "device-000-other".to_owned(),
        approved: true,
    };
    assert!(matches!(
        store.resolve_confirmation(mismatched),
        Err(SurfaceStoreError::Invalid(_))
    ));
    assert_eq!(store.pending_confirmations(), vec![request.clone()]);

    let approved = store
        .resolve_confirmation(SurfaceConfirmationDecision {
            protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
            confirmation_id: request.confirmation_id,
            instance_id: request.instance_id,
            attachment_id: request.attachment_id,
            device_id: request.device_id,
            approved: true,
        })
        .expect("approve confirmation");
    let SurfaceConfirmationResolution::Approved {
        event: approved_event,
        ack: approved_ack,
    } = approved
    else {
        panic!("expected approved confirmation")
    };
    assert_eq!(approved_event, event);
    assert_eq!(approved_ack.status, SurfaceActionStatus::Queued);
    assert_eq!(store.pending_events(), vec![approved_event]);
    assert!(store.pending_confirmations().is_empty());
    let _ = fs::remove_dir_all(path.parent().expect("parent"));
}

#[test]
fn invalid_store_json_is_not_silently_overwritten() {
    let path = temp_path("invalid-json");
    fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
    fs::write(&path, b"{broken").expect("write broken JSON");
    assert!(matches!(
        SurfaceInstanceStore::new(&path),
        Err(SurfaceStoreError::Json(_))
    ));
    assert_eq!(fs::read(&path).expect("read original"), b"{broken");
    let _ = fs::remove_dir_all(path.parent().expect("parent"));
}

#[test]
fn persisted_store_io_is_bounded_before_parse_and_during_serialization() {
    let path = temp_path("bounded-io");
    fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
    fs::write(&path, b"0123456789").expect("write oversized fixture");

    let bytes = read_surface_store_bytes_with_limit(&path, 4).expect("bounded read");
    assert_eq!(bytes.len(), 5, "reader keeps only one detection byte");

    let mut store =
        SurfaceInstanceStore::new(path.with_file_name("fresh.json")).expect("open empty store");
    create(&mut store);
    let error = document_bytes_with_limit(&store.instances, 16)
        .expect_err("bounded writer must reject an oversized document");
    assert!(error.to_string().contains("byte limit"));
    let _ = fs::remove_dir_all(path.parent().expect("parent"));
}

#[test]
fn deeply_nested_persisted_state_is_rejected() {
    let path = temp_path("deep-state");
    let mut store = SurfaceInstanceStore::new(&path).expect("open store");
    let instance_id = create(&mut store).descriptor.instance_id;
    drop(store);

    let mut root: Value =
        serde_json::from_slice(&fs::read(&path).expect("read store")).expect("parse store");
    let mut nested = Value::Null;
    for _ in 0..=MAX_SURFACE_STORE_JSON_DEPTH {
        nested = Value::Array(vec![nested]);
    }
    root["instances"][instance_id.as_str()]["authoritativeState"] = nested;
    fs::write(&path, serde_json::to_vec(&root).expect("encode deep store"))
        .expect("write deep store");

    let error = match SurfaceInstanceStore::new(&path) {
        Ok(_) => panic!("deep store must fail"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        SurfaceStoreError::Io(ref error) if error.kind() == std::io::ErrorKind::InvalidData
    ));
    let _ = fs::remove_dir_all(path.parent().expect("parent"));
}

#[test]
fn persisted_json_nesting_is_bounded_before_parse() {
    let path = temp_path("preparse-depth");
    fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
    fs::write(&path, "[".repeat(MAX_SURFACE_STORE_JSON_DEPTH + 2))
        .expect("write deeply nested invalid JSON");

    let error = match SurfaceInstanceStore::new(&path) {
        Ok(_) => panic!("pre-parse nesting limit must fail"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        SurfaceStoreError::Io(ref error) if error.kind() == std::io::ErrorKind::InvalidData
    ));
    let _ = fs::remove_dir_all(path.parent().expect("parent"));
}

#[test]
fn persisted_document_rejects_temporary_instances() {
    let path = temp_path("temporary-in-persisted-document");
    let mut store = SurfaceInstanceStore::new(&path).expect("open store");
    let instance_id = create(&mut store).descriptor.instance_id;
    store
        .instances
        .get_mut(&instance_id)
        .expect("stored instance")
        .descriptor
        .persistence = SurfaceInstancePersistence::Temporary;
    let document = SurfaceStoreDocument {
        schema_version: SURFACE_STORE_SCHEMA_VERSION,
        instances: store.instances.clone(),
    };
    fs::write(
        &path,
        serde_json::to_vec(&document).expect("encode invalid persisted document"),
    )
    .expect("write invalid persisted document");

    let error = match SurfaceInstanceStore::new(&path) {
        Ok(_) => panic!("persisted temporary instance must fail"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        SurfaceStoreError::Io(ref error) if error.kind() == std::io::ErrorKind::InvalidData
    ));
    let _ = fs::remove_dir_all(path.parent().expect("parent"));
}

#[test]
fn persisted_instance_map_key_must_match_its_descriptor() {
    let path = temp_path("mismatched-instance-key");
    let mut store = SurfaceInstanceStore::new(&path).expect("open store");
    let instance_id = create(&mut store).descriptor.instance_id;
    drop(store);

    let mut root: Value =
        serde_json::from_slice(&fs::read(&path).expect("read store")).expect("parse store");
    let instances = root["instances"].as_object_mut().expect("instances object");
    let record = instances.remove(&instance_id).expect("stored instance");
    instances.insert("instance:forged".to_owned(), record);
    fs::write(
        &path,
        serde_json::to_vec(&root).expect("encode forged store"),
    )
    .expect("write forged store");

    let error = match SurfaceInstanceStore::new(&path) {
        Ok(_) => panic!("mismatched key must fail"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        SurfaceStoreError::Io(ref error) if error.kind() == std::io::ErrorKind::InvalidData
    ));
    let _ = fs::remove_dir_all(path.parent().expect("parent"));
}

#[test]
fn recovered_pending_event_count_cannot_bypass_runtime_quota() {
    let path = temp_path("recovery-event-quota");
    let mut store = SurfaceInstanceStore::new(&path).expect("open store");
    let instance_id = create(&mut store).descriptor.instance_id;
    let event = SurfaceEvent {
        protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
        instance_id: instance_id.clone(),
        attachment_id: "attachment:recovery".to_owned(),
        event_id: "event:recovery".to_owned(),
        node_id: "node:recovery".to_owned(),
        event: "click".to_owned(),
        action: Some("refresh".to_owned()),
        class: SurfaceEventClass::Discrete,
        generation: 0,
        base_revision: 0,
        payload: Value::Null,
    };
    store
        .instances
        .get_mut(&instance_id)
        .expect("stored instance")
        .pending_events = vec![event; MAX_PENDING_SURFACE_EVENTS + 1];

    let error = validate_loaded_instances(&store.instances)
        .expect_err("recovery must enforce the pending event quota");
    assert!(matches!(
        error,
        SurfaceStoreError::Io(ref error) if error.kind() == std::io::ErrorKind::InvalidData
    ));
    let _ = fs::remove_dir_all(path.parent().expect("parent"));
}

#[test]
fn patch_requires_current_revision_and_updates_recoverable_snapshot() {
    let path = temp_path("patch");
    let mut store = SurfaceInstanceStore::new(&path).expect("open store");
    let record = create(&mut store);
    let attachment = store
        .attach(
            &record.descriptor.instance_id,
            "hook-node:1",
            "device:1",
            None,
        )
        .expect("attach");
    store
        .put_snapshot(
            &record.descriptor.instance_id,
            snapshot(&record, &attachment.descriptor.attachment_id),
        )
        .expect("put snapshot");
    let patch = SurfacePatch {
        protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
        instance_id: record.descriptor.instance_id.clone(),
        attachment_id: attachment.descriptor.attachment_id,
        base_revision: 1,
        revision: 2,
        operations: vec![SurfacePatchOperation::Set {
            node_id: "price".to_owned(),
            path: "/props/text".to_owned(),
            value: serde_json::json!("101"),
        }],
        state_patch: serde_json::json!({"price": 101}),
        resources: Vec::new(),
        resource_leases: Vec::new(),
    };
    store
        .apply_patch(&record.descriptor.instance_id, patch.clone())
        .expect("apply patch");
    assert!(matches!(
        store.apply_patch(&record.descriptor.instance_id, patch),
        Err(SurfaceStoreError::Conflict(_))
    ));
    let reloaded = SurfaceInstanceStore::new(&path).expect("reload store");
    let record = reloaded
        .get(&record.descriptor.instance_id)
        .expect("stored record");
    let snapshot = record
        .attachments
        .values()
        .next()
        .and_then(|attachment| attachment.snapshot.as_ref())
        .expect("stored snapshot");
    assert_eq!(snapshot.revision, 2);
    assert_eq!(snapshot.scene.children[0].props["text"], "101");
    assert_eq!(record.authoritative_state["price"], 101);
    let _ = fs::remove_dir_all(path.parent().expect("parent"));
}
