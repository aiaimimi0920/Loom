// Persistent-projection write elision and confirmation-expiry regression coverage.
/// Bytes that no store write could ever produce, planted over the store file so that the next
/// real write is observable: if the file still holds them, `persist` skipped the filesystem.
const NOT_A_STORE_DOCUMENT: &[u8] = b"not a Surface store document\n";

#[test]
fn a_mutation_that_leaves_the_persistent_projection_alone_writes_nothing() {
    let path = temp_path("persist-skip");
    let mut store = SurfaceInstanceStore::new(&path).expect("open store");
    let persistent = create(&mut store);
    assert!(
        path.exists(),
        "creating a persistent instance must write the store"
    );
    fs::write(&path, NOT_A_STORE_DOCUMENT).expect("plant sentinel bytes");

    let temporary = store
        .create(
            "neuro.official/stock-price",
            "1.2.3",
            &"b".repeat(64),
            1,
            SurfaceInstancePersistence::Temporary,
            SurfaceInstanceMode::Independent,
        )
        .expect("create temporary instance");
    store
        .attach(
            &temporary.descriptor.instance_id,
            "hook-node:stock",
            "device-000-local",
            None,
        )
        .expect("attach to the temporary instance");
    assert_eq!(
        fs::read(&path).expect("read store file"),
        NOT_A_STORE_DOCUMENT,
        "temporary instances are filtered out of the document, so nothing changed on disk"
    );

    let attachment = store
        .attach(
            &persistent.descriptor.instance_id,
            "hook-node:stock",
            "device-000-local",
            None,
        )
        .expect("attach to the persistent instance");
    assert_ne!(
        fs::read(&path).expect("read store file"),
        NOT_A_STORE_DOCUMENT,
        "a change to a persistent instance must still be written"
    );

    let reloaded = SurfaceInstanceStore::new(&path).expect("reload store");
    let record = reloaded
        .get(&persistent.descriptor.instance_id)
        .expect("persistent instance survives the reload");
    assert!(record
        .attachments
        .contains_key(&attachment.descriptor.attachment_id));
    assert!(reloaded.get(&temporary.descriptor.instance_id).is_none());
    let _ = fs::remove_dir_all(path.parent().expect("parent"));
}

#[test]
fn a_store_that_found_no_file_writes_on_its_first_persist() {
    let path = temp_path("first-persist");
    let mut store = SurfaceInstanceStore::new(&path).expect("open store");
    assert!(!path.exists());
    store
        .create(
            "neuro.official/stock-price",
            "1.2.3",
            &"c".repeat(64),
            1,
            SurfaceInstancePersistence::Temporary,
            SurfaceInstanceMode::Independent,
        )
        .expect("create temporary instance");
    assert!(
        path.exists(),
        "the first persist creates the file even when the projection is empty"
    );
    let _ = fs::remove_dir_all(path.parent().expect("parent"));
}

#[test]
fn an_expiry_tick_writes_only_when_something_expired() {
    let path = temp_path("expire-tick");
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
        event_id: "event:expire-one".to_owned(),
        node_id: "price".to_owned(),
        event: "click".to_owned(),
        action: Some("refresh".to_owned()),
        class: SurfaceEventClass::Discrete,
        generation: 0,
        base_revision: 1,
        payload: serde_json::json!({"symbol": "MSFT"}),
    };
    store
        .await_confirmation(
            &record.descriptor.instance_id,
            event.clone(),
            SurfaceActionRisk::High,
        )
        .expect("await confirmation");
    fs::write(&path, NOT_A_STORE_DOCUMENT).expect("plant sentinel bytes");

    assert!(store
        .expire_confirmations()
        .expect("run an idle expiry tick")
        .is_empty());
    assert_eq!(
        fs::read(&path).expect("read store file"),
        NOT_A_STORE_DOCUMENT,
        "a tick with nothing to expire must not write"
    );

    for pending in store
        .instances
        .get_mut(&record.descriptor.instance_id)
        .expect("instance")
        .pending_confirmations
        .values_mut()
    {
        pending.request.expires_at_ms = 1;
    }
    let expired = store.expire_confirmations().expect("run an expiry tick");
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0].status, SurfaceActionStatus::Failed);
    assert_eq!(
        expired[0].error.as_ref().map(|error| error.code.as_str()),
        Some("surface_confirmation_expired")
    );
    assert_ne!(
        fs::read(&path).expect("read store file"),
        NOT_A_STORE_DOCUMENT,
        "an expiry that changed the document must be written"
    );
    assert!(store.pending_confirmations().is_empty());
    let _ = fs::remove_dir_all(path.parent().expect("parent"));
}
