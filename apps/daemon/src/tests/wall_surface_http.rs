#[test]
fn wall_surface_http_rejects_foreign_event_identity_before_deduplicating_an_ack() {
    let fixture = WallSurfaceFixture::new();
    let state = fixture.open();
    fixture.acknowledge(1);
    let wall = fixture.event(&state, 1);
    let mut ordinary = wall["event"].clone();
    ordinary["attachmentId"] = json!(fixture.source_attachment);
    let (status, source_ack) = device(
        fixture.port,
        &fixture.token,
        "POST",
        &format!("/v1/surfaces/instances/{}/events", fixture.instance),
        Some(ordinary),
    );
    assert_eq!(status, 202, "{source_ack}");
    assert_eq!(source_ack["status"], "awaiting_confirmation");
    let (status, error) = fixture.call("event", wall);
    assert_eq!(status, 409, "{error}");
    assert_eq!(error["error"]["code"], "surface_conflict");
    let current = fixture.call("state", json!({"view":state["view"]})).1;
    assert_eq!(current["confirmations"], json!([]));
    assert_eq!(current["pending"], json!([]));
    let source = fixture.source();
    let confirmations = source["pendingConfirmations"].as_object().unwrap();
    assert_eq!(confirmations.len(), 1);
    assert_eq!(
        confirmations.values().next().unwrap()["request"]["attachmentId"],
        fixture.source_attachment
    );
    assert_eq!(source["eventAcks"]["wall-event-1"], source_ack);

    let next = fixture.event(&state, 2);
    assert_eq!(fixture.call("event", next.clone()).0, 202);
    let mut claimed = next["event"].clone();
    claimed["attachmentId"] = json!(fixture.source_attachment);
    let (status, _) = device(
        fixture.port,
        &fixture.token,
        "POST",
        &format!("/v1/surfaces/instances/{}/events", fixture.instance),
        Some(claimed),
    );
    assert_eq!(
        status, 409,
        "ordinary attachment must not claim a wall acknowledgement"
    );
    let mut repeated = next;
    repeated["sequence"] = json!(3);
    assert_eq!(
        fixture.call("event", repeated).0,
        202,
        "same-view idempotency remains valid"
    );
}

#[test]
fn wall_surface_http_grants_are_ephemeral_and_cannot_escape_through_surface_routes() {
    let fixture = WallSurfaceFixture::new();
    let state = fixture.open();
    assert_eq!(state["snapshot"]["authoritativeState"]["value"], 7);
    assert_eq!(state["width"], 640);
    assert_eq!(state["snapshot"]["resourceLeases"], json!([]));
    assert_eq!(fixture.open()["view"], state["view"]);
    assert_eq!(
        fixture.source()["attachments"].as_object().unwrap().len(),
        2
    );
    let body = json!({"view":state["view"],"snapshotRevision":state["snapshot"]["revision"]});
    assert!(fixture.call("state", body.clone()).1["snapshot"].is_null());
    let (_, outsider) = pair(fixture.port, "Other Art tile");
    assert_eq!(
        device(
            fixture.port,
            &outsider,
            "POST",
            "/v1/walls/surfaces/state",
            Some(body.clone())
        )
        .0,
        403
    );
    assert_eq!(
        admin(
            fixture.port,
            "POST",
            "/v1/walls/surfaces/state",
            Some(body.clone())
        )
        .0,
        401
    );
    let event = fixture.event(&state, 1);
    assert_eq!(
        device(
            fixture.port,
            &fixture.token,
            "POST",
            &format!("/v1/surfaces/instances/{}/events", fixture.instance),
            Some(event["event"].clone())
        )
        .0,
        403
    );
    let (_, stream) = device(
        fixture.port,
        &fixture.token,
        "GET",
        "/v1/surfaces/stream?after=0&timeoutMs=0",
        None,
    );
    assert!(!stream
        .to_string()
        .contains(state["view"]["attachmentId"].as_str().unwrap()));
    assert_eq!(fixture.call("close", json!({"view":state["view"]})).0, 200);
    let source = fixture.source();
    assert_eq!(source["attachments"].as_object().unwrap().len(), 1);
    assert!(source["attachments"]
        .get(&fixture.source_attachment)
        .is_some());
    assert_eq!(source["authoritativeState"]["value"], 7);
    assert_eq!(fixture.call("state", body).0, 409);
}

