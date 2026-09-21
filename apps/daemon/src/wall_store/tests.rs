use super::*;
include!("tests_live.rs");
#[path = "tests_identification.rs"]
mod identification;
#[path = "tests_presentation.rs"]
mod presentation;

struct TestRoot(PathBuf);

impl TestRoot {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("loom-wall-store-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        // This path was exclusively created by this fixture, never supplied by a caller.
        fs::remove_dir_all(&self.0).expect("clean wall store fixture");
    }
}

#[derive(Deserialize)]
struct Fixture {
    layout: WallLayout,
    endpoints: Vec<TileEndpoint>,
}

fn fixture() -> Fixture {
    serde_json::from_str(include_str!(
        "../../../../protocol/fixtures/wall-geometry.v1.json"
    ))
    .unwrap()
}

fn populate(store: &WallStore) -> WallLayout {
    let mut data = fixture();
    for (index, endpoint) in data.endpoints.into_iter().enumerate() {
        store.register(index as u64, endpoint, None).unwrap();
    }
    data.layout.revision = 3;
    store.put_layout(2, data.layout.clone()).unwrap();
    data.layout
}

#[test]
fn durable_configuration_survives_reopen_without_presence_or_lease_secrets() {
    let root = TestRoot::new();
    let store = WallStore::open(&root.0).unwrap();
    let layout = populate(&store);
    let lease = store.connect("endpoint-left", "computer-a").unwrap();
    let bytes = fs::read(&store.path).unwrap();
    store
        .heartbeat(
            "endpoint-left",
            "computer-a",
            &lease.lease_id,
            1,
            Some(3),
            None,
        )
        .unwrap();
    let online = store.snapshot(None).unwrap();
    assert!(online.endpoints[0].online);
    assert_eq!(online.endpoints[0].applied_revision, Some(3));
    assert_eq!(
        fs::read(&store.path).unwrap(),
        bytes,
        "heartbeats must not write configuration"
    );
    assert!(!String::from_utf8(bytes).unwrap().contains(&lease.lease_id));
    drop(store);
    let reopened = WallStore::open(&root.0).unwrap();
    let snapshot = reopened.snapshot(None).unwrap();
    assert_eq!(snapshot.revision, 3);
    assert_eq!(snapshot.layouts, vec![layout]);
    assert!(snapshot
        .endpoints
        .iter()
        .all(|entry| !entry.online && entry.applied_revision.is_none()));
    assert_eq!(
        reopened
            .heartbeat(
                "endpoint-left",
                "computer-a",
                &lease.lease_id,
                2,
                Some(3),
                None
            )
            .unwrap_err()
            .code,
        "wall_lease_invalid"
    );
}

#[test]
fn catalog_cas_serializes_competing_writers_and_prevents_a_second_store_owner() {
    let root = TestRoot::new();
    let store = Arc::new(WallStore::open(&root.0).unwrap());
    assert!(WallStore::open(&root.0).is_err());
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let workers: Vec<_> = fixture()
        .endpoints
        .into_iter()
        .map(|endpoint| {
            let store = Arc::clone(&store);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                store.register(0, endpoint, None)
            })
        })
        .collect();
    let results: Vec<_> = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect();
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|result| result
                .as_ref()
                .err()
                .is_some_and(|error| error.status == 409))
            .count(),
        1
    );
    let snapshot = store.snapshot(None).unwrap();
    assert_eq!(snapshot.revision, 1);
    assert_eq!(snapshot.endpoints.len(), 1);
    drop(store);
    assert_eq!(
        WallStore::open(&root.0)
            .unwrap()
            .snapshot(None)
            .unwrap()
            .revision,
        1
    );
}

#[test]
fn endpoint_ownership_membership_and_snapshots_are_scoped() {
    let root = TestRoot::new();
    let store = WallStore::open(&root.0).unwrap();
    let layout = populate(&store);
    let own = store.snapshot(Some("computer-a")).unwrap();
    assert_eq!(own.endpoints.len(), 1);
    assert_eq!(own.layouts.len(), 1);
    let other = store.snapshot(Some("outsider")).unwrap();
    assert!(other.endpoints.is_empty() && other.layouts.is_empty());
    let mut stolen = fixture().endpoints.remove(0);
    stolen.device_id = "computer-b".into();
    assert_eq!(
        store
            .register(3, stolen, Some("computer-b"))
            .unwrap_err()
            .status,
        403
    );
    assert_eq!(
        store
            .remove_endpoint(3, "endpoint-left", Some("computer-b"))
            .unwrap_err()
            .status,
        403
    );
    assert_eq!(
        store
            .remove_endpoint(3, "endpoint-left", Some("computer-a"))
            .unwrap_err()
            .status,
        409
    );
    let mut duplicate_wall = layout;
    duplicate_wall.wall_id = "second-wall".into();
    duplicate_wall.revision = 4;
    assert_eq!(store.put_layout(3, duplicate_wall).unwrap_err().status, 409);
    let mut duplicate_output = fixture().endpoints.remove(0);
    duplicate_output.endpoint_id = "alias".into();
    assert_eq!(
        store
            .register(3, duplicate_output, None)
            .unwrap_err()
            .status,
        409
    );
    assert_eq!(store.snapshot(None).unwrap().revision, 3);
}

