use super::*;
use crate::wall_store::{WallPresentationMode::*, WallPresentationOutcome::*};
use loom_protocol::wall::TileInputCapability;

fn applied(revision: u64) -> Option<WallPresentationReport> {
    Some(WallPresentationReport {
        revision,
        outcome: Applied,
    })
}

#[test]
fn display_control_and_resume_do_not_retain_a_previous_scene_receipt() {
    let root = TestRoot::new();
    let store = WallStore::open(&root.0).unwrap();
    let layout = populate(&store);
    let lease = store.connect("endpoint-left", "computer-a").unwrap();
    store
        .heartbeat_with_scene(
            "endpoint-left",
            "computer-a",
            &lease.lease_id,
            1,
            Some(3),
            None,
            Some(WallSceneReport {
                revision: 3,
                prepared: true,
                clock_uncertainty_ms: Some(1),
                applied_at_ms: Some(store.media_timestamp(Instant::now()).unwrap()),
            }),
        )
        .unwrap();
    assert!(store
        .snapshot(None)
        .unwrap()
        .endpoints
        .iter()
        .any(|status| status.scene.is_some()));
    let frozen = store.set_presentation(3, &layout.wall_id, Frozen).unwrap();
    assert!(frozen.endpoints.iter().all(|status| status.scene.is_none()));
    let resumed = store.set_presentation(4, &layout.wall_id, Running).unwrap();
    assert!(resumed
        .endpoints
        .iter()
        .all(|status| status.scene.is_none() && status.applied_revision.is_none()));
}

#[test]
fn pause_preserves_read_grants_but_resume_never_replays_old_input() {
    let root = TestRoot::new();
    let store = WallStore::open(&root.0).unwrap();
    let layout = populate(&store);
    let lease = store.connect("endpoint-left", "computer-a").unwrap();
    let mut binding = WallInputBinding {
        endpoint_id: "endpoint-left".into(),
        lease_id: lease.lease_id.clone(),
        revision: 3,
    };
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
    let input = |binding: &WallInputBinding| {
        store.with_input_target(
            "computer-a",
            binding,
            Some("application"),
            None,
            TileInputCapability::Pointer,
            |_| Ok(()),
        )
    };
    assert!(input(&binding).is_ok());
    let paused = store.set_presentation(3, &layout.wall_id, Frozen).unwrap();
    assert_eq!(paused.layouts, vec![layout.clone()]);
    assert_eq!(
        input(&binding).unwrap_err().code,
        "wall_presentation_paused"
    );
    assert!(store
        .authorize_live("endpoint-left", "computer-a", &lease.lease_id, 3, "live-1")
        .is_ok());
    assert_eq!(
        store
            .set_presentation(4, &layout.wall_id, Frozen)
            .unwrap()
            .revision,
        4
    );
    assert!(store.set_presentation(3, &layout.wall_id, Black).is_err());
    let resumed = store.set_presentation(4, &layout.wall_id, Running).unwrap();
    assert_eq!(resumed.layouts[0].revision, 5);
    assert!(resumed.endpoints[0].applied_revision.is_none());
    assert!(resumed.presentations.is_empty());
    assert!(input(&binding).is_err());
    binding.revision = 5;
    assert!(
        input(&binding).is_err(),
        "new mapping still requires a presentation acknowledgement"
    );
    store
        .heartbeat(
            "endpoint-left",
            "computer-a",
            &lease.lease_id,
            2,
            Some(5),
            None,
        )
        .unwrap();
    assert!(input(&binding).is_ok());
    assert!(serde_json::to_value(resumed)
        .unwrap()
        .get("presentations")
        .is_none());
}

