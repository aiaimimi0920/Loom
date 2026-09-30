fn projection_settings_put(port: u16, revision: u64, target: &str, policy: &str, allowed: Value, denied: Value) -> (u16, Value) {
    response(http_request(port, "PUT", "/v1/projection-settings", Some(&json!({
        "expectedRevision": revision,
        "groups": [{"groupId":"team","name":"Team","members":allowed}],
        "rules": [{"deviceId":target,"policy":policy,"whitelist":{"devices":[],"groups":["team"],"users":[]},"blacklist":denied}]
    }).to_string())))
}

#[test]
fn projection_settings_http_shared_rules_are_authoritative_and_rechecked() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let root = ProjectionRoot::new(); let (port, mut server) = start(&root.0);
    let source = Identity::pair(port, "Source"); let target = Identity::pair(port, "Target");
    target.post(port, "inbox", json!({"policy":"auto"}));
    let snapshot = png(48); let envelope = source.invitation(&snapshot, unix_time_millis()+270_000);
    let create = json!({"envelope":envelope,"snapshot":snapshot,"targetDeviceId":target.id});
    let members = json!([{"deviceId":source.id}]);
    assert_eq!(projection_settings_put(port, 0, &target.id, "reject", json!([]), json!([])).0, 200);
    assert_eq!(source.post(port,"create",create.clone()).0,403);
    assert_eq!(projection_settings_put(port, 1, &target.id, "reject", members.clone(), members).0,200);
    assert_eq!(source.post(port,"create",create).0,200);
    assert_eq!(target.post(port,"inbox",json!({"policy":"confirm"})).1["invitations"][0]["receivePolicy"],"auto");
    assert_eq!(projection_settings_put(port, 2, &target.id, "confirm", json!([]), json!([])).0,200);
    assert_eq!(target.post(port,"inbox",json!({"policy":"auto"})).1["invitations"][0]["receivePolicy"],"confirm");
    assert_eq!(projection_settings_put(port, 3, &target.id, "reject", json!([]), json!([])).0,200);
    assert_eq!(target.post(port,"accept",acceptance(&envelope,"unit")).0,403);
    assert_eq!(target.post(port,"inspect",json!({"envelope":envelope})).0,403);
    assert!(target.post(port,"inbox",json!({"policy":"auto"})).1["invitations"].as_array().unwrap().is_empty());
    assert_eq!(projection_settings_put(port, 0, &target.id, "auto", json!([]), json!([])).0,409);
    let headers = format!("Authorization: Device {}\r\nX-Loom-Device-Nonce: {}\r\n",target.token,Uuid::new_v4());
    assert_eq!(response(http_request_with_extra_headers(port,"GET","/v1/projection-settings",None,&headers)).0,403);
    server.finish().unwrap();
}

#[test]
fn projection_settings_http_offline_namespace_and_acceptance_are_enforced() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let ra=ProjectionRoot::new(); let rb=ProjectionRoot::new();
    let (a,mut sa)=start(&ra.0); let (b,mut sb)=start(&rb.0);
    let va=peer_view(a); let vb=peer_view(b); trust_peer(a,&vb,b); trust_peer(b,&va,a);
    let source=Identity::pair(a,"A"); let target=Identity::pair(b,"B");
    source.post(a,"inbox",json!({"policy":"confirm"}));
    target.offline(b,"inbox",json!({"policy":"auto"}));
    let directory=response(http_request(b,"GET","/v1/projection-settings",None));
    assert_eq!(directory.0,200);
    assert!(directory.1["targets"].as_array().unwrap().iter().any(|entry| entry["route"] == "offline_peer"
        && entry["remoteDeviceId"] == source.id && entry["peerId"] == va["identity"]["peerId"]));
    assert_eq!(projection_settings_put(b,0,&target.id,"reject",json!([{"deviceId":source.id}]),json!([])).0,200);
    let snapshot=png(49); let denied=source.invitation(&snapshot,unix_time_millis()+270_000);
    assert_eq!(source.offline(a,"create",offline_create(&source,&target,&vb,&denied,&snapshot)).0,403);
    let scoped=json!([{"deviceId":source.id,"peerId":va["identity"]["peerId"]}]);
    assert_eq!(projection_settings_put(b,1,&target.id,"reject",scoped.clone(),scoped).0,200);
    let envelope=source.invitation(&snapshot,unix_time_millis()+270_000);
    let created=source.offline(a,"create",offline_create(&source,&target,&vb,&envelope,&snapshot));
    assert_eq!(created.0,200,"{}",created.1);
    assert_eq!(target.offline(b,"inbox",json!({"policy":"confirm"})).1["invitations"][0]["receivePolicy"],"auto");
    assert_eq!(projection_settings_put(b,2,&target.id,"reject",json!([]),json!([])).0,200);
    assert_eq!(target.offline(b,"accept",acceptance(&envelope,"remote")).0,403);
    assert_eq!(target.offline(b,"inspect",json!({"envelope":envelope})).0,403);
    assert!(target.offline(b,"inbox",json!({"policy":"auto"})).1["invitations"].as_array().unwrap().is_empty());
    sa.finish().unwrap(); sb.finish().unwrap();
}