#[test]
fn wall_surface_http_layout_rebind_discards_old_requests_and_preserves_source() {
    let mut fixture = WallSurfaceFixture::new();
    let old = fixture.open();
    fixture.acknowledge(1);
    fixture.layout["revision"] = json!(3);
    fixture.layout["placements"][0]["sourceCrop"] =
        json!({"x":0.2,"y":0.1,"width":0.6,"height":0.8});
    assert_eq!(
        admin(
            fixture.port,
            "PUT",
            "/v1/walls/layouts",
            Some(json!({"baseRevision":2,"layout":fixture.layout}))
        )
        .0,
        200
    );
    fixture.binding["revision"] = json!(3);
    let current = fixture.open();
    assert_eq!(current["snapshot"]["authoritativeState"]["value"], 7);
    assert_eq!(fixture.call("event", fixture.event(&old, 1)).0, 403);
    assert_eq!(fixture.call("close", json!({"view":old["view"]})).0, 200);
    assert_eq!(
        fixture.call("state", json!({"view":current["view"]})).0,
        200
    );
    assert_eq!(
        device(
            fixture.port,
            &fixture.token,
            "POST",
            "/v1/walls/disconnect",
            Some(json!({
                "endpointId":"art-tile","leaseId":fixture.binding["leaseId"]
            }))
        )
        .0,
        200
    );
    assert_eq!(
        fixture.source()["attachments"].as_object().unwrap().len(),
        1
    );
    assert_eq!(fixture.source()["authoritativeState"]["value"], 7);
}

