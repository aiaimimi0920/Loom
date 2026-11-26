// Reuses real LAN, pairing, and windowed child; independently asserts source instance liveness, single ephemeral view, and bounded resource updates.
#[test]
#[ignore = "explicit real Loom LAN Art image mixed Tile view update freeze black resume stop"]
fn real_lan_native_tile_art_views_images_and_cleanup() {
    art_run(false);
}
#[test]
#[ignore = "explicit real Loom LAN Art Tile authorization revocation and view cleanup"]
fn real_lan_native_tile_art_revocation_reclaims_views() {
    art_run(true);
}
fn art_run(revoked: bool) {
    let (f, device, mut server, _sessions) = setup_configure(true);
    let (status, created) = admin(
        f.port,
        "POST",
        "/v1/surfaces/instances",
        Some(json!({"artId":"wall-art"})),
    );
    assert_eq!(status, 201, "{created}");
    let instance = created["descriptor"]["instanceId"].as_str().unwrap();
    let mut capabilities = default_declarative_surface_host_capabilities();
    capabilities.capabilities.push("remote_resources".into());
    let (status, attached) = admin(
        f.port,
        "POST",
        &format!("/v1/surfaces/instances/{instance}/attachments"),
        Some(json!({"hookNodeId":"source-art","deviceId":device,"capabilities":capabilities})),
    );
    assert_eq!(status, 201, "{attached}");
    let source = attached["descriptor"]["attachmentId"].as_str().unwrap();
    assert_eq!(
        admin(
            f.port,
            "POST",
            &format!("/v1/surfaces/instances/{instance}/mount"),
            Some(json!({"attachmentId":source}))
        )
        .0,
        200
    );
    let (status, resource) = admin(
        f.port,
        "POST",
        "/v1/surfaces/resources",
        Some(
            json!({"kind":"image","mime":"application/x-neuro-rgba8","width":2,"height":2,"dataBase64":BASE64.encode([0,255,0,255].repeat(4))}),
        ),
    );
    assert_eq!(status, 201, "{resource}");
    let mut output = f.child("art_and_cleanup", revoked);
    let registered = f.until(&mut output, |s| {
        s["endpoints"]
            .as_array()
            .is_some_and(|e| e.len() == 1 && e[0]["online"] == true)
    });
    let endpoint = &registered["endpoints"][0]["endpoint"];
    assert_eq!(
        endpoint["renderModes"],
        json!(["image", "raw_bgra", "surface_v1"])
    );
    assert_eq!(endpoint["inputCapabilities"], json!(["pointer"]));
    let layout = json!({"protocolVersion":"loom.wall.v1","wallId":"native-image-wall","revision":2,"bounds":{"x":0,"y":0,"width":4,"height":2},
        "tiles":[{"tileId":"native-tile","endpointId":endpoint["endpointId"],"rect":{"x":0,"y":0,"width":4,"height":2},"rotation":"deg0"}],
        "placements":[{"placementId":"image","source":{"kind":"image","id":resource["resource"]["resourceId"]},"rect":{"x":0,"y":0,"width":2,"height":2},"sourceCrop":{"x":0,"y":0,"width":1,"height":1},"zIndex":0,"interactive":false},
        {"placementId":"art1","source":{"kind":"surface","id":instance},"rect":{"x":2,"y":0,"width":1,"height":2},"sourceCrop":{"x":0,"y":0,"width":0.5,"height":1},"zIndex":1,"interactive":false},
        {"placementId":"art2","source":{"kind":"surface","id":instance},"rect":{"x":3,"y":0,"width":1,"height":2},"sourceCrop":{"x":0.5,"y":0,"width":0.5,"height":1},"zIndex":2,"interactive":false}]});
    let (status, scheduled) = admin(
        f.port,
        "PUT",
        "/v1/walls/layouts",
        Some(json!({"baseRevision":1,"layout":layout})),
    );
    assert_eq!(status, 200, "{scheduled}");
    let first = f.phase(&mut output, "tile-art-first.json");
    let read = || {
        admin(
            f.port,
            "GET",
            &format!("/v1/surfaces/instances/{instance}"),
            None,
        )
        .1
    };
    let current = read();
    let attachments = current["attachments"].as_object().unwrap();
    assert_eq!(attachments.len(), 2);
    let (view, record) = attachments
        .iter()
        .find(|(id, _)| id.as_str() != source)
        .unwrap();
    // `ephemeral` and `mirror_of` are intentionally omitted from serialization; public identity and final attachment reclamation jointly prove view ownership and lifetime.
    assert!(record.get("ephemeral").is_none());
    assert_eq!(record["descriptor"]["deviceId"], device);
    assert_eq!(record["descriptor"]["hookNodeId"], format!("wall:{}", endpoint["endpointId"].as_str().unwrap()));
    assert_eq!(record["descriptor"]["attachmentId"], view.as_str());
    let mut snapshot = record["snapshot"].clone();
    snapshot["revision"] = json!(snapshot["revision"].as_u64().unwrap() + 1);
    snapshot["resources"] = json!([resource["resource"]]);
    snapshot["resourceLeases"] = json!([resource]);
    snapshot["scene"] = json!({"id":"root","type":"image","props":{"resourceId":resource["resource"]["resourceId"]},"layout":{"width":640,"height":480}});
    let (status, changed) = admin(
        f.port,
        "PUT",
        &format!("/v1/surfaces/instances/{instance}/snapshot"),
        Some(snapshot),
    );
    assert_eq!(status, 200, "{changed}");
    let updated = f.phase(&mut output, "tile-art-updated.json");
    assert_eq!(updated["sourcePbgra"], json!([0, 255, 0, 255]));
    // Consecutive null-snapshot polls must reuse the same view rather than creating a second source or attachment.
    thread::sleep(Duration::from_millis(750));
    assert_eq!(read()["attachments"].as_object().unwrap().len(), 2);
    assert!(read()["attachments"].get(view).is_some());
    let mut proof = json!({"first":first,"updated":updated,"oneViewForTwoPlacements":true,"placementsRemainReadOnly":true});
    if revoked {
        assert_eq!(admin(f.port,"PUT",&format!("/v1/devices/{device}"),Some(json!({"name":"Tile test","kind":"computer","address":"127.0.0.1","enabled":false}))).0,200);
    } else {
        proof["frozen"] = f.present(&mut output, "frozen");
        f.phase(&mut output, "tile-art-frozen.json");
        proof["black"] = f.present(&mut output, "black");
        f.phase(&mut output, "tile-art-black.json");
        proof["resumed"] = f.present(&mut output, "running");
        f.phase(&mut output, "tile-art-resumed.json");
        fs::write(f.root.join("tile-stop"), b"stop").unwrap();
    }
    f.joined(&mut output);
    let complete: Value =
        serde_json::from_slice(&fs::read(f.root.join("tile-output-complete.json")).unwrap())
            .unwrap();
    assert_eq!(
        complete,
        json!({"art":true,"revoked":revoked,"pixelsReleased":true,"threadsJoined":true,"ordinaryOutputExe":false,"finalSwapChainReadback":false})
    );
    while read()["attachments"].as_object().unwrap().len() != 1 {
        assert!(Instant::now() < f.deadline);
        thread::sleep(Duration::from_millis(10));
    }
    let source_record = read();
    assert!(source_record["attachments"].get(source).is_some());
    assert_eq!(source_record["authoritativeState"]["value"], 7);
    proof["cleanup"] = complete;
    proof["sourcePreserved"] = json!(true);
    server.finish().unwrap();
    if let Some(dir) = std::env::var_os("OTTER_TILE_JOINT_EVIDENCE_DIR") {
        let dir = PathBuf::from(dir);
        assert!(dir.is_absolute());
        fs::create_dir(&dir).unwrap();
        fs::write(
            dir.join("tile-art-service-oracle.json"),
            serde_json::to_vec_pretty(&proof).unwrap(),
        )
        .unwrap();
        for name in ["art_and_cleanup.stdout.log", "art_and_cleanup.stderr.log"] {
            fs::copy(f.root.join(name), dir.join(name)).unwrap();
        }
    }
}
