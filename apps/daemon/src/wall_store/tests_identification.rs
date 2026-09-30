use super::*;
use crate::wall_store::WallIdentificationOutcome::{Applied, Dismissed};
use loom_protocol::wall::{TileDisplayInfo, TileInputCapability};

fn identifying_store(root: &TestRoot) -> (WallStore, String) {
    let store = WallStore::open(&root.0).unwrap();
    let mut data = fixture();
    for (index, mut endpoint) in data.endpoints.into_iter().enumerate() {
        endpoint.display = Some(TileDisplayInfo {
            name: format!("Display {}", index + 1),
            can_identify: true,
        });
        store.register(index as u64, endpoint, None).unwrap();
    }
    data.layout.revision = 3;
    store.put_layout(2, data.layout).unwrap();
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
    (store, lease.lease_id)
}

fn command(store: &WallStore) -> WallIdentification {
    store
        .snapshot(None)
        .unwrap()
        .endpoints
        .remove(0)
        .identification
        .unwrap()
}

#[test]
fn identification_is_lease_owned_volatile_and_does_not_rebase_layouts() {
    let root = TestRoot::new();
    let (store, lease) = identifying_store(&root);
    let before = fs::read(&store.path).unwrap();
    let started = store.identify_endpoint("endpoint-left").unwrap();
    let first = command(&store);
    assert_eq!(started.revision, 3);
    assert_eq!(started.layouts[0].revision, 3);
    assert_eq!(started.endpoints[0].applied_revision, Some(3));
    assert!(!first.applied);
    assert!((1..=10_000).contains(&first.remaining_ms));
    assert!(started.endpoints[1].identification.is_none());
    let deadline = store.lock().unwrap().leases["endpoint-left"]
        .identification
        .as_ref()
        .unwrap()
        .deadline;
    store.identify_endpoint("endpoint-left").unwrap();
    assert_eq!(command(&store).request_id, first.request_id);
    assert_eq!(
        store.lock().unwrap().leases["endpoint-left"]
            .identification
            .as_ref()
            .unwrap()
            .deadline,
        deadline
    );
    store
        .report_identification(
            "endpoint-left",
            "computer-a",
            &lease,
            &first.request_id,
            Applied,
        )
        .unwrap();
    assert!(command(&store).applied);
    assert_eq!(fs::read(&store.path).unwrap(), before);
    assert!(store
        .snapshot(Some("computer-b"))
        .unwrap()
        .endpoints
        .iter()
        .all(|e| e.identification.is_none()));
    drop(store);
    let reopened = WallStore::open(&root.0).unwrap();
    assert!(reopened
        .snapshot(None)
        .unwrap()
        .endpoints
        .iter()
        .all(|e| e.identification.is_none() && !e.online));
    assert_eq!(fs::read(&reopened.path).unwrap(), before);
}

#[test]
fn identification_rejects_unavailable_targets_and_expires_without_reports() {
    let root = TestRoot::new();
    let (store, lease) = identifying_store(&root);
    assert_eq!(
        store.identify_endpoint("endpoint-right").unwrap_err().code,
        "wall_endpoint_offline"
    );
    store.identify_endpoint("endpoint-left").unwrap();
    let first = command(&store).request_id;
    store
        .lock()
        .unwrap()
        .leases
        .get_mut("endpoint-left")
        .unwrap()
        .identification
        .as_mut()
        .unwrap()
        .deadline = Instant::now();
    assert!(store.snapshot(None).unwrap().endpoints[0]
        .identification
        .is_none());
    store.identify_endpoint("endpoint-left").unwrap();
    assert_ne!(command(&store).request_id, first);
    store
        .report_identification("endpoint-left", "computer-a", &lease, &first, Applied)
        .unwrap();
    assert!(!command(&store).applied);
    store
        .set_presentation(3, "living-room", WallPresentationMode::Black)
        .unwrap();
    assert!(store.snapshot(None).unwrap().endpoints[0]
        .identification
        .is_none());
    assert_eq!(
        store.identify_endpoint("endpoint-left").unwrap_err().code,
        "wall_presentation_paused"
    );
    let mut endpoint = store.snapshot(None).unwrap().endpoints.remove(1).endpoint;
    endpoint.display = None;
    store.register(4, endpoint, None).unwrap();
    assert_eq!(
        store.identify_endpoint("endpoint-right").unwrap_err().code,
        "wall_identification_unsupported"
    );
}

#[test]
fn identification_reports_cannot_cross_devices_or_survive_lease_replacement() {
    let root = TestRoot::new();
    let (store, lease) = identifying_store(&root);
    store.identify_endpoint("endpoint-left").unwrap();
    let id = command(&store).request_id;
    assert_eq!(
        store
            .report_identification("endpoint-left", "computer-b", &lease, &id, Applied)
            .unwrap_err()
            .status,
        403
    );
    assert_eq!(
        store
            .report_identification("endpoint-left", "computer-a", "wrong", &id, Applied)
            .unwrap_err()
            .code,
        "wall_lease_invalid"
    );
    store
        .disconnect("endpoint-left", "computer-a", &lease)
        .unwrap();
    let next = store.connect("endpoint-left", "computer-a").unwrap();
    assert!(store.snapshot(None).unwrap().endpoints[0]
        .identification
        .is_none());
    assert!(store
        .report_identification("endpoint-left", "computer-a", &lease, &id, Applied)
        .is_err());
    store
        .report_identification("endpoint-left", "computer-a", &next.lease_id, &id, Applied)
        .unwrap();
    assert!(store.snapshot(None).unwrap().endpoints[0]
        .identification
        .is_none());
}

#[test]
fn identification_blocks_new_input_but_not_media_and_dismissal_restores_authority() {
    let root = TestRoot::new();
    let (store, lease) = identifying_store(&root);
    let binding = WallInputBinding {
        endpoint_id: "endpoint-left".into(),
        lease_id: lease.clone(),
        revision: 3,
    };
    let input = || {
        store.with_input_target(
            "computer-a",
            &binding,
            Some("application"),
            None,
            TileInputCapability::Pointer,
            |_| Ok(()),
        )
    };
    assert!(input().is_ok());
    store.identify_endpoint("endpoint-left").unwrap();
    assert_eq!(input().unwrap_err().code, "wall_endpoint_identifying");
    assert!(store
        .authorize_live("endpoint-left", "computer-a", &lease, 3, "live-1")
        .is_ok());
    store
        .report_identification(
            "endpoint-left",
            "computer-a",
            &lease,
            &command(&store).request_id,
            Dismissed,
        )
        .unwrap();
    assert!(store.snapshot(None).unwrap().endpoints[0]
        .identification
        .is_none());
    assert!(input().is_ok());
    store.identify_endpoint("endpoint-left").unwrap();
    let mut layout = store.snapshot(None).unwrap().layouts.remove(0);
    layout.revision = 4;
    store.put_layout(3, layout).unwrap();
    assert!(store.snapshot(None).unwrap().endpoints[0]
        .identification
        .is_none());
    assert!(
        input().is_err(),
        "identification must not weaken geometry invalidation"
    );
}
