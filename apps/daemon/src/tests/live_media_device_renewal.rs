// Real signed reauthorization never resets live membership, sequence or input authority.
fn renewed_token(f: &Fixture, id: &str, key: &ed25519_dalek::SigningKey, old: &str) -> String {
    let issued_after = unix_time_millis();
    let (status, session) = wall_http::issue_session(f.port, id, key);
    assert_eq!(status, 201);
    assert_eq!(session["deviceId"], id);
    let expiry = session["expiresAtMs"].as_u64().unwrap();
    assert!(expiry >= issued_after.saturating_add(DEVICE_SESSION_TTL_MILLIS));
    assert!(expiry <= unix_time_millis().saturating_add(DEVICE_SESSION_TTL_MILLIS));
    let token = session["token"].as_str().unwrap().to_owned();
    assert!(token != old, "renewal must issue a distinct credential");
    token
}

fn denied_upgrade(f: &Fixture, token: &str, role: &str, status: u16) {
    match f.connect(token, role) {
        Err(error) => match *error {
            tungstenite::Error::Http(response) => assert_eq!(response.status().as_u16(), status),
            _ => panic!("expected HTTP denial, not a transport failure"),
        },
        Ok(_) => panic!("unauthorized media upgrade succeeded"),
    }
}

fn resume_body(id: &str, sequence: u64) -> Value {
    json!({"envelope": LiveControlEnvelope {
        protocol_version: loom_protocol::LIVE_PROTOCOL_VERSION.to_owned(),
        session_id: "live:device-auth".to_owned(),
        epoch: 1,
        sequence,
        message: LiveControlMessage::ResumeRequest(loom_protocol::LiveResumeRequest {
            last_control_sequence: sequence - 1,
            last_frame_id: 1,
            last_input_sequence: 0,
            requester_device_id: id.to_owned(),
        }),
    }})
}

fn renewal_after_expiry(role: &str) {
    let f = Fixture::new();
    let key = ed25519_dalek::SigningKey::generate(&mut OsRng);
    let (owner, source_token) = if role == "source" {
        wall_http::pair_with_key(f.port, "Renewing source", &key)
    } else {
        wall_http::pair(f.port, "Retained source")
    };
    let (viewer, viewer_token) = if role == "viewer" {
        wall_http::pair_with_key(f.port, "Renewing viewer", &key)
    } else {
        wall_http::pair(f.port, "Retained viewer")
    };
    f.create(&owner, &[&viewer]);
    let before = f.sessions.get("live:device-auth").unwrap();
    let mut source = f.connect(&source_token, "source").unwrap();
    let mut receiver = f.connect(&viewer_token, "viewer").unwrap();
    source
        .send(tungstenite::Message::Binary(encoded_live_frame(1)))
        .unwrap();
    assert_eq!(binary(&mut receiver), 1);
    // Seed an actual store lease; no OS input is injected by this software fixture.
    f.sessions
        .change_controller(
            &viewer,
            "live:device-auth",
            &phase_five_lease(LiveControlLeaseAction::Acquire, 2),
        )
        .unwrap();
    assert_eq!(
        f.sessions
            .get("live:device-auth")
            .unwrap()
            .session
            .controller_device
            .as_deref(),
        Some(viewer.as_str())
    );
    let (id, old, sequence) = if role == "source" {
        (&owner, &source_token, 2)
    } else {
        (&viewer, &viewer_token, 3)
    };
    expire_soon(&f, old);
    if role == "source" {
        assert_eq!(closed(&mut source), None);
        f.wait(|s| !s.source_connected);
    } else {
        assert_eq!(closed(&mut receiver), None);
        f.wait(|s| !s.viewer_connections.contains_key(&viewer));
    }
    assert!(f
        .sessions
        .get("live:device-auth")
        .unwrap()
        .session
        .controller_device
        .is_none());
    denied_upgrade(&f, old, role, 401);
    let next = renewed_token(&f, id, &key, old);
    let body = resume_body(id, sequence);
    let resume_path = "/v1/live/sessions/live:device-auth/resume";
    assert_eq!(
        wall_http::device(f.port, old, "POST", resume_path, Some(body.clone())).0,
        401
    );
    assert_eq!(
        wall_http::device(f.port, &next, "POST", resume_path, Some(body.clone())).0,
        200
    );
    // A new token is not a new control-sequence namespace.
    assert_eq!(
        wall_http::device(f.port, &next, "POST", resume_path, Some(body)).0,
        409
    );
    if role == "source" {
        source = f.connect(&next, role).unwrap();
    } else {
        receiver = f.connect(&next, role).unwrap();
        assert_eq!(binary(&mut receiver), 1);
    }
    source
        .send(tungstenite::Message::Binary(encoded_live_frame(2)))
        .unwrap();
    assert_eq!(binary(&mut receiver), 2);
    denied_upgrade(&f, old, role, 401);
    let after = f.sessions.get("live:device-auth").unwrap();
    assert_eq!(
        after.session.source_device_id,
        before.session.source_device_id
    );
    assert_eq!(after.session.source_hook_id, before.session.source_hook_id);
    assert_eq!(after.session.session_id, before.session.session_id);
    assert_eq!(after.epoch, before.epoch);
    assert_eq!(after.session.viewer_devices, before.session.viewer_devices);
    assert!(after.source_connected);
    assert_eq!(after.viewer_connections.get(&viewer), Some(&1));
    assert!(after.session.controller_device.is_none());
    let input = phase_five_input("live:device-auth", &viewer, 4, 1);
    assert_eq!(
        f.sessions
            .forward_input(&viewer, "live:device-auth", input)
            .unwrap_err()
            .code,
        "live_input_controller_required"
    );
}