#[test]
fn wall_surface_http_actions_need_applied_mapping_and_host_owned_confirmation() {
    let fixture = WallSurfaceFixture::new();
    let state = fixture.open();
    let request = fixture.event(&state, 1);
    assert_eq!(fixture.call("event", request.clone()).0, 409);
    fixture.acknowledge(1);
    let mut outside = request.clone();
    outside["pixel"] = json!({"x":801,"y":1});
    assert_eq!(fixture.call("event", outside).0, 400);
    let (status, ack) = fixture.call("event", request.clone());
    assert_eq!(status, 202, "{ack}");
    assert_eq!(ack["status"], "awaiting_confirmation");
    assert_eq!(fixture.call("event", request).0, 409);
    let pending = fixture.call("state", json!({"view":state["view"]})).1;
    let confirmation = &pending["confirmations"][0];
    assert_eq!(confirmation["deviceId"], fixture.owner);
    assert_eq!(pending["snapshot"]["authoritativeState"]["value"], 7);
    let direct = json!({"protocolVersion":"loom.surface.v1", "confirmationId":confirmation["confirmationId"],
        "instanceId":fixture.instance, "attachmentId":state["view"]["attachmentId"],"deviceId":fixture.owner,"approved":true});
    assert_eq!(
        device(
            fixture.port,
            &fixture.token,
            "POST",
            "/v1/surfaces/confirmations/decision",
            Some(direct)
        )
        .0,
        403
    );
    let direct_cancel = json!({"protocolVersion":"loom.surface.v1","instanceId":fixture.instance,
        "requestId":ack["requestId"],"deviceId":fixture.owner});
    assert_eq!(
        device(
            fixture.port,
            &fixture.token,
            "POST",
            "/v1/surfaces/actions/cancel",
            Some(direct_cancel)
        )
        .0,
        403
    );
    let (status, cancelled) = fixture.call(
        "confirmation",
        json!({
            "view":state["view"], "placementId":"art", "pixel":{"x":100,"y":100},
            "confirmationId":confirmation["confirmationId"], "approved":false
        }),
    );
    assert_eq!(status, 200, "{cancelled}");
    assert_eq!(cancelled["status"], "cancelled");
    assert!(
        fixture.call("state", json!({"view":state["view"]})).1["confirmations"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn wall_surface_http_display_only_endpoint_cannot_invent_input_capabilities() {
    let fixture = WallSurfaceFixture::with_inputs(json!([]));
    let state = fixture.open();
    let source = fixture.source();
    let capabilities = &source["attachments"][state["view"]["attachmentId"].as_str().unwrap()]
        ["hostCapabilities"]["input"];
    assert_eq!(
        capabilities,
        &json!({"pointer":false,"hover":false,"touch":false,"keyboard":false})
    );
    fixture.acknowledge(1);
    let (status, error) = fixture.call("event", fixture.event(&state, 1));
    assert_eq!(status, 403);
    assert_eq!(error["error"]["code"], "wall_surface_input_unavailable");
}

#[test]
fn wall_surface_http_rejects_ambiguous_independent_source_views() {
    let fixture = WallSurfaceFixture::new();
    let (_, attached) = admin(
        fixture.port,
        "POST",
        &format!("/v1/surfaces/instances/{}/attachments", fixture.instance),
        Some(json!({"hookNodeId":"second-source", "deviceId":fixture.owner})),
    );
    let mut snapshot =
        fixture.source()["attachments"][&fixture.source_attachment]["snapshot"].clone();
    snapshot["attachmentId"] = attached["descriptor"]["attachmentId"].clone();
    assert_eq!(
        admin(
            fixture.port,
            "PUT",
            &format!("/v1/surfaces/instances/{}/snapshot", fixture.instance),
            Some(snapshot)
        )
        .0,
        200
    );
    let (status, response) = fixture.call(
        "open",
        json!({"binding":fixture.binding,"instanceId":fixture.instance}),
    );
    assert_eq!(status, 409, "{response}");
    assert_eq!(response["error"]["code"], "surface_conflict");
    assert_eq!(
        fixture.source()["attachments"].as_object().unwrap().len(),
        2
    );
}

#[test]
fn wall_surface_http_failed_execution_is_sanitized_and_scoped_to_its_view() {
    let fixture = WallSurfaceFixture::new();
    let state = fixture.open();
    fixture.acknowledge(1);
    let (_, ack) = fixture.call("event", fixture.event(&state, 1));
    let pending = fixture.call("state", json!({"view":state["view"]})).1;
    let (status, _) = fixture.call(
        "confirmation",
        json!({
            "view":state["view"], "placementId":"art", "pixel":{"x":100,"y":100},
            "confirmationId":pending["confirmations"][0]["confirmationId"], "approved":true
        }),
    );
    assert_eq!(status, 200);
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    let failed = loop {
        let response = fixture.call("state", json!({"view":state["view"]})).1;
        if !response["failure"].is_null() {
            break response;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "fixture execution did not report failure"
        );
        thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(
        failed["failure"],
        json!({"requestId":ack["requestId"], "code":"wall_surface_action_failed"})
    );
    assert_eq!(failed["snapshot"]["authoritativeState"]["value"], 7);
    assert_eq!(fixture.call("close", json!({"view":state["view"]})).0, 200);
    assert!(fixture.open()["failure"].is_null());
}

#[test]
fn wall_surface_http_resource_grants_expire_without_releasing_source_leases_or_conflating_commits()
{
    let fixture = WallSurfaceFixture::new();
    let (status, uploaded) = admin(
        fixture.port,
        "POST",
        "/v1/surfaces/resources",
        Some(json!({
            "kind":"image", "mime":"application/x-neuro-rgba8", "width":1, "height":1,
            "dataBase64":BASE64.encode([255,0,0,255])
        })),
    );
    assert_eq!(status, 201);
    let mut snapshot =
        fixture.source()["attachments"][&fixture.source_attachment]["snapshot"].clone();
    snapshot["revision"] = json!(snapshot["revision"].as_u64().unwrap() + 1);
    snapshot["resources"] = json!([uploaded["resource"]]);
    snapshot["resourceLeases"] = json!([uploaded]);
    assert_eq!(
        admin(
            fixture.port,
            "PUT",
            &format!("/v1/surfaces/instances/{}/snapshot", fixture.instance),
            Some(snapshot)
        )
        .0,
        200
    );
    for (route, commit) in [
        (
            "preview",
            json!({"previewRevision":1,"portId":"preview","value":{"kind":"value","value":"temporary"}}),
        ),
        (
            "result",
            json!({"resultRevision":1,"outputs":{"output":{"kind":"value","value":"formal"}}}),
        ),
    ] {
        let mut body = json!({"protocolVersion":"loom.surface.v1","instanceId":fixture.instance,"requestId":"source-result","generation":0});
        body.as_object_mut()
            .unwrap()
            .extend(commit.as_object().unwrap().clone());
        assert_eq!(
            admin(
                fixture.port,
                "POST",
                &format!("/v1/surfaces/instances/{}/{route}", fixture.instance),
                Some(body)
            )
            .0,
            200
        );
    }
    let state = fixture.open();
    assert_eq!(state["snapshot"]["resourceLeases"], json!([]));
    assert_eq!(state["preview"]["value"]["value"], "temporary");
    assert_eq!(state["result"]["outputs"]["output"]["value"], "formal");
    let image = json!({"view":state["view"], "resourceId":uploaded["resource"]["resourceId"]});
    let (status, read) = fixture.call("image", image.clone());
    assert_eq!(status, 200, "{read}");
    assert_eq!(
        BASE64.decode(read["dataBase64"].as_str().unwrap()).unwrap(),
        [255, 0, 0, 255]
    );
    assert_eq!(
        fixture
            .call(
                "image",
                json!({"view":state["view"],"resourceId":format!("sha256:{}", "0".repeat(64))})
            )
            .0,
        403
    );
    assert_eq!(fixture.call("close", json!({"view":state["view"]})).0, 200);
    assert_eq!(fixture.call("image", image).0, 409);
    assert_eq!(
        fixture.source()["latestResult"]["outputs"]["output"]["value"],
        "formal"
    );
    let source = fixture.source();
    assert_eq!(
        source["attachments"][&fixture.source_attachment]["snapshot"]["resourceLeases"][0]
            ["leaseId"],
        uploaded["leaseId"]
    );
}
