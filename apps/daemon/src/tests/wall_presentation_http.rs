#[test]
fn wall_presentation_revokes_control_without_waiting_for_terminal_acknowledgement() {
    let mut f = WallInputFixture::new();
    let control = f.acquire(0);
    assert_eq!(
        f.input(&control, 1, WallInputFixture::button("pressed")).0,
        200
    );
    let body = json!({"baseRevision": 3, "wallId": "living-room", "mode": "frozen"});
    assert_eq!(
        device(
            f.port,
            &f.token,
            "PUT",
            "/v1/walls/presentation",
            Some(body.clone())
        )
        .0,
        403
    );
    let mut invalid = body.clone();
    invalid["mode"] = json!("hidden");
    assert_eq!(
        admin(f.port, "PUT", "/v1/walls/presentation", Some(invalid)).0,
        400
    );
    let (status, paused) = admin(f.port, "PUT", "/v1/walls/presentation", Some(body));
    assert_eq!(status, 200);
    assert_eq!(paused["layouts"][0]["revision"], 3);
    assert!(paused["endpoints"][0].get("presentation").is_none());
    assert!(f
        .sessions
        .get("live-1")
        .unwrap()
        .session
        .controller_device
        .is_none());
    assert_eq!(
        f.input(&control, 2, WallInputFixture::button("released")).0,
        409
    );
    let (status, rejected) = f.post("/v1/walls/control/acquire", f.acquire_body(1));
    assert_eq!(status, 409);
    assert_eq!(rejected["error"]["code"], "wall_presentation_paused");
    let (status, resumed) = admin(
        f.port,
        "PUT",
        "/v1/walls/presentation",
        Some(json!({"baseRevision": 4, "wallId": "living-room", "mode": "running"})),
    );
    assert_eq!(status, 200);
    assert_eq!(resumed["layouts"][0]["revision"], 5);
    assert_eq!(
        f.post("/v1/walls/control/acquire", f.acquire_body(0)).0,
        409
    );
    f.bindings[0]["revision"] = json!(5);
    assert_eq!(
        f.post(
            "/v1/walls/heartbeat",
            json!({"endpointId": "endpoint-left", "leaseId": f.bindings[0]["leaseId"],
        "sequence": 2, "appliedRevision": 5})
        )
        .0,
        200
    );
    let new_control = f.acquire(0);
    assert_ne!(new_control["controlId"], control["controlId"]);
    assert_eq!(f.post("/v1/walls/control/release", new_control).0, 200);
    f.server.finish().unwrap();
}
