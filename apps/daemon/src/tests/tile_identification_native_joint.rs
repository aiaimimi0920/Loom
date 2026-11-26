// Real LAN endpoint reports only; admin independently initiates identification and verifies volatile state, expiry, and recovery without spoofing a presenter.
#[test]
#[ignore = "explicit real Loom LAN native Tile identification lifecycle gate"]
fn real_lan_native_tile_identification_dismiss_expire_and_resume() {
    identify(false);
}
#[test]
#[ignore = "explicit real Loom LAN native Tile identification revocation gate"]
fn real_lan_native_tile_identification_revocation_clears_marker() {
    identify(true);
}
fn identify(revoked: bool) {
    let (f, device, mut server, _sessions) = setup();
    let mut output = f.child("identification_and_cleanup", revoked);
    let registered = f.until(&mut output, |s| {
        s["endpoints"]
            .as_array()
            .is_some_and(|e| e.len() == 1 && e[0]["online"] == true)
    });
    let endpoint = registered["endpoints"][0]["endpoint"].clone();
    assert_eq!(
        endpoint["display"],
        json!({"name":"fixture","canIdentify":true})
    );
    assert_eq!(endpoint["pixelSize"], json!({"width":640,"height":480}));
    assert!(registered["layouts"].as_array().unwrap().is_empty());
    let command = |f: &Joint| {
        let (status, state) = admin(
            f.port,
            "POST",
            "/v1/walls/endpoints/identify",
            Some(json!({"endpointId":endpoint["endpointId"]})),
        );
        assert_eq!(status, 200, "{state}");
        assert_eq!(state["endpoints"][0]["identification"]["applied"], false);
        state
    };
    let first = command(&f);
    assert_eq!(first["revision"], registered["revision"]);
    let confirmed = f.until(&mut output, |s| {
        s["endpoints"][0]["identification"]["applied"] == true
    });
    assert!(confirmed["endpoints"][0]["appliedRevision"].is_null());
    let marker = f.phase(&mut output, "tile-identify-marker-1.json");
    assert_eq!(
        marker["requestId"],
        first["endpoints"][0]["identification"]["requestId"]
    );
    assert_eq!(marker["nativeVisiblePaint"], true);
    let mut proof = json!({"registered":registered,"unassignedCommand":first,"applied":confirmed,"nativeMarker":marker});
    if revoked {
        let (status, value) = admin(
            f.port,
            "PUT",
            &format!("/v1/devices/{device}"),
            Some(
                json!({"name":"Tile isolated","kind":"computer","address":"127.0.0.1","enabled":false}),
            ),
        );
        assert_eq!(status, 200, "{value}");
    } else {
        fs::write(f.root.join("tile-identify-dismiss"), b"dismiss").unwrap();
        proof["dismissed"] = f.until(&mut output, |s| {
            s["endpoints"][0].get("identification").is_none()
        });
        let (status, image) = admin(
            f.port,
            "POST",
            "/v1/surfaces/resources",
            Some(
                json!({"kind":"image","mime":"application/x-neuro-rgba8","width":2,"height":2,"dataBase64":BASE64.encode([255,0,0,255,0,255,0,255,0,0,255,255,255,255,255,255])}),
            ),
        );
        assert_eq!(status, 201, "{image}");
        let layout = json!({"protocolVersion":"loom.wall.v1","wallId":"identify-wall","revision":2,"bounds":{"x":0,"y":0,"width":2,"height":2},"tiles":[{"tileId":"tile","endpointId":endpoint["endpointId"],"rect":{"x":0,"y":0,"width":2,"height":2},"rotation":"deg0"}],"placements":[{"placementId":"picture","source":{"kind":"image","id":image["resource"]["resourceId"]},"rect":{"x":0,"y":0,"width":2,"height":2},"sourceCrop":{"x":0,"y":0,"width":1,"height":1},"zIndex":0,"interactive":false}]});
        let (status, value) = admin(
            f.port,
            "PUT",
            "/v1/walls/layouts",
            Some(json!({"baseRevision":1,"layout":layout})),
        );
        assert_eq!(status, 200, "{value}");
        f.until(&mut output, |s| s["endpoints"][0]["appliedRevision"] == 2);
        proof["runningNative"] = f.phase(&mut output, "tile-identify-running.json");
        let second = command(&f);
        assert_ne!(
            second["endpoints"][0]["identification"]["requestId"],
            proof["unassignedCommand"]["endpoints"][0]["identification"]["requestId"]
        );
        let second_applied = f.until(&mut output, |s| {
            s["endpoints"][0]["identification"]["applied"] == true
        });
        assert!(second_applied["endpoints"][0]["appliedRevision"].is_null());
        proof["secondMarker"] = f.phase(&mut output, "tile-identify-marker-2.json");
        proof["secondApplied"] = second_applied;
        // Let the real 10-second expiry elapse naturally without relaxing the 30-second joint gate or forcibly deleting the marker from the test.
        proof["expired"] = f.until(&mut output, |s| {
            s["endpoints"][0].get("identification").is_none()
                && s["endpoints"][0]["appliedRevision"] == 2
        });
        proof["resumedNative"] = f.phase(&mut output, "tile-identify-resumed.json");
        fs::write(f.root.join("tile-stop"), b"stop").unwrap();
    }
    f.joined(&mut output);
    proof["cleanup"] =
        serde_json::from_slice(&fs::read(f.root.join("tile-identify-complete.json")).unwrap())
            .unwrap();
    assert_eq!(
        proof["cleanup"],
        json!({"revoked":revoked,"threadsJoined":true,"markerCleared":true,"dismissed":!revoked,"expiredAndResumed":!revoked,"physicalOutput":false,"ordinaryOutputExe":false})
    );
    proof["final"] = f.state();
    assert_eq!(proof["final"]["endpoints"][0]["online"], false);
    assert!(proof["final"]["endpoints"][0]
        .get("identification")
        .is_none());
    fs::write(
        f.root.join("tile-identification-service-oracle.json"),
        serde_json::to_vec_pretty(&proof).unwrap(),
    )
    .unwrap();
    server.finish().unwrap();
    if let Some(evidence) = std::env::var_os("OTTER_TILE_JOINT_EVIDENCE_DIR") {
        let evidence = PathBuf::from(evidence);
        assert!(evidence.is_absolute());
        fs::create_dir(&evidence).unwrap();
        for name in [
            "tile-identification-service-oracle.json",
            "identification_and_cleanup.stdout.log",
            "identification_and_cleanup.stderr.log",
        ] {
            fs::copy(f.root.join(name), evidence.join(name)).unwrap();
        }
    }
}
