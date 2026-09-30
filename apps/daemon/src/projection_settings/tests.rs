use super::*;
use serde_json::json;

fn update(value: serde_json::Value) -> Update {
    serde_json::from_value(value).unwrap()
}
fn root() -> PathBuf {
    std::env::temp_dir().join(format!("loom-projection-settings-{}", uuid::Uuid::new_v4()))
}
fn source(peer_id: Option<String>) -> DeviceRef {
    DeviceRef {
        device_id: "source".into(),
        peer_id,
    }
}
fn fixture(policy: &str) -> serde_json::Value {
    json!({"expectedRevision":0,"groups":[{"groupId":"team","name":"Team","members":[{"deviceId":"source"}]}],
        "rules":[{"deviceId":"receiver","policy":policy,"whitelist":{"devices":[],"groups":[],"users":[]},"blacklist":[]}]})
}

#[test]
fn precedence_namespace_and_unverified_users() {
    let root = root();
    let mut store = ProjectionSettings::open(&root).unwrap();
    let mut input = fixture("reject");
    input["rules"][0]["whitelist"]["groups"] = json!(["team"]);
    input["rules"][0]["whitelist"]["users"] = json!(["friend"]);
    input["rules"][0]["blacklist"] = json!([{"deviceId":"source"}]);
    store.update(update(input)).unwrap();
    assert_eq!(
        store.decision("receiver", &source(None), None).unwrap(),
        Decision::Auto
    );
    let foreign = source(Some(format!("loom-{}", "a".repeat(64))));
    assert_eq!(
        store.decision("receiver", &foreign, None).unwrap(),
        Decision::Reject
    );
    assert_eq!(
        store
            .decision("receiver", &foreign, Some("friend"))
            .unwrap(),
        Decision::Auto
    );
    assert_eq!(
        store.decision("other", &foreign, None).unwrap(),
        Decision::Confirm
    );
    drop(store);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn blacklist_beats_auto_and_whitelist_device_beats_blacklist() {
    let root = root();
    let mut store = ProjectionSettings::open(&root).unwrap();
    let mut input = fixture("auto");
    input["rules"][0]["blacklist"] = json!([{"deviceId":"source"}]);
    store.update(update(input.clone())).unwrap();
    assert_eq!(
        store.decision("receiver", &source(None), None).unwrap(),
        Decision::Reject
    );
    input["expectedRevision"] = json!(1);
    input["rules"][0]["whitelist"]["devices"] = json!([{"deviceId":"source"}]);
    store.update(update(input)).unwrap();
    assert_eq!(
        store.decision("receiver", &source(None), None).unwrap(),
        Decision::Auto
    );
    drop(store);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn revision_lock_and_restart_preserve_rules() {
    let root = root();
    let mut store = ProjectionSettings::open(&root).unwrap();
    assert!(ProjectionSettings::open(&root).is_err());
    store.update(update(fixture("reject"))).unwrap();
    assert_eq!(
        store
            .update(update(fixture("auto")))
            .err()
            .expect("stale write")
            .code,
        "projection_settings_revision_conflict"
    );
    drop(store);
    let reopened = ProjectionSettings::open(&root).unwrap();
    assert_eq!(reopened.view().unwrap().revision, 1);
    assert_eq!(
        reopened.decision("receiver", &source(None), None).unwrap(),
        Decision::Reject
    );
    drop(reopened);
    fs::write(root.join("settings.json"), b"corrupt").unwrap();
    assert!(ProjectionSettings::open(&root).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn malformed_groups_and_capacity_are_rejected_without_committing() {
    let root = root();
    let mut store = ProjectionSettings::open(&root).unwrap();
    let mut missing = fixture("confirm");
    missing["rules"][0]["whitelist"]["groups"] = json!(["missing"]);
    assert!(store.update(update(missing)).is_err());
    let mut duplicate = fixture("confirm");
    duplicate["groups"][0]["members"] = json!([{"deviceId":"source"},{"deviceId":"source"}]);
    assert!(store.update(update(duplicate)).is_err());
    let mut oversized = fixture("auto");
    oversized["groups"] = json!(vec![oversized["groups"][0].clone(); 33]);
    assert!(store.update(update(oversized)).is_err());
    assert_eq!(store.view().unwrap().revision, 0);
    drop(store);
    fs::remove_dir_all(root).unwrap();
}