#[test]
fn signed_source_renewal_after_expiry_preserves_identity_without_input_authority() {
    renewal_after_expiry("source");
}

#[test]
fn signed_viewer_renewal_after_expiry_preserves_membership_without_input_authority() {
    renewal_after_expiry("viewer");
}

#[test]
fn signed_renewal_cannot_reopen_closed_session_or_admit_unrelated_device() {
    let f = Fixture::new();
    let key = ed25519_dalek::SigningKey::generate(&mut OsRng);
    let (owner, old) = wall_http::pair_with_key(f.port, "Source to close", &key);
    let (outsider, outsider_token) = wall_http::pair(f.port, "Unrelated device");
    f.create(&owner, &[]);
    denied_upgrade(&f, &outsider_token, "source", 403);
    denied_upgrade(&f, &outsider_token, "viewer", 403);
    assert_eq!(
        wall_http::device(
            f.port,
            &outsider_token,
            "POST",
            "/v1/live/sessions/live:device-auth/resume",
            Some(resume_body(&outsider, 2))
        )
        .0,
        403
    );
    let close = LiveControlEnvelope {
        protocol_version: loom_protocol::LIVE_PROTOCOL_VERSION.to_owned(),
        session_id: "live:device-auth".to_owned(),
        epoch: 1,
        sequence: 2,
        message: LiveControlMessage::SessionEnd(loom_protocol::LiveSessionEnd {
            ended_by_device_id: owner.clone(),
            reason: loom_protocol::LiveSessionEndReason::Closed,
            detail: None,
        }),
    };
    assert_eq!(
        wall_http::device(
            f.port,
            &old,
            "POST",
            "/v1/live/sessions/live:device-auth/close",
            Some(json!({"envelope":close}))
        )
        .0,
        200
    );
    let next = renewed_token(&f, &owner, &key, &old);
    denied_upgrade(&f, &next, "source", 404);
    assert_eq!(
        wall_http::device(
            f.port,
            &next,
            "POST",
            "/v1/live/sessions/live:device-auth/resume",
            Some(resume_body(&owner, 3))
        )
        .0,
        404
    );
    assert!(f.sessions.get("live:device-auth").unwrap().closed);
}

#[test]
fn disabled_device_cannot_issue_a_new_signed_session() {
    let f = Fixture::new();
    let key = ed25519_dalek::SigningKey::generate(&mut OsRng);
    let (owner, token) = wall_http::pair_with_key(f.port, "Disabled source", &key);
    f.create(&owner, &[]);
    let mut source = f.connect(&token, "source").unwrap();
    let body = json!({"name":"Disabled source", "kind":"computer", "address":"127.0.0.1", "enabled":false}).to_string();
    let response = http_request(f.port, "PUT", &format!("/v1/devices/{owner}"), Some(&body));
    assert_eq!(response.split_whitespace().nth(1), Some("200"));
    assert_eq!(closed(&mut source).unwrap().1, "live_media_device_revoked");
    let (status, denial) = wall_http::issue_session(f.port, &owner, &key);
    assert_eq!(status, 403);
    assert_eq!(denial["error"]["code"], "device_not_authorized");
    assert!(denial.get("token").is_none());
    denied_upgrade(&f, &token, "source", 401);
}
