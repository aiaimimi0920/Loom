// Real device HTTP + immutable approval + signed host proof, using the original Live store.
const VIEWER_ROUTE: &str = "/v1/art/local/live-viewer/attach";
fn viewer_request(f: &Fixture, grant: Value, session: &str) -> Value {
    json!({"protocolVersion":loom_protocol::local_art::live_viewer::PROTOCOL,
        "binding":f.request["binding"],"grant":grant,"requireExistingMembership":false,
        "envelope":live_viewer_envelope(session,&f.device)})
}
fn viewer_session(f: &Fixture, session: &str) {
    let (source, token) = pair(f.port, "Native viewer source");
    let mut envelope = live_start_envelope(session);
    let LiveControlMessage::SessionStart(start) = &mut envelope.message else {
        panic!("expected source start");
    };
    start.session.source_device_id = source.clone();
    start.requested_by_device_id = source;
    let response = device(
        f.port,
        &token,
        "POST",
        "/v1/live/sessions",
        Some(json!({"envelope":envelope})),
    );
    assert_eq!(response.0, 201, "{:?}", response);
}
fn attach_viewer(f: &Fixture, input: Value) -> (u16, Value) {
    device(f.port, &f.token, "POST", VIEWER_ROUTE, Some(input))
}
impl Fixture {
    fn server_grant(&self, prepared: &Value) -> loom_protocol::local_art::Grant {
        serde_json::from_slice(
            &BASE64
                .decode(prepared["grant"]["payload"].as_str().unwrap())
                .unwrap(),
        )
        .unwrap()
    }
    fn sign_viewer_grant(&self, grant: &loom_protocol::local_art::Grant) -> Value {
        // Only this fixture's newly generated issuer; never a real profile or caller-selected path.
        let state: Value = serde_json::from_slice(
            &fs::read(self.root.join("settings/offline-projection-peers.json")).unwrap(),
        )
        .unwrap();
        let key: SigningKeyDocument = serde_json::from_value(state["identity"].clone()).unwrap();
        serde_json::from_slice(&loom_plugin_security::local_art_grant::issue(&key, grant).unwrap())
            .unwrap()
    }
}
#[test]
fn local_art_viewer_http_requires_device_exact_grant_and_keeps_surface_contract() {
    let f = Fixture::new();
    assert_eq!(f.approve(0, true).0, 200);
    let (status, prepared) = f.prepare(f.request.clone());
    assert_eq!(status, 200);
    viewer_session(&f, "live:local-art");
    let input = viewer_request(&f, prepared["grant"].clone(), "live:local-art");
    assert_eq!(
        public(f.port, "POST", VIEWER_ROUTE, Some(input.clone())).0,
        401
    );
    assert_eq!(
        admin(f.port, "POST", VIEWER_ROUTE, Some(input.clone())).0,
        403
    );
    for (pointer, bad) in [
        ("/binding/deviceId", json!("other")),
        ("/binding/generation", json!(2)),
        ("/grant/signature", json!("AAAA")),
        ("/envelope/payload/responderDeviceId", json!("other")),
        ("/envelope/payload/accepted", json!(false)),
    ] {
        let mut changed = input.clone();
        *changed.pointer_mut(pointer).unwrap() = bad;
        assert_eq!(attach_viewer(&f, changed).0, 403, "{pointer}");
    }
    let legacy = device(
        f.port,
        &f.token,
        "POST",
        "/v1/live/sessions/live:local-art/viewers",
        Some(input.clone()),
    );
    assert_eq!(legacy.0, 400, "{legacy:?}");
    let mut require = input.clone();
    require["requireExistingMembership"] = json!(true);
    assert_eq!(attach_viewer(&f, require).0, 409);
    let (status, joined) = attach_viewer(&f, input.clone());
    assert_eq!(status, 200, "{joined}");
    assert_eq!(joined["session"]["viewerDevices"], json!([f.device]));
    assert_eq!(joined["session"]["controllerDevice"], Value::Null);
    assert_eq!(f.approve(1, false).0, 200);
    assert_eq!(attach_viewer(&f, input).0, 403);
}
#[test]
fn local_art_viewer_http_rechecks_source_and_denies_disabled_or_unapproved_host() {
    let f = Fixture::new();
    assert_eq!(f.approve(0, true).0, 200);
    let (status, prepared) = f.prepare(f.request.clone());
    assert_eq!(status, 200);
    viewer_session(&f, "live:source-check");
    let mut grant: loom_protocol::local_art::Grant = f.server_grant(&prepared);
    grant.issued_at_ms = unix_time_millis() - 10000;
    grant.expires_at_ms = grant.issued_at_ms + 1;
    grant.offline_until_ms = 0;
    let expired = f.sign_viewer_grant(&grant);
    assert_eq!(
        attach_viewer(&f, viewer_request(&f, expired, "live:source-check")).0,
        403
    );
    grant = f.server_grant(&prepared);
    grant.source.digest = "c".repeat(64);
    grant.source.resource_id = format!("sha256:{}", grant.source.digest);
    let wrong = f.sign_viewer_grant(&grant);
    assert_eq!(
        attach_viewer(&f, viewer_request(&f, wrong, "live:source-check")).0,
        403
    );
    let disabled = admin(
        f.port,
        "PUT",
        &format!("/v1/devices/{}", f.device),
        Some(
            json!({"name":"Local Art client","kind":"computer","address":"127.0.0.1","enabled":false}),
        ),
    );
    assert_eq!(disabled.0, 200, "{disabled:?}");
    assert_ne!(
        attach_viewer(
            &f,
            viewer_request(&f, prepared["grant"].clone(), "live:source-check")
        )
        .0,
        200
    );
}
