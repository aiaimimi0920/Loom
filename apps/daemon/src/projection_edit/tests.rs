use super::*;

struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("loom-projection-edit-{}", uuid::Uuid::new_v4())))
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn document() -> Session {
    Session {
        schema: SCHEMA.into(),
        session_id: format!("edit:{}", "1".repeat(32)),
        source_device_id: "source".into(),
        source_unit_id: "unit".into(),
        source_epoch: 1,
        initialization_digest: "a".repeat(64),
        basis: Basis {
            digest: "a".repeat(64),
            width: 2,
            height: 2,
        },
        bindings: [format!("projection:{}", "1".repeat(32))].into(),
        offline_bindings: Default::default(),
        revision: 1,
        mode_revision: 1,
        checkpoint_revision: 1,
        mode: Mode::OneWay,
        objects: BTreeMap::new(),
        receipts: BTreeMap::new(),
    }
}

#[test]
fn projection_edit_reclaims_inactive_binding_namespaces_independently() {
    let root = Root::new();
    let mut store = Store::open(root.0.clone()).unwrap();
    let mut session = document();
    let offline = format!("projection:{}", "2".repeat(32));
    session.bindings.insert(offline.clone());
    session.offline_bindings.insert(offline.clone());
    store.commit(session.clone()).unwrap();
    store.prune(&Default::default()).unwrap();
    let mut restored = Store::open(root.0.clone()).unwrap();
    assert_eq!(
        restored.get(&session.session_id).unwrap().bindings,
        [offline.clone()].into()
    );
    restored.prune_offline(&[offline].into()).unwrap();
    assert!(restored.get(&session.session_id).is_ok());
    restored.prune_offline(&Default::default()).unwrap();
    assert!(Store::open(root.0.clone())
        .unwrap()
        .get(&session.session_id)
        .is_err());
}

#[test]
fn projection_edit_failed_commit_does_not_publish_or_consume_operation() {
    let root = Root::new();
    fs::create_dir_all(&root.0).unwrap();
    let path = root.0.join("sessions");
    let mut store = Store::open(path.clone()).unwrap();
    fs::write(&path, b"blocked directory").unwrap();
    let mut session = document();
    session.receipt("op".into(), "source", "b".repeat(64));
    assert_eq!(
        store.commit(session.clone()).unwrap_err().code,
        "projection_edit_storage_failed"
    );
    assert!(store.get(&session.session_id).is_err());
    fs::remove_file(&path).unwrap();
    assert_eq!(
        store.commit(session.clone()).unwrap_err().code,
        "projection_edit_storage_failed"
    );
    let mut store = Store::open(path.clone()).unwrap();
    assert!(store.commit(session.clone()).is_ok());
    let restored = Store::open(path).unwrap();
    assert!(restored
        .get(&session.session_id)
        .unwrap()
        .replay("op", "source", &"b".repeat(64))
        .unwrap());
}

#[test]
fn projection_edit_restart_rejects_corruption_and_duplicate_binding() {
    let root = Root::new();
    let mut store = Store::open(root.0.clone()).unwrap();
    let session = document();
    store.commit(session.clone()).unwrap();
    let file = root.0.join(format!("{}.json", "1".repeat(32)));
    let mut corrupt = serde_json::to_value(&session).unwrap();
    corrupt["unexpected"] = json!(true);
    fs::write(&file, serde_json::to_vec(&corrupt).unwrap()).unwrap();
    assert!(Store::open(root.0.clone()).is_err());
    store.commit(session.clone()).unwrap();
    let mut duplicate = session;
    duplicate.session_id = format!("edit:{}", "2".repeat(32));
    fs::write(
        root.0.join(format!("{}.json", "2".repeat(32))),
        serde_json::to_vec(&duplicate).unwrap(),
    )
    .unwrap();
    assert!(Store::open(root.0.clone()).is_err());
}

#[test]
fn projection_edit_history_and_tombstones_are_bounded_without_eviction() {
    let root = Root::new();
    let mut store = Store::open(root.0.clone()).unwrap();
    let mut session = document();
    for index in 0..256 {
        session.receipt(format!("op-{index}"), "source", "b".repeat(64));
        session.objects.insert(
            format!("object-{index}"),
            Object {
                revision: 1,
                value: None,
            },
        );
    }
    store.commit(session.clone()).unwrap();
    assert!(session.replay("op-0", "source", &"b".repeat(64)).unwrap());
    assert_eq!(
        session
            .replay("new", "source", &"b".repeat(64))
            .unwrap_err()
            .code,
        "projection_edit_log_full"
    );
    session.objects.insert(
        "overflow".into(),
        Object {
            revision: 1,
            value: None,
        },
    );
    assert_eq!(
        store.commit(session.clone()).unwrap_err().code,
        "projection_edit_invalid_document"
    );
    assert_eq!(store.get(&session.session_id).unwrap().objects.len(), 256);
    let active = std::collections::BTreeSet::new();
    store.prune(&active).unwrap();
    assert!(Store::open(root.0.clone())
        .unwrap()
        .get(&session.session_id)
        .is_err());
}

#[test]
fn projection_edit_budget_matches_actual_persisted_encoding() {
    let root = Root::new();
    let mut store = Store::open(root.0.clone()).unwrap();
    let mut session = document();
    for index in 0..64 {
        let id = format!("brush-{index}");
        let value = json!({"id":id,"type":"brush","points":vec![json!({"x":0,"y":0});100]});
        session.objects.insert(
            id,
            Object {
                revision: 1,
                value: Some(value),
            },
        );
    }
    assert!(session.valid());
    assert!(serde_json::to_vec(&session).unwrap().len() < MAX_SESSION_BYTES);
    assert!(serde_json::to_vec_pretty(&session).unwrap().len() > MAX_SESSION_BYTES);
    assert_eq!(
        store.commit(session.clone()).unwrap_err().code,
        "projection_edit_store_full"
    );
    assert!(Store::open(root.0.clone())
        .unwrap()
        .get(&session.session_id)
        .is_err());
}
