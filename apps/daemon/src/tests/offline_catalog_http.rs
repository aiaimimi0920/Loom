#[test]
fn offline_catalog_is_one_hop_private_and_never_authorizes_a_v1_send() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let roots = [ProjectionRoot::new(), ProjectionRoot::new(), ProjectionRoot::new()];
    let (a, mut sa) = start(&roots[0].0); let (b, mut sb) = start(&roots[1].0); let (c, mut sc) = start(&roots[2].0);
    let va = peer_view(a); let vb = peer_view(b); let vc = peer_view(c);
    trust_peer(a, &vb, b); trust_peer(b, &va, a); trust_peer(b, &vc, c); trust_peer(c, &vb, b);
    let source = Identity::pair(a, "Source A"); let local = Identity::pair(a, "Local A");
    let remote = Identity::pair(b, "Remote B"); let transitive = Identity::pair(c, "Transitive C");
    local.post(a, "inbox", json!({"policy": "confirm"}));
    transitive.post(c, "inbox", json!({"policy": "confirm"}));
    assert_eq!(source.post(a, "targets", json!({})).1["targets"].as_array().unwrap().len(), 1);
    remote.post(b, "inbox", json!({"policy": "auto"}));
    let result = source.post(a, "targets", json!({})); assert_eq!(result.0, 200);
    assert_eq!(result.1["peerDirectory"]["status"], "complete");
    assert_eq!(result.1["capabilities"]["offlinePeerDirectory"], true);
    assert_eq!(result.1["capabilities"]["offlinePeers"], true);
    let targets = result.1["targets"].as_array().unwrap(); assert_eq!(targets.len(), 2);
    assert_eq!(targets[0]["deviceId"], local.id);
    let target = &targets[1];
    assert_eq!(target["route"], "offline_peer"); assert_eq!(target["remoteDeviceId"], remote.id);
    assert_eq!(target["peerId"], vb["identity"]["peerId"]); assert_eq!(target["deliveryAvailable"], true);
    assert!(target.get("publicKey").is_none()); assert!(target.get("origin").is_none());
    assert!(!result.1.to_string().contains("Transitive C"));
    let snapshot = png(10); let envelope = source.invitation(&snapshot, unix_time_millis() + 270_000);
    let create = source.post(a, "create", json!({"envelope": envelope, "snapshot": snapshot, "targetDeviceId": target["deviceId"]}));
    assert_eq!(error_code(&create), "projection_target_unavailable");
    remote.post(b, "inbox", json!({"policy": "disabled"}));
    assert_eq!(source.post(a, "targets", json!({})).1["targets"].as_array().unwrap().len(), 1);
    remote.post(b, "inbox", json!({"policy": "auto"}));
    let revoke = peer_admin(b, "DELETE", "/v1/projection-peers", json!({
        "expectedRevision": peer_view(b)["revision"], "peerId": va["identity"]["peerId"]}));
    assert_eq!(revoke.0, 200);
    let result = source.post(a, "targets", json!({}));
    assert_eq!(result.1["targets"].as_array().unwrap().len(), 1);
    assert_eq!(result.1["peerDirectory"]["status"], "partial");
    sa.finish().unwrap(); sb.finish().unwrap(); sc.finish().unwrap();
}

#[test]
fn offline_catalog_excludes_revoked_device_epoch_and_failed_peer_keeps_local_targets() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let ra = ProjectionRoot::new(); let rb = ProjectionRoot::new();
    let (a, mut sa) = start(&ra.0); let (b, mut sb) = start(&rb.0);
    let va = peer_view(a); let vb = peer_view(b); trust_peer(a, &vb, b); trust_peer(b, &va, a);
    let source = Identity::pair(a, "Source"); let local = Identity::pair(a, "Local"); let remote = Identity::pair(b, "Remote");
    local.post(a, "inbox", json!({"policy": "confirm"})); remote.post(b, "inbox", json!({"policy": "auto"}));
    for enabled in [false, true] {
        assert_eq!(peer_admin(b, "PUT", &format!("/v1/devices/{}", remote.id), json!({
            "name": "Remote", "kind": "computer", "address": "127.0.0.1", "enabled": enabled})).0, 200);
        assert_eq!(source.post(a, "targets", json!({})).1["targets"].as_array().unwrap().len(), 1);
    }
    sb.finish().unwrap();
    let reply = source.post(a, "targets", json!({})); assert_eq!(reply.0, 200);
    assert_eq!(reply.1["targets"][0]["deviceId"], local.id);
    assert_eq!(reply.1["peerDirectory"]["status"], "partial");
    sa.finish().unwrap();
}