#[test]
fn reports_are_scoped_coherent_and_volatile_while_controls_survive_restart() {
    let root = TestRoot::new();
    let store = WallStore::open(&root.0).unwrap();
    populate(&store);
    let lease = store.connect("endpoint-left", "computer-a").unwrap();
    store.set_presentation(3, "living-room", Frozen).unwrap();
    let durable = fs::read(&store.path).unwrap();
    assert!(store
        .heartbeat(
            "endpoint-left",
            "computer-a",
            &lease.lease_id,
            1,
            Some(3),
            applied(5)
        )
        .is_err());
    assert!(store
        .heartbeat(
            "endpoint-left",
            "computer-a",
            &lease.lease_id,
            1,
            None,
            applied(4)
        )
        .is_err());
    store
        .heartbeat(
            "endpoint-left",
            "computer-a",
            &lease.lease_id,
            1,
            Some(3),
            applied(4),
        )
        .unwrap();
    assert_eq!(
        store.snapshot(Some("computer-a")).unwrap().endpoints[0].presentation,
        applied(4)
    );
    assert!(store
        .snapshot(Some("other-device"))
        .unwrap()
        .presentations
        .is_empty());
    assert_eq!(fs::read(&store.path).unwrap(), durable);
    store.set_presentation(4, "living-room", Black).unwrap();
    assert!(store.snapshot(None).unwrap().endpoints[0]
        .presentation
        .is_none());
    assert!(store
        .heartbeat(
            "endpoint-left",
            "computer-a",
            &lease.lease_id,
            2,
            Some(3),
            applied(4)
        )
        .is_err());
    assert!(store
        .heartbeat(
            "endpoint-left",
            "computer-a",
            &lease.lease_id,
            2,
            Some(3),
            applied(5)
        )
        .is_err());
    store
        .heartbeat(
            "endpoint-left",
            "computer-a",
            &lease.lease_id,
            2,
            None,
            applied(5),
        )
        .unwrap();
    drop(store);
    let store = WallStore::open(&root.0).unwrap();
    let state = store.snapshot(None).unwrap();
    assert_eq!(state.presentations[0].mode, Black);
    assert!(state
        .endpoints
        .iter()
        .all(|endpoint| !endpoint.online && endpoint.presentation.is_none()));
    assert!(store
        .heartbeat(
            "endpoint-left",
            "computer-a",
            &lease.lease_id,
            3,
            None,
            applied(5)
        )
        .is_err());
    assert!(store
        .remove_layout(5, "living-room")
        .unwrap()
        .presentations
        .is_empty());
}

#[test]
fn geometry_changes_while_frozen_invalidate_reports_and_surface_input_only() {
    let root = TestRoot::new();
    let store = WallStore::open(&root.0).unwrap();
    let mut data = fixture();
    data.endpoints[0]
        .render_modes
        .push(loom_protocol::wall::TileRenderMode::SurfaceV1);
    for (index, endpoint) in data.endpoints.into_iter().enumerate() {
        store.register(index as u64, endpoint, None).unwrap();
    }
    data.layout.revision = 3;
    data.layout.placements[0].source =
        loom_protocol::wall::WallContentSource::Surface("form-1".into());
    store.put_layout(2, data.layout.clone()).unwrap();
    let lease = store.connect("endpoint-left", "computer-a").unwrap();
    let binding = WallInputBinding {
        endpoint_id: "endpoint-left".into(),
        lease_id: lease.lease_id.clone(),
        revision: 3,
    };
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
    store.set_presentation(3, "living-room", Frozen).unwrap();
    assert!(store
        .authorize_surface("computer-a", &binding, "form-1", None)
        .is_ok());
    assert_eq!(
        store
            .authorize_surface("computer-a", &binding, "form-1", Some("application"))
            .unwrap_err()
            .code,
        "wall_presentation_paused"
    );
    store
        .heartbeat(
            "endpoint-left",
            "computer-a",
            &lease.lease_id,
            2,
            Some(3),
            applied(4),
        )
        .unwrap();
    data.layout.revision = 5;
    store.put_layout(4, data.layout).unwrap();
    assert!(store.snapshot(None).unwrap().endpoints[0]
        .presentation
        .is_none());
    assert!(store
        .heartbeat(
            "endpoint-left",
            "computer-a",
            &lease.lease_id,
            3,
            Some(3),
            applied(4)
        )
        .is_err());
    let missing = Some(WallPresentationReport {
        revision: 4,
        outcome: FrameUnavailable,
    });
    store
        .heartbeat(
            "endpoint-left",
            "computer-a",
            &lease.lease_id,
            3,
            None,
            missing,
        )
        .unwrap();
    assert_eq!(
        store.snapshot(None).unwrap().endpoints[0].presentation,
        missing
    );
}

#[test]
fn malformed_persisted_controls_fail_closed_without_rewriting_storage() {
    let root = TestRoot::new();
    let store = WallStore::open(&root.0).unwrap();
    populate(&store);
    let original: serde_json::Value =
        serde_json::from_slice(&fs::read(&store.path).unwrap()).unwrap();
    assert!(
        original.get("presentations").is_none(),
        "ordinary documents retain their previous wire shape"
    );
    let path = store.path.clone();
    drop(store);
    let control = serde_json::json!({"wallId": "living-room", "revision": 3, "mode": "frozen"});
    for invalid in [
        serde_json::json!([{ "wallId": "missing", "revision": 3, "mode": "frozen" }]),
        serde_json::json!([{ "wallId": "living-room", "revision": 4, "mode": "frozen" }]),
        serde_json::json!([{ "wallId": "living-room", "revision": 0, "mode": "frozen" }]),
        serde_json::json!([{ "wallId": "living-room", "revision": 3, "mode": "running" }]),
        serde_json::json!([{ "wallId": "living-room", "revision": 3, "mode": "frozen", "extra": true }]),
        serde_json::json!([control.clone(), control]),
    ] {
        let mut document = original.clone();
        document["presentations"] = invalid;
        let bytes = serde_json::to_vec(&document).unwrap();
        fs::write(&path, &bytes).unwrap();
        assert!(WallStore::open(&root.0).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
}