#[test]
fn layout_changes_and_output_resizing_invalidate_old_mapping_without_rewinding_revisions() {
    let root = TestRoot::new();
    let store = WallStore::open(&root.0).unwrap();
    let mut layout = populate(&store);
    let lease = store.connect("endpoint-left", "computer-a").unwrap();
    store
        .heartbeat(
            "endpoint-left",
            "computer-a",
            &lease.lease_id,
            1,
            Some(3),
            None,
        )
        .unwrap();
    layout.revision = 4;
    store.put_layout(3, layout.clone()).unwrap();
    assert_eq!(
        store.snapshot(None).unwrap().endpoints[0].applied_revision,
        None
    );
    assert_eq!(
        store
            .heartbeat(
                "endpoint-left",
                "computer-a",
                &lease.lease_id,
                2,
                Some(3),
                None
            )
            .unwrap_err()
            .status,
        409
    );
    store
        .heartbeat(
            "endpoint-left",
            "computer-a",
            &lease.lease_id,
            2,
            Some(4),
            None,
        )
        .unwrap();
    let peer = store.connect("endpoint-right", "computer-b").unwrap();
    store
        .heartbeat(
            "endpoint-right",
            "computer-b",
            &peer.lease_id,
            1,
            Some(4),
            None,
        )
        .unwrap();
    let mut endpoint = fixture().endpoints.remove(0);
    endpoint.pixel_size.width = 200;
    let updated = store.register(4, endpoint, Some("computer-a")).unwrap();
    assert_eq!(updated.layouts[0].revision, 5);
    assert!(!updated.endpoints[0].online);
    let peer_status = store.snapshot(Some("computer-b")).unwrap();
    assert!(peer_status.endpoints[0].online);
    assert_eq!(peer_status.endpoints[0].applied_revision, None);
    assert!(store
        .heartbeat(
            "endpoint-left",
            "computer-a",
            &lease.lease_id,
            3,
            Some(5),
            None
        )
        .is_err());
    store.remove_layout(5, &layout.wall_id).unwrap();
    assert!(store.put_layout(3, layout.clone()).is_err());
    layout.revision = 7;
    store.put_layout(6, layout).unwrap();
    assert_eq!(store.snapshot(None).unwrap().layouts[0].revision, 7);
}

#[test]
fn leases_reject_replay_expiry_identity_forgery_and_old_connection_takeover() {
    let root = TestRoot::new();
    let store = WallStore::open(&root.0).unwrap();
    populate(&store);
    let first = store.connect("endpoint-left", "computer-a").unwrap();
    assert!(store.connect("endpoint-left", "computer-a").is_err());
    assert_eq!(
        store
            .connect("endpoint-left", "computer-b")
            .unwrap_err()
            .status,
        403
    );
    assert!(store
        .heartbeat("endpoint-left", "computer-a", "forged", 1, Some(3), None)
        .is_err());
    store
        .heartbeat(
            "endpoint-left",
            "computer-a",
            &first.lease_id,
            1,
            Some(3),
            None,
        )
        .unwrap();
    assert!(store
        .heartbeat(
            "endpoint-left",
            "computer-a",
            &first.lease_id,
            1,
            Some(3),
            None
        )
        .is_err());
    store
        .state
        .lock()
        .unwrap()
        .leases
        .get_mut("endpoint-left")
        .unwrap()
        .deadline = Instant::now() - Duration::from_secs(1);
    assert!(!store.snapshot(None).unwrap().endpoints[0].online);
    assert!(store
        .heartbeat(
            "endpoint-left",
            "computer-a",
            &first.lease_id,
            2,
            Some(3),
            None
        )
        .is_err());
    let second = store.connect("endpoint-left", "computer-a").unwrap();
    assert_ne!(first.lease_id, second.lease_id);
    assert!(store
        .disconnect("endpoint-left", "computer-a", &first.lease_id)
        .is_err());
    store
        .disconnect("endpoint-left", "computer-a", &second.lease_id)
        .unwrap();
    assert!(!store.snapshot(None).unwrap().endpoints[0].online);
}

#[test]
fn corrupt_unknown_duplicate_and_oversized_storage_fails_closed_without_overwrite() {
    let root = TestRoot::new();
    let path = root.0.join("walls.json");
    for bytes in [
        b"{broken".to_vec(),
        br#"{"storageVersion":2,"revision":0,"endpoints":[],"layouts":[]}"#.to_vec(),
        br#"{"storageVersion":1,"revision":0,"endpoints":[],"layouts":[],"unknown":true}"#.to_vec(),
        serde_json::to_vec(&WallDocument {
            storage_version: 1,
            revision: 1,
            endpoints: vec![fixture().endpoints.remove(0); 2],
            layouts: vec![],
            presentations: vec![],
        })
        .unwrap(),
    ] {
        fs::write(&path, &bytes).unwrap();
        assert!(WallStore::open(&root.0).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
    let file = File::create(&path).unwrap();
    file.set_len(MAX_DOCUMENT_BYTES as u64 + 1).unwrap();
    drop(file);
    assert!(WallStore::open(&root.0).is_err());
    assert_eq!(
        fs::metadata(&path).unwrap().len(),
        MAX_DOCUMENT_BYTES as u64 + 1
    );
}

#[test]
fn uncertain_persistence_failure_stops_serving_stale_state_until_reopen() {
    let root = TestRoot::new();
    let store = WallStore::open(&root.0).unwrap();
    populate(&store);
    fs::rename(&store.path, root.0.join("preserved.json")).unwrap();
    fs::create_dir(&store.path).unwrap();
    let error = store.remove_layout(3, "living-room").unwrap_err();
    assert_eq!(error.status, 503);
    assert_eq!(store.snapshot(None).unwrap_err().status, 503);
    assert_eq!(
        store
            .connect("endpoint-left", "computer-a")
            .unwrap_err()
            .status,
        503
    );
    let persisted: WallDocument =
        serde_json::from_slice(&fs::read(root.0.join("preserved.json")).unwrap()).unwrap();
    assert_eq!(persisted.revision, 3);
}
