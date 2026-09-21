#[test]
fn wall_image_http_requires_current_owned_presenter_and_exact_image_reference() {
    let root = Root::new();
    let (port, mut server) = start(&root.0);
    let (owner, token) = pair(port, "Image tile");
    let (_, outsider) = pair(port, "Other tile");
    let (status, uploaded) = admin(
        port,
        "POST",
        "/v1/surfaces/resources",
        Some(json!({
            "kind": "image", "mime": "application/x-neuro-rgba8", "width": 1, "height": 1,
            "dataBase64": BASE64.encode([255, 0, 0, 255]),
        })),
    );
    assert_eq!(status, 201, "{uploaded}");
    let id = uploaded["resource"]["resourceId"].clone();
    let mut fixture: Value = serde_json::from_str(include_str!(
        "../../../../protocol/fixtures/wall-geometry.v1.json"
    ))
    .unwrap();
    fixture["endpoints"][0]["deviceId"] = json!(owner);
    assert_eq!(
        device(
            port,
            &token,
            "POST",
            "/v1/walls/endpoints/register",
            Some(json!({
                "baseRevision": 0, "endpoint": fixture["endpoints"][0]
            }))
        )
        .0,
        200
    );
    let mut layout = fixture["layout"].clone();
    layout["revision"] = json!(2);
    layout["tiles"] = json!([fixture["layout"]["tiles"][0]]);
    layout["placements"] = json!([fixture["layout"]["placements"][0]]);
    layout["placements"][0]["source"] = json!({"kind": "image", "id": id});
    layout["placements"][0]["interactive"] = json!(false);
    assert_eq!(
        admin(
            port,
            "PUT",
            "/v1/walls/layouts",
            Some(json!({
                "baseRevision": 1, "layout": layout
            }))
        )
        .0,
        200
    );
    let (status, lease) = device(
        port,
        &token,
        "POST",
        "/v1/walls/connect",
        Some(json!({"endpointId": "endpoint-left"})),
    );
    assert_eq!(status, 200);
    let request = json!({"endpointId": "endpoint-left", "leaseId": lease["leaseId"],
        "revision": 2, "resourceId": id});
    let route = "/v1/walls/images/read";
    assert_eq!(public(port, "POST", route, Some(request.clone())).0, 401);
    assert_eq!(admin(port, "POST", route, Some(request.clone())).0, 401);
    assert_eq!(
        device(port, &outsider, "POST", route, Some(request.clone())).0,
        403
    );
    let (status, read) = device(port, &token, "POST", route, Some(request.clone()));
    assert_eq!(status, 200, "{read}");
    assert_eq!(read["resource"], uploaded["resource"]);
    assert_eq!(
        BASE64.decode(read["dataBase64"].as_str().unwrap()).unwrap(),
        [255, 0, 0, 255]
    );
    for (field, value, status) in [
        (
            "resourceId",
            json!(format!("sha256:{}", "0".repeat(64))),
            403,
        ),
        ("revision", json!(1), 409),
        ("leaseId", json!("wrong-lease"), 409),
    ] {
        let mut denied = request.clone();
        denied[field] = value;
        assert_eq!(device(port, &token, "POST", route, Some(denied)).0, status);
    }
    layout["revision"] = json!(3);
    layout["placements"] = json!([]);
    assert_eq!(
        admin(
            port,
            "PUT",
            "/v1/walls/layouts",
            Some(json!({
                "baseRevision": 2, "layout": layout
            }))
        )
        .0,
        200
    );
    assert_eq!(
        device(port, &token, "POST", route, Some(request.clone())).0,
        409
    );
    let mut unreferenced = request.clone();
    unreferenced["revision"] = json!(3);
    assert_eq!(
        device(port, &token, "POST", route, Some(unreferenced)).0,
        403
    );
    assert_eq!(
        admin(
            port,
            "PUT",
            &format!("/v1/devices/{owner}"),
            Some(json!({
                "name": "Image tile", "kind": "computer", "address": "127.0.0.1", "enabled": false
            }))
        )
        .0,
        200
    );
    assert_eq!(device(port, &token, "POST", route, Some(request)).0, 401);
    server.finish().unwrap();
}

#[test]
fn wall_image_gc_retains_persisted_references_after_surface_lease_release() {
    let root = Root::new();
    fs::create_dir_all(&root.0).unwrap();
    let wall_path = root.0.join("walls");
    let mut resource_store = SurfaceResourceStore::new(root.0.join("resources")).unwrap();
    let resource = resource_store
        .register(
            SurfaceResourceKind::Image,
            "image/png",
            b"wall-image",
            None,
            None,
            None,
        )
        .unwrap();
    resource_store.release(&resource.lease_id).unwrap();
    drop(resource_store);
    // Age only this fixture's metadata, then reopen through normal persistence validation.
    let digest = resource
        .resource
        .resource_id
        .strip_prefix("sha256:")
        .unwrap();
    let metadata_path = root.0.join("resources").join(format!("{digest}.json"));
    let mut metadata: Value = serde_json::from_slice(&fs::read(&metadata_path).unwrap()).unwrap();
    metadata["createdAtMs"] = json!(0);
    fs::write(&metadata_path, serde_json::to_vec(&metadata).unwrap()).unwrap();
    let resource_store = SurfaceResourceStore::new(root.0.join("resources")).unwrap();
    let resources = Arc::new(Mutex::new(resource_store));
    let instances = Arc::new(Mutex::new(
        SurfaceInstanceStore::new(root.0.join("instances.json")).unwrap(),
    ));
    let walls = Arc::new(WallStore::open(&wall_path).unwrap());
    let mut fixture: Value = serde_json::from_str(include_str!(
        "../../../../protocol/fixtures/wall-geometry.v1.json"
    ))
    .unwrap();
    for (index, endpoint) in fixture["endpoints"].as_array().unwrap().iter().enumerate() {
        walls
            .register(
                index as u64,
                serde_json::from_value(endpoint.clone()).unwrap(),
                None,
            )
            .unwrap();
    }
    fixture["layout"]["revision"] = json!(3);
    fixture["layout"]["placements"][0]["source"] =
        json!({"kind": "image", "id": resource.resource.resource_id});
    fixture["layout"]["placements"][0]["interactive"] = json!(false);
    walls
        .put_layout(
            2,
            serde_json::from_value(fixture["layout"].clone()).unwrap(),
        )
        .unwrap();
    drop(walls);
    let walls = Arc::new(WallStore::open(&wall_path).unwrap());
    let kept = collect_surface_resource_garbage(&instances, &resources, &walls).unwrap();
    assert_eq!(kept.retained_objects, 1);
    assert_eq!(kept.removed_objects, 0);
    walls.remove_layout(3, "living-room").unwrap();
    let swept = collect_surface_resource_garbage(&instances, &resources, &walls).unwrap();
    assert_eq!(swept.removed_objects, 1);
    assert_eq!(swept.retained_objects, 0);
}
