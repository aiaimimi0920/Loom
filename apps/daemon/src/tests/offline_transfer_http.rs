impl Identity {
    fn offline(&self, port: u16, operation: &str, body: Value) -> (u16, Value) {
        let headers = format!("Authorization: Device {}\r\nX-Loom-Device-Nonce: {}\r\n", self.token, Uuid::new_v4());
        response(http_request_with_extra_headers(port, "POST", &format!("/v1/offline-projections/{operation}"), Some(&body.to_string()), &headers))
    }
}

#[test]
fn offline_transfer_device_epoch_cannot_revive_after_reenable() {
    let _guard=TEST_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for revoke_source in [false, true] {
        let ra=ProjectionRoot::new();let rb=ProjectionRoot::new();
        let(a,mut sa)=start(&ra.0);let(b,mut sb)=start(&rb.0);
        let va=peer_view(a);let vb=peer_view(b);trust_peer(a,&vb,b);trust_peer(b,&va,a);
        let mut source=Identity::pair(a,"A");let mut target=Identity::pair(b,"B");
        target.offline(b,"inbox",json!({"policy":"auto"}));
        let snapshot=png(18);let envelope=source.invitation(&snapshot,unix_time_millis()+270_000);
        assert_eq!(source.offline(a,"create",offline_create(&source,&target,&vb,&envelope,&snapshot)).0,200);
        assert_eq!(target.offline(b,"accept",acceptance(&envelope,"b")).0,200);
        let (port, identity)=if revoke_source {(a,&mut source)} else {(b,&mut target)};
        for enabled in [false,true] {
            let raw=http_request(port,"PUT",&format!("/v1/devices/{}",identity.id),Some(&json!({"name":"Epoch device","kind":"computer","address":"127.0.0.1","enabled":enabled}).to_string()));
            assert_eq!(response(raw).0,200);
        }
        identity.session(port);
        assert_eq!(target.offline(b,"read",json!({"projectionId":envelope.projection_id,"knownRevision":1})).0,403);
        sa.finish().unwrap();sb.finish().unwrap();
    }
}
fn offline_create(source: &Identity, target: &Identity, peer: &Value, envelope: &ProjectionEnvelope, snapshot: &ProjectionSnapshot) -> Value {
    let peer_id = peer["identity"]["peerId"].as_str().unwrap();
    assert_eq!(envelope.source.device_id, source.id);
    json!({"envelope": envelope, "snapshot": snapshot, "peerId": peer_id, "remoteDeviceId": target.id,
        "targetDeviceId": format!("peer-target:{}", sha256_bytes(format!("{peer_id}\n{}", target.id).as_bytes()))})
}
fn restart_offline(root: &Path, port: u16) -> ConcurrencyTestFixture {
    let daemon = LoomDaemon::bind(DaemonConfig::localhost(port).with_control_plane_root(root).with_bounded_request_executor(4,16)).unwrap();
    let (tx, rx) = mpsc::channel(); let server = thread::spawn(move || daemon.serve_until(rx));
    ConcurrencyTestFixture::new(tx, server)
}

