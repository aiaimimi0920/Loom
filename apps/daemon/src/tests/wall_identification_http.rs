#[test]
fn wall_identification_requires_admin_releases_input_and_scopes_actual_reports() {
    let mut f = WallInputFixture::with_identification(true);
    let command = json!({"endpointId":"endpoint-left"});
    let control = f.acquire(0);
    assert_eq!(
        f.input(&control, 1, WallInputFixture::button("pressed")).0,
        200
    );
    assert_eq!(
        f.post("/v1/walls/endpoints/identify", command.clone()).0,
        403
    );
    assert_eq!(
        public(
            f.port,
            "POST",
            "/v1/walls/endpoints/identify",
            Some(command.clone())
        )
        .0,
        401
    );
    assert_eq!(
        admin(
            f.port,
            "POST",
            "/v1/walls/endpoints/identify",
            Some(json!({"endpointId":"endpoint-left","ttlMs":999999}))
        )
        .0,
        400
    );
    let (status, state) = admin(
        f.port,
        "POST",
        "/v1/walls/endpoints/identify",
        Some(command.clone()),
    );
    assert_eq!(status, 200);
    assert_eq!(state["revision"], 3);
    assert_eq!(state["layouts"][0]["revision"], 3);
    let id = state["endpoints"][0]["identification"]["requestId"].clone();
    assert_eq!(state["endpoints"][0]["identification"]["applied"], false);
    assert!(state["endpoints"][1].get("identification").is_none());
    assert!(f
        .sessions
        .get("live-1")
        .unwrap()
        .session
        .controller_device
        .is_none());
    assert_eq!(
        f.post("/v1/walls/control/acquire", f.acquire_body(0)).1["error"]["code"],
        "wall_endpoint_identifying"
    );
    let peer = f.acquire(1);
    assert_eq!(f.post("/v1/walls/control/release", peer).0, 200);
    let report = json!({"endpointId":"endpoint-left","leaseId":f.bindings[0]["leaseId"],"requestId":id,"outcome":"applied"});
    let (_, outsider) = pair(f.port, "Other identification device");
    assert_eq!(
        device(
            f.port,
            &outsider,
            "POST",
            "/v1/walls/endpoints/identify/report",
            Some(report.clone())
        )
        .0,
        403
    );
    assert_eq!(
        admin(
            f.port,
            "POST",
            "/v1/walls/endpoints/identify/report",
            Some(report.clone())
        )
        .0,
        401
    );
    assert_eq!(
        f.post("/v1/walls/endpoints/identify/report", report.clone())
            .0,
        200
    );
    let (_, reported) = admin(f.port, "GET", "/v1/walls/state", None);
    assert_eq!(reported["endpoints"][0]["identification"]["applied"], true);
    let mut dismissed = report.clone();
    dismissed["outcome"] = json!("dismissed");
    assert_eq!(
        f.post("/v1/walls/endpoints/identify/report", dismissed).0,
        200
    );
    let resumed = f.acquire(0);
    assert_ne!(resumed["controlId"], control["controlId"]);
    assert_eq!(f.post("/v1/walls/control/release", resumed).0, 200);
    assert_eq!(
        admin(
            f.port,
            "POST",
            "/v1/walls/endpoints/identify",
            Some(command.clone())
        )
        .0,
        200
    );
    assert_eq!(f.post("/v1/walls/endpoints/identify/report", report).0, 200);
    let (_, successor) = admin(f.port, "GET", "/v1/walls/state", None);
    assert_ne!(successor["endpoints"][0]["identification"]["requestId"], id);
    assert_eq!(
        successor["endpoints"][0]["identification"]["applied"],
        false
    );
    f.devices
        .lock()
        .unwrap()
        .devices
        .get_mut(&f.device_id)
        .unwrap()
        .enabled = false;
    let (_, revoked) = admin(f.port, "GET", "/v1/walls/state", None);
    assert_eq!(revoked["endpoints"][0]["online"], false);
    assert!(revoked["endpoints"][0].get("identification").is_none());
    assert_eq!(
        admin(
            f.port,
            "POST",
            "/v1/walls/endpoints/identify",
            Some(command)
        )
        .0,
        403
    );
    f.server.finish().unwrap();
}
