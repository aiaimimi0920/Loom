fn peer_admin(port: u16, method: &str, path: &str, body: Value) -> (u16, Value) {
    response(http_request(port, method, path, Some(&body.to_string())))
}

fn peer_view(port: u16) -> Value {
    let reply = peer_admin(port, "GET", "/v1/projection-peers", json!({}));
    assert_eq!(reply.0, 200);
    assert!(!reply.1.to_string().contains("privateKey"));
    assert_eq!(reply.1["deliveryAvailable"], false);
    reply.1
}

fn trust_peer(port: u16, remote: &Value, remote_port: u16) -> Value {
    let view = peer_view(port);
    let reply = peer_admin(port, "PUT", "/v1/projection-peers", json!({
        "expectedRevision": view["revision"], "peer": {
            "peerId": remote["identity"]["peerId"], "publicKey": remote["identity"]["publicKey"],
            "name": "Explicit offline peer", "origin": format!("http://127.0.0.1:{remote_port}"), "enabled": true
        }
    }));
    assert_eq!(reply.0, 200);
    reply.1
}

#[test]
fn offline_peers_http_mutual_identity_restart_and_revocation() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let root_a = ProjectionRoot::new();
    let root_b = ProjectionRoot::new();
    let (a, mut server_a) = start(&root_a.0);
    let (b, mut server_b) = start(&root_b.0);
    let view_a = peer_view(a);
    let view_b = peer_view(b);
    assert!(offline_peers::OfflinePeers::new(&root_a.0).is_err());
    assert_ne!(view_a["identity"], view_b["identity"]);
    trust_peer(a, &view_b, b);
    let probe = json!({"peerId": view_b["identity"]["peerId"]});
    assert_eq!(peer_admin(a, "POST", "/v1/projection-peers/probe", probe.clone()).0, 502);
    trust_peer(b, &view_a, a);
    let checked = peer_admin(a, "POST", "/v1/projection-peers/probe", probe.clone());
    assert_eq!(checked.0, 200);
    assert_eq!(checked.1["verified"], true);
    assert_eq!(checked.1["deliveryAvailable"], false);
    assert_eq!(peer_admin(b, "POST", "/v1/projection-peers/probe",
        json!({"peerId": view_a["identity"]["peerId"]})).0, 200);
    server_a.finish().unwrap();
    let (a, mut server_a) = start(&root_a.0);
    assert_eq!(peer_view(a)["identity"], view_a["identity"]);
    assert_eq!(peer_view(a)["peers"].as_array().unwrap().len(), 1);
    assert_eq!(peer_admin(a, "POST", "/v1/projection-peers/probe", probe.clone()).0, 200);
    let revoked = peer_admin(b, "DELETE", "/v1/projection-peers", json!({
        "expectedRevision": peer_view(b)["revision"], "peerId": view_a["identity"]["peerId"]
    }));
    assert_eq!(revoked.0, 200);
    assert_eq!(peer_admin(a, "POST", "/v1/projection-peers/probe", probe).0, 502);
    server_a.finish().unwrap(); server_b.finish().unwrap();
}

#[test]
fn offline_peers_http_rejects_device_admin_access_tampering_and_replay() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let root_a = ProjectionRoot::new(); let root_b = ProjectionRoot::new();
    let (a, mut server_a) = start(&root_a.0); let (b, mut server_b) = start(&root_b.0);
    let view_a = peer_view(a); let view_b = peer_view(b);
    let hook = Identity::pair(a, "Hook cannot configure Loom peers");
    let headers = format!("Authorization: Device {}\r\nX-Loom-Device-Nonce: {}\r\n", hook.token, Uuid::new_v4());
    assert_eq!(response(http_request_with_extra_headers(a, "GET", "/v1/projection-peers", None, &headers)).0, 403);
    assert_eq!(public(a, "/v1/projection-peers/probe", json!({})).0, 403);
    let trusted = trust_peer(b, &view_a, a);
    let stale = json!({"expectedRevision": 0, "peerId": view_a["identity"]["peerId"]});
    assert_eq!(peer_admin(b, "DELETE", "/v1/projection-peers", stale).0, 409);
    let stored: Value = serde_json::from_slice(&fs::read(root_a.0.join("settings/offline-projection-peers.json")).unwrap()).unwrap();
    let key: SigningKeyDocument = serde_json::from_value(stored["identity"].clone()).unwrap();
    let nonce = Uuid::new_v4().simple().to_string(); let now = unix_time_millis();
    let message = format!("loom.offline-peer.v1\nrequest\n{}\n{}\n{nonce}\n{now}",
        view_a["identity"]["peerId"].as_str().unwrap(), view_b["identity"]["peerId"].as_str().unwrap());
    let request = json!({"sourceId": view_a["identity"]["peerId"], "targetId": view_b["identity"]["peerId"],
        "nonce": nonce, "timestampMs": now, "signature": sign_message(&key, message.as_bytes()).unwrap()});
    let mut tampered = request.clone(); tampered["nonce"] = json!("0".repeat(32));
    assert_eq!(public(b, "/v1/projection-peer/handshake", tampered).0, 403);
    assert_eq!(public(b, "/v1/projection-peer/handshake", request.clone()).0, 200);
    assert_eq!(public(b, "/v1/projection-peer/handshake", request).0, 409);
    let mut disabled = trusted["peers"][0].clone(); disabled["enabled"] = json!(false);
    assert_eq!(peer_admin(b, "PUT", "/v1/projection-peers", json!({"expectedRevision": trusted["revision"], "peer": disabled})).0, 200);
    trust_peer(a, &view_b, b);
    assert_eq!(peer_admin(a, "POST", "/v1/projection-peers/probe", json!({"peerId": view_b["identity"]["peerId"]})).0, 502);
    assert_eq!(peer_view(b)["peers"].as_array().unwrap().len(), 1);
    server_a.finish().unwrap(); server_b.finish().unwrap();
}

#[test]
fn offline_peers_http_rejects_bad_pins_origins_and_corrupt_persistence() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let root = ProjectionRoot::new(); let other = ProjectionRoot::new();
    let (port, mut server) = start(&root.0); let (b, mut server_b) = start(&other.0);
    let own = peer_view(port); let remote = peer_view(b);
    let mut peer = json!({"peerId": remote["identity"]["peerId"], "publicKey": remote["identity"]["publicKey"],
        "name": "B", "origin": "http://192.168.1.2:8765", "enabled": true});
    let put = |value: Value| peer_admin(port, "PUT", "/v1/projection-peers", json!({"expectedRevision": 0, "peer": value}));
    assert_eq!(put(peer.clone()).0, 400);
    peer["origin"] = json!("https://peer.example.test"); peer["peerId"] = own["identity"]["peerId"].clone();
    assert_eq!(put(peer.clone()).0, 400);
    peer["publicKey"] = own["identity"]["publicKey"].clone();
    assert_eq!(put(peer).0, 400);
    assert_eq!(peer_view(port)["revision"], 0);
    server.finish().unwrap(); server_b.finish().unwrap();
    let path = root.0.join("settings/offline-projection-peers.json");
    fs::write(&path, b"{corrupt").unwrap();
    assert!(LoomDaemon::bind(DaemonConfig::localhost(0).with_control_plane_root(&root.0)).is_err());
    assert_eq!(fs::read(path).unwrap(), b"{corrupt");
}
