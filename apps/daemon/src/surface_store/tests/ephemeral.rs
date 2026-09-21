fn ephemeral_host() -> SurfaceHostCapabilities {
    crate::default_declarative_surface_host_capabilities()
}

#[test]
fn connection_owned_surface_attachments_do_not_survive_restart_or_remove_source_state() {
    let path = temp_path("ephemeral-attachments");
    let mut store = SurfaceInstanceStore::new(&path).unwrap();
    let source = create(&mut store);
    let id = &source.descriptor.instance_id;
    let original = store
        .attach(id, "source-node", "source-device", None)
        .unwrap();
    store
        .put_snapshot(id, snapshot(&source, &original.descriptor.attachment_id))
        .unwrap();
    let view = store
        .attach_ephemeral(id, "wall-node", "tile-device", ephemeral_host())
        .unwrap();
    store
        .put_snapshot(id, snapshot(&source, &view.descriptor.attachment_id))
        .unwrap();
    assert_eq!(store.get(id).unwrap().attachments.len(), 2);
    let recovered = SurfaceInstanceStore::new(&path).unwrap().get(id).unwrap();
    assert_eq!(recovered.attachments.len(), 1);
    assert!(recovered
        .attachments
        .contains_key(&original.descriptor.attachment_id));
    assert_eq!(
        recovered.authoritative_state,
        serde_json::json!({"price":100})
    );
    assert!(store
        .remove_ephemeral_attachment(id, &original.descriptor.attachment_id)
        .is_err());
    store
        .remove_ephemeral_attachment(id, &view.descriptor.attachment_id)
        .unwrap();
    assert_eq!(
        store.get(id).unwrap().authoritative_state,
        recovered.authoritative_state
    );
    fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn connection_owned_surface_capacity_preserves_idempotent_reopen() {
    let path = temp_path("ephemeral-capacity");
    let mut store = SurfaceInstanceStore::new(&path).unwrap();
    let source = create(&mut store);
    let id = &source.descriptor.instance_id;
    let original = store
        .attach(id, "source-node", "source-device", None)
        .unwrap();
    store
        .put_snapshot(id, snapshot(&source, &original.descriptor.attachment_id))
        .unwrap();
    let first = store
        .attach_ephemeral(id, "wall-0", "tile-device", ephemeral_host())
        .unwrap();
    for index in 1..128 {
        store
            .attach_ephemeral(
                id,
                &format!("wall-{index}"),
                "tile-device",
                ephemeral_host(),
            )
            .unwrap();
    }
    let repeated = store
        .attach_ephemeral(id, "wall-0", "tile-device", ephemeral_host())
        .unwrap();
    assert_eq!(
        first.descriptor.attachment_id,
        repeated.descriptor.attachment_id
    );
    assert!(store
        .attach_ephemeral(id, "overflow", "tile-device", ephemeral_host())
        .is_err());
    fs::remove_dir_all(path.parent().unwrap()).unwrap();
}