#[test]
fn offline_transfer_http_confirm_update_restart_receipt_and_stop() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ra = ProjectionRoot::new(); let rb = ProjectionRoot::new();
    let (a, mut sa) = start(&ra.0); let (b, mut sb) = start(&rb.0);
    let va = peer_view(a); let vb = peer_view(b); trust_peer(a, &vb, b); trust_peer(b, &va, a);
    let mut source = Identity::pair(a, "Source A"); let mut target = Identity::pair(b, "Target B");
    let outsider = Identity::pair(b, "Unrelated B");
    assert_eq!(target.offline(b, "inbox", json!({"policy":"confirm"})).0, 200);
    let snapshot = png(50); let envelope = source.invitation(&snapshot, unix_time_millis()+270_000);
    let create = offline_create(&source, &target, &vb, &envelope, &snapshot);
    let created = source.offline(a, "create", create.clone()); assert_eq!(created.0,200,"{}",created.1);
    assert_eq!(created.1["envelope"], json!(envelope));
    assert_eq!(source.offline(a, "create", create).0,200);
    let inbox = target.offline(b,"inbox",json!({"policy":"confirm"}));
    assert_eq!(inbox.1["invitations"][0]["envelope"],json!(envelope));
    assert_eq!(outsider.offline(b,"accept",acceptance(&envelope,"unit-b")).0,403);
    let received = target.offline(b,"accept",acceptance(&envelope,"unit-b")); assert_eq!(received.0,200,"{}",received.1);
    assert_eq!(received.1["snapshot"],json!(snapshot));
    assert_eq!(target.offline(b,"accept",acceptance(&envelope,"unit-b")).0,200);
    let receipt=json!({"projectionId":envelope.projection_id,"status":"displayed"});
    assert_eq!(target.offline(b,"receipt",receipt.clone()).0,200);
    assert_eq!(target.offline(b,"receipt",receipt).0,200);
    assert_eq!(source.offline(a,"read",json!({"projectionId":envelope.projection_id,"knownRevision":1})).1["delivery"]["status"],"displayed");
    let record_path = |root: &Path| root.join("offline-projections").join(format!("{}.json", sha256_bytes(envelope.projection_id.as_bytes())));
    let before = [fs::metadata(record_path(&ra.0)).unwrap().modified().unwrap(), fs::metadata(record_path(&rb.0)).unwrap().modified().unwrap()];
    thread::sleep(Duration::from_millis(30));
    for _ in 0..3 {
        assert_eq!(target.offline(b,"read",json!({"projectionId":envelope.projection_id,"knownRevision":1})).0,200);
    }
    assert_eq!(before, [fs::metadata(record_path(&ra.0)).unwrap().modified().unwrap(), fs::metadata(record_path(&rb.0)).unwrap().modified().unwrap()]);
    thread::sleep(Duration::from_millis(550));
    assert_eq!(source.offline(a,"update",update(&envelope,1,&png(90))).0,200);
    assert_eq!(target.offline(b,"read",json!({"projectionId":envelope.projection_id,"knownRevision":1})).1["snapshot"],json!(png(90)));
    sa.finish().unwrap();sb.finish().unwrap();
    let mut sa=restart_offline(&ra.0,a);let mut sb=restart_offline(&rb.0,b);
    source.session(a);target.session(b);
    let recovered=target.offline(b,"read",json!({"projectionId":envelope.projection_id,"knownRevision":1}));
    assert_eq!(recovered.0,200,"{}",recovered.1);assert_eq!(recovered.1["snapshot"],json!(png(90)));
    assert_eq!(source.offline(a,"unlink",json!({"projectionId":envelope.projection_id})).0,200);
    assert_eq!(target.offline(b,"read",json!({"projectionId":envelope.projection_id,"knownRevision":2})).0,410);
    sa.finish().unwrap();sb.finish().unwrap();
}

#[test]
fn offline_transfer_http_rejects_forged_source_disabled_target_and_peer_revocation() {
    let _guard=TEST_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let ra=ProjectionRoot::new();let rb=ProjectionRoot::new();let(a,mut sa)=start(&ra.0);let(b,mut sb)=start(&rb.0);
    let va=peer_view(a);let vb=peer_view(b);trust_peer(a,&vb,b);trust_peer(b,&va,a);
    let source=Identity::pair(a,"A");let target=Identity::pair(b,"B");let other=Identity::pair(a,"Other");
    let snapshot=png(12);let envelope=source.invitation(&snapshot,unix_time_millis()+270_000);
    let create=offline_create(&source,&target,&vb,&envelope,&snapshot);
    assert_eq!(source.offline(a,"create",create.clone()).0,409);
    target.offline(b,"inbox",json!({"policy":"auto"}));
    assert_eq!(other.offline(a,"create",create.clone()).0,403);
    assert_eq!(source.offline(a,"create",create).0,200);
    let rejection=json!({"projectionId":envelope.projection_id,"status":"rejected"});
    assert_eq!(target.offline(b,"receipt",rejection.clone()).0,200);
    assert_eq!(target.offline(b,"receipt",rejection).0,200);
    assert_eq!(source.offline(a,"read",json!({"projectionId":envelope.projection_id,"knownRevision":1})).0,410);
    let envelope=source.invitation(&snapshot,unix_time_millis()+270_000);
    assert_eq!(source.offline(a,"create",offline_create(&source,&target,&vb,&envelope,&snapshot)).0,200);
    assert_eq!(target.offline(b,"accept",acceptance(&envelope,"b")).0,200);
    let peer_id=va["identity"]["peerId"].clone();
    assert_eq!(peer_admin(b,"DELETE","/v1/projection-peers",json!({"expectedRevision":peer_view(b)["revision"],"peerId":peer_id})).0,200);
    trust_peer(b,&va,a);
    assert_eq!(target.offline(b,"read",json!({"projectionId":envelope.projection_id,"knownRevision":1})).0,403);
    assert_eq!(public(a,"/v1/offline-projections/create",json!({})).0,403);
    sa.finish().unwrap();sb.finish().unwrap();
}
