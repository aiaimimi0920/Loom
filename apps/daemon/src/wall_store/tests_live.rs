#[test]
fn wall_live_grant_tracks_visibility_revision_owner_and_presenter_lifetime() {
    let root = TestRoot::new();
    let store = WallStore::open(&root.0).unwrap();
    let mut layout = populate(&store);
    let lease = store.connect("endpoint-left", "computer-a").unwrap();
    let grant = |actor, lease_id, revision, source| store.authorize_live("endpoint-left", actor, lease_id, revision, source);
    assert!(grant("computer-a", &lease.lease_id, 3, "live-1").is_ok());
    assert!(grant("computer-b", &lease.lease_id, 3, "live-1").is_err());
    assert!(grant("computer-a", "wrong", 3, "live-1").is_err());
    assert!(grant("computer-a", &lease.lease_id, 2, "live-1").is_err());
    assert!(grant("computer-a", &lease.lease_id, 3, "other").is_err());
    layout.revision = 4;
    layout.placements[0].rect.x = 0.0;
    layout.placements[0].rect.width = 100.0;
    store.put_layout(3, layout.clone()).unwrap();
    assert!(grant("computer-a", &lease.lease_id, 3, "live-1").is_err());
    assert!(grant("computer-a", &lease.lease_id, 4, "live-1").is_err(), "edge contact does not grant media");
    layout.revision = 5;
    layout.placements[0].rect.x = -100.0;
    store.put_layout(4, layout).unwrap();
    assert!(grant("computer-a", &lease.lease_id, 5, "live-1").is_ok());
    store.disconnect("endpoint-left", "computer-a", &lease.lease_id).unwrap();
    let replacement = store.connect("endpoint-left", "computer-a").unwrap();
    assert!(grant("computer-a", &lease.lease_id, 5, "live-1").is_err());
    assert!(grant("computer-a", &replacement.lease_id, 5, "live-1").is_ok());
    store.lock().unwrap().leases.get_mut("endpoint-left").unwrap().deadline = Instant::now();
    assert!(grant("computer-a", &replacement.lease_id, 5, "live-1").is_err());
}
