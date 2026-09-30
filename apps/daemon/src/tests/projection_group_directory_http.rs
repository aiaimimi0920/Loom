#[test]
fn projection_group_directory_scopes_members_and_preserves_unavailable_groups() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let ra = ProjectionRoot::new(); let rb = ProjectionRoot::new();
    let (a, mut sa) = start(&ra.0); let (b, mut sb) = start(&rb.0);
    let va = peer_view(a); let vb = peer_view(b); trust_peer(a, &vb, b); trust_peer(b, &va, a);
    let source = Identity::pair(a, "Source"); let local = Identity::pair(a, "Local");
    let remote = Identity::pair(b, "Remote");
    source.post(a, "inbox", json!({"policy":"confirm"}));
    local.post(a, "inbox", json!({"policy":"confirm"}));
    remote.post(b, "inbox", json!({"policy":"auto"}));
    let groups = json!([
        {"groupId":"mixed","name":"Mixed","members":[
            {"deviceId":source.id}, {"deviceId":local.id},
            {"deviceId":remote.id,"peerId":vb["identity"]["peerId"]},
            {"deviceId":remote.id}, {"deviceId":"missing-private-device"}]},
        {"groupId":"overlap","name":"Overlap","members":[{"deviceId":local.id}]}
    ]);
    let put = peer_admin(a, "PUT", "/v1/projection-settings", json!({
        "expectedRevision":0,"groups":groups,"rules":[]
    }));
    assert_eq!(put.0, 200, "{}", put.1);
    let directory = source.post(a, "targets", json!({}));
    assert_eq!(directory.0, 200);
    assert_eq!(directory.1["settingsRevision"], 1);
    let targets = directory.1["targets"].as_array().unwrap();
    let remote_id = &targets.iter().find(|target| target["route"] == "offline_peer").unwrap()["deviceId"];
    assert_eq!(directory.1["groups"][0]["targetIds"], json!([local.id, remote_id]));
    assert_eq!(directory.1["groups"][0]["unavailableCount"], 3);
    assert_eq!(directory.1["groups"][1]["targetIds"], json!([local.id]));
    assert!(directory.1.get("rules").is_none());
    assert!(!directory.1.to_string().contains("missing-private-device"));
    assert_eq!(directory.1["friends"]["status"], "unavailable");
    assert_eq!(directory.1["friends"]["reason"], "official_account_not_implemented");

    local.post(a, "inbox", json!({"policy":"disabled"}));
    remote.post(b, "inbox", json!({"policy":"disabled"}));
    let empty = source.post(a, "targets", json!({}));
    assert_eq!(empty.1["groups"][0]["targetIds"], json!([]));
    assert_eq!(empty.1["groups"][0]["unavailableCount"], 5);
    assert_eq!(empty.1["groups"][1]["unavailableCount"], 1);
    let stored = response(http_request(a, "GET", "/v1/projection-settings", None));
    assert_eq!(stored.1["groups"], groups);
    sa.finish().unwrap(); sb.finish().unwrap();
}
