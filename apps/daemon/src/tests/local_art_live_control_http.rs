// Independent HTTP/control-store oracle; no OS input, new credential or caller-selected data path.
const ART_CONTROL: &str = "/v1/art/local/live-viewer/control";
fn art_control_request(f: &Fixture, grant: &Value, operation: Value) -> Value {
    json!({"protocolVersion":"loom.art.local.live-control.v1","binding":f.request["binding"],"grant":grant,"operation":operation})
}
fn art_control(f: &Fixture, grant: &Value, operation: Value) -> (u16, Value) {
    device(
        f.port,
        &f.token,
        "POST",
        ART_CONTROL,
        Some(art_control_request(f, grant, operation)),
    )
}
fn art_op(kind: &str, sequence: u64) -> Value {
    json!({"kind":kind,"sessionId":"live:art-control","epoch":1,"sequence":sequence})
}
fn art_input(sequence: u64, input: u64) -> Value {
    json!({"kind":"input","envelope":{"protocolVersion":"loom.live.v1","sessionId":"live:art-control","epoch":1,"sequence":sequence,"messageType":"input_event",
        "payload":{"inputSequence":input,"issuedAtMs":unix_time_millis(),"sourceDeviceId":"replaced-by-fixture","kind":{"kind":"key","data":{"code":"vk:65","state":"pressed","modifiers":0}}}}})
}
fn art_control_fixture(interactive: bool) -> (Fixture, Value) {
    let f = Fixture::new();
    assert_eq!(f.approve(0, true).0, 200);
    let (status, prepared) = f.prepare(f.request.clone());
    assert_eq!(status, 200);
    art_control_session(&f, "live:art-control", interactive);
    (f, prepared["grant"].clone())
}
fn art_control_session(f: &Fixture, session: &str, interactive: bool) {
    let (source, token) = pair(f.port, "Art controller source");
    let mut start = live_start_envelope(session);
    if let LiveControlMessage::SessionStart(value) = &mut start.message {
        value.session.source_device_id = source.clone();
        value.requested_by_device_id = source;
        value.session.interaction_capabilities = if interactive {
            vec![
                loom_protocol::LiveInteractionCapability::Keyboard,
                loom_protocol::LiveInteractionCapability::Cancel,
            ]
        } else {
            vec![]
        };
    }
    assert_eq!(
        device(
            f.port,
            &token,
            "POST",
            "/v1/live/sessions",
            Some(json!({"envelope":start}))
        )
        .0,
        201
    );
}
#[test]
fn local_art_control_requires_explicit_member_source_consent_and_original_ordering() {
    let (f, grant) = art_control_fixture(true);
    // Proof/Preview alone never joins or acquires control.
    assert_eq!(art_control(&f, &grant, art_op("acquire", 1)).0, 403);
    assert_eq!(
        attach_viewer(&f, viewer_request(&f, grant.clone(), "live:art-control")).0,
        200
    );
    assert!(f
        .live_sessions
        .get("live:art-control")
        .unwrap()
        .session
        .controller_device
        .is_none());
    assert_eq!(art_control(&f, &grant, art_op("acquire", 2)).0, 200);
    let mut input = art_input(3, 1);
    input["envelope"]["payload"]["sourceDeviceId"] = json!(f.device);
    let (status, accepted) = art_control(&f, &grant, input.clone());
    assert_eq!(status, 200, "{accepted}");
    assert!(accepted["acceptedSequence"].as_u64().unwrap() > 0);
    assert_eq!(art_control(&f, &grant, input).0, 409);
    assert_eq!(art_control(&f, &grant, art_op("release", 4)).0, 200);
    let state = f.live_sessions.get("live:art-control").unwrap();
    assert!(state.session.controller_device.is_none());
    assert_eq!(state.session.viewer_devices, vec![f.device.clone()]);
    let (readonly, grant) = art_control_fixture(false);
    assert_eq!(
        attach_viewer(
            &readonly,
            viewer_request(&readonly, grant.clone(), "live:art-control")
        )
        .0,
        200
    );
    assert_eq!(art_control(&readonly, &grant, art_op("acquire", 2)).0, 409);
}
#[test]
fn local_art_control_release_survives_art_revoke_but_never_reacquires_or_forwards() {
    let (f, grant) = art_control_fixture(true);
    assert_eq!(
        attach_viewer(&f, viewer_request(&f, grant.clone(), "live:art-control")).0,
        200
    );
    assert_eq!(art_control(&f, &grant, art_op("acquire", 2)).0, 200);
    assert_eq!(f.approve(1, false).0, 200);
    assert_eq!(art_control(&f, &grant, art_op("acquire", 3)).0, 403);
    let mut input = art_input(3, 1);
    input["envelope"]["payload"]["sourceDeviceId"] = json!(f.device);
    assert_eq!(art_control(&f, &grant, input).0, 403);
    assert_eq!(art_control(&f, &grant, art_op("release", 3)).0, 200);
    assert_eq!(art_control(&f, &grant, art_op("release", 4)).0, 403);
    assert!(f
        .live_sessions
        .get("live:art-control")
        .unwrap()
        .session
        .controller_device
        .is_none());
}
#[test]
fn local_art_control_rejects_admin_substitution_signature_and_expired_acquire() {
    let (f, grant) = art_control_fixture(true);
    let body = art_control_request(&f, &grant, art_op("acquire", 2));
    assert_eq!(
        public(f.port, "POST", ART_CONTROL, Some(body.clone())).0,
        401
    );
    assert_eq!(
        admin(f.port, "POST", ART_CONTROL, Some(body.clone())).0,
        403
    );
    for (pointer, bad) in [
        ("/binding/generation", json!(2)),
        ("/binding/deviceId", json!("other")),
        ("/grant/signature", json!("AAAA")),
    ] {
        let mut changed = body.clone();
        *changed.pointer_mut(pointer).unwrap() = bad;
        assert_eq!(
            device(f.port, &f.token, "POST", ART_CONTROL, Some(changed)).0,
            403
        );
    }
    assert_eq!(
        attach_viewer(&f, viewer_request(&f, grant.clone(), "live:art-control")).0,
        200
    );
    assert_eq!(art_control(&f, &grant, art_op("acquire", 2)).0, 200);
    let mut expired = f.server_grant(&json!({"grant":grant}));
    expired.issued_at_ms = unix_time_millis() - 10000;
    expired.expires_at_ms = expired.issued_at_ms + 1;
    expired.offline_until_ms = 0;
    let expired = f.sign_viewer_grant(&expired);
    assert_eq!(art_control(&f, &expired, art_op("acquire", 3)).0, 403);
    assert_eq!(art_control(&f, &expired, art_op("release", 3)).0, 200);
}
