// HTTP 接收独立常量 oracle；服务端登记/交付不伪装成 Otter 回调/正式结果已执行。
#[test]
fn local_art_trigger_http_claim_is_once_and_receipt_is_not_canonical_success() {
    let t = TriggerFixture::new();
    t.configure();
    t.observe(1, 100.);
    assert_eq!(
        t.inspect()["status"],
        "idle",
        "first matching sample is not a rising edge"
    );
    t.observe(2, 0.);
    t.observe(3, 100.);
    let pending = t.inspect();
    assert_eq!(pending["status"], "ready");
    assert_eq!(t.ack(&pending["deliveryId"], "no_result").0, 409);
    let (status, claimed) = t.send(t.op("claim"));
    assert_eq!(status, 200, "{claimed}");
    assert_eq!(claimed["status"], "claimed");
    assert_eq!(claimed["delivery"]["callback"], "onLiveTrigger");
    assert_eq!(claimed["delivery"]["observation"]["sequence"], 3);
    assert_eq!(claimed["delivery"]["observation"]["value"]["value"], 100.);
    assert_eq!(claimed["delivery"]["sourceDeviceId"], t.source);
    assert_eq!(claimed["binding"], t.f.request["binding"]);
    assert_eq!(claimed["deliveryId"], claimed["delivery"]["deliveryId"]);
    for _ in 0..3 {
        let again = t.send(t.op("claim"));
        assert_eq!(again.0, 200);
        assert_eq!(again.1["delivery"], Value::Null);
    }
    let id = &claimed["deliveryId"];
    assert_eq!(t.ack(id, "no_result").0, 200);
    assert_eq!(t.ack(id, "no_result").0, 200);
    assert_eq!(t.ack(id, "failed").0, 409);
    let result = t.inspect();
    assert_eq!(result["status"], "acknowledged");
    assert_eq!(result["receipt"], json!({"kind":"no_result"}));
    let shared = t.f.live_sessions.get(ART_TRIGGER_SESSION).unwrap();
    assert!(shared.triggers.is_empty() && shared.session.trigger_bindings.is_empty());
    assert!(shared.session.controller_device.is_none());
    assert!(
        shared.trigger_audits.is_empty(),
        "local receipt is not a Surface action audit"
    );
}
#[test]
fn local_art_trigger_http_preview_is_not_consent_and_auth_scope_is_exact() {
    let t = TriggerFixture::new();
    t.observe(1, 0.);
    t.observe(2, 100.);
    assert_eq!(t.send(t.op("claim")).0, 404);
    let body = t.body(t.configuration(1, 2));
    assert_eq!(
        public(t.f.port, "POST", ART_TRIGGER_ROUTE, Some(body.clone())).0,
        401
    );
    assert_eq!(
        admin(t.f.port, "POST", ART_TRIGGER_ROUTE, Some(body.clone())).0,
        403
    );
    for (pointer, bad) in [
        ("/binding/generation", json!(2)),
        ("/binding/workspaceId", json!("other")),
        ("/binding/packageDigest", json!("c".repeat(64))),
        ("/grant/signature", json!("AAAA")),
    ] {
        let mut changed = body.clone();
        *changed.pointer_mut(pointer).unwrap() = bad;
        assert_eq!(
            device(
                t.f.port,
                &t.f.token,
                "POST",
                ART_TRIGGER_ROUTE,
                Some(changed)
            )
            .0,
            403,
            "{pointer}"
        );
    }
    let mut changed = body.clone();
    changed["operation"]["configuration"]
        .as_object_mut()
        .unwrap()
        .remove("consent");
    assert_eq!(
        device(
            t.f.port,
            &t.f.token,
            "POST",
            ART_TRIGGER_ROUTE,
            Some(changed)
        )
        .0,
        400
    );
    t.configure();
    let mut wrong = t.op("claim");
    wrong["scope"]["epoch"] = json!(2);
    assert_eq!(t.send(wrong).0, 409);
    let mut wrong = t.op("claim");
    wrong["scope"]["authorizationId"] = json!("authorization:other");
    assert_eq!(t.send(wrong).0, 403);
}
#[test]
fn local_art_trigger_http_single_slot_never_overwrites_in_flight_or_replays_unknown_claim() {
    let mut t = TriggerFixture::new();
    t.ready();
    let (_, claimed) = t.send(t.op("claim"));
    let id = claimed["deliveryId"].clone();
    for index in 3..=50 {
        t.observe(index, if index % 2 == 0 { 100. } else { 0. });
    }
    assert_eq!(t.inspect()["deliveryId"], id);
    assert_eq!(t.send(t.op("claim")).1["delivery"], Value::Null);
    assert_eq!(t.ack(&id, "no_result").0, 200);
    t.observe(51, 0.);
    t.observe(52, 100.);
    let (_, second) = t.send(t.op("claim"));
    assert_ne!(second["deliveryId"], id);
    assert_eq!(second["delivery"]["observation"]["sequence"], 52);
    assert_eq!(t.ack(&id, "no_result").0, 409);
    assert_eq!(t.send(t.configuration(1, 3)).0, 409);
    let old_scope = t.scope.clone();
    t.scope["authorizationId"] = json!("authorization:2");
    assert_eq!(t.send(t.configuration(2, 3)).0, 200);
    assert_eq!(t.send(json!({"kind":"ack","scope":old_scope,"deliveryId":second["deliveryId"],"receipt":{"kind":"no_result"}})).0, 403);
    assert_eq!(t.inspect()["status"], "idle");
}
#[test]
fn local_art_trigger_http_policy_reapproval_does_not_revive_automatic_authority() {
    let t = TriggerFixture::new();
    t.ready();
    assert_eq!(t.f.approve(1, false).0, 200);
    assert_eq!(t.send(t.op("claim")).0, 403);
    assert_eq!(t.f.approve(2, true).0, 200);
    assert_eq!(t.send(t.op("claim")).0, 409);
    assert_eq!(t.inspect()["status"], "stopped");
    assert_eq!(t.send(t.op("claim")).0, 409);
    assert_eq!(t.send(t.op("stop")).0, 200);
    assert_eq!(t.f.approve(3, false).0, 200);
    assert_eq!(
        t.send(t.op("stop")).0,
        200,
        "reduction must survive Art approval revocation"
    );
}
#[test]
fn local_art_trigger_http_concurrent_claims_publish_exactly_one_payload() {
    let t = TriggerFixture::new();
    t.ready();
    let gate = Arc::new(std::sync::Barrier::new(3));
    let mut threads = Vec::new();
    for _ in 0..2 {
        let (port, token, body, gate) = (
            t.f.port,
            t.f.token.clone(),
            t.body(t.op("claim")),
            gate.clone(),
        );
        threads.push(std::thread::spawn(move || {
            gate.wait();
            device(port, &token, "POST", ART_TRIGGER_ROUTE, Some(body))
        }));
    }
    gate.wait();
    let replies: Vec<_> = threads
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect();
    assert!(replies.iter().all(|r| r.0 == 200), "{replies:?}");
    assert_eq!(
        replies
            .iter()
            .filter(|r| !r.1["delivery"].is_null())
            .count(),
        1
    );
    assert_eq!(t.inspect()["status"], "claimed");
}
