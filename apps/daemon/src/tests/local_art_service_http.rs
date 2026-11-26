// Real HTTP → current issuer verification → bounded task → signed child → terminal response.
impl Fixture {
    fn service_submit(&self, call: &str, input: Value, timeout: u64) -> Value {
        let (status, prepared) = self.prepare(self.request.clone());
        assert_eq!(status, 200, "{prepared}");
        let bytes = serde_json::to_vec(&prepared["grant"]).unwrap();
        let grant = loom_plugin_security::local_art_grant::verify(
            &bytes,
            self.issuer["keyId"].as_str().unwrap(),
            self.issuer["publicKey"].as_str().unwrap(),
        )
        .unwrap();
        json!({"protocolVersion":"loom.art.local.service.v1","owner":{"binding":grant.binding,"grantId":grant.grant_id,"callId":call,"service":SERVICE_ID},"grant":prepared["grant"],"input":input,"timeoutMs":timeout})
    }
    fn service(&self, action: &str, body: Value) -> (u16, Value) {
        device(
            self.port,
            &self.token,
            "POST",
            &format!("/v1/art/local/service/{action}"),
            Some(body),
        )
    }
    fn service_control(body: &Value) -> Value {
        json!({"protocolVersion":body["protocolVersion"],"owner":body["owner"],"grant":body["grant"]})
    }
    fn service_started(&self, submit: &Value) {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let (status, reply) = self.service("poll", Self::service_control(submit));
            assert_eq!(status, 200, "{reply}");
            if reply["progress"]["step"] == 1 {
                return;
            }
            assert!(Instant::now() < deadline, "missing real progress: {reply}");
            thread::sleep(Duration::from_millis(10));
        }
    }
    fn service_final(&self, submit: &Value) -> Value {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            let (status, reply) = self.service("poll", Self::service_control(submit));
            assert_eq!(status, 200, "{reply}");
            if matches!(
                reply["status"].as_str(),
                Some("succeeded" | "failed" | "cancelled")
            ) {
                return reply;
            }
            assert!(Instant::now() < deadline, "service did not settle: {reply}");
            thread::sleep(Duration::from_millis(10));
        }
    }
}
#[test]
fn local_art_service_http_executes_once_and_preserves_null_terminal_output() {
    let f = Fixture::with_service();
    assert_eq!(f.approve(0, true).0, 200);
    let submit = f.service_submit("call:one", json!({}), 2000);
    assert_eq!(f.service("submit", submit.clone()).0, 200);
    let final_reply = f.service_final(&submit);
    assert_eq!(final_reply["status"], "succeeded", "{final_reply}");
    assert_eq!(final_reply["output"]["calls"], 1);
    let (_, duplicate) = f.service("submit", submit.clone());
    assert_eq!(duplicate, final_reply);
    assert_eq!(
        f.service("cancel", Fixture::service_control(&submit)).1,
        final_reply
    );
    let mut conflict = submit.clone();
    conflict["input"] = json!({"different":true});
    assert_eq!(f.service("submit", conflict).0, 409);
    let mut null = submit.clone();
    null["owner"]["callId"] = json!("call:null");
    null["input"] = json!({"null":true});
    assert_eq!(f.service("submit", null.clone()).0, 200);
    let reply = f.service_final(&null);
    assert_eq!(reply["status"], "succeeded", "{reply}");
    let parsed: loom_protocol::local_art::service::Reply = serde_json::from_value(reply).unwrap();
    assert_eq!(parsed.output, Some(Value::Null));
    assert!(parsed.validate(&parsed.owner).is_ok());
}
#[test]
fn local_art_service_http_rejects_forgery_cross_owner_and_revoked_approval() {
    let f = Fixture::with_service();
    assert_eq!(f.approve(0, true).0, 200);
    let submit = f.service_submit("call:scope", json!({"hang":true}), 5000);
    let mut forged = submit.clone();
    forged["grant"]["signature"] = json!("invalid");
    assert_eq!(f.service("submit", forged).0, 403);
    let mut scope = submit.clone();
    scope["owner"]["binding"]["generation"] = json!(2);
    assert_eq!(f.service("submit", scope).0, 403);
    let mut scope = submit.clone();
    scope["owner"]["service"] = json!("capability:publisher.example:admin.run");
    assert_eq!(f.service("submit", scope).0, 403);
    assert_eq!(f.service("submit", submit.clone()).0, 200);
    let (_, other_token) = pair(f.port, "other service client");
    assert_eq!(
        device(
            f.port,
            &other_token,
            "POST",
            "/v1/art/local/service/poll",
            Some(Fixture::service_control(&submit))
        )
        .0,
        403
    );
    assert_eq!(f.approve(1, false).0, 200);
    assert_eq!(f.service("poll", Fixture::service_control(&submit)).0, 403);
    assert_eq!(f.service("submit", submit).0, 403);
}
#[test]
fn local_art_service_http_other_session_cannot_cancel_owner_task() {
    let f = Fixture::with_service();
    assert_eq!(f.approve(0, true).0, 200);
    let submit = f.service_submit("call:other-session", json!({"hang":true}), 5000);
    assert_eq!(f.service("submit", submit.clone()).0, 200);
    f.service_started(&submit);
    let (status, session) = issue_session(f.port, &f.device, &f.key);
    assert_eq!(status, 201, "{session}");
    let token = session["token"].as_str().expect("device session token");
    let (status, reply) = device(
        f.port,
        token,
        "POST",
        "/v1/art/local/service/poll",
        Some(Fixture::service_control(&submit)),
    );
    assert_eq!(status, 403, "{reply}");
    let (status, reply) = device(
        f.port,
        token,
        "POST",
        "/v1/art/local/service/cancel",
        Some(Fixture::service_control(&submit)),
    );
    assert_eq!(status, 403, "{reply}");
    let (status, reply) = f.service("poll", Fixture::service_control(&submit));
    assert_eq!(status, 200, "{reply}");
    assert_eq!(reply["status"], "running");
    let (status, reply) = f.service("cancel", Fixture::service_control(&submit));
    assert_eq!(status, 200, "{reply}");
    assert_eq!(reply["status"], "cancelled");
}

#[test]
fn local_art_service_http_cancel_timeout_and_output_limit_are_terminal() {
    let f = Fixture::with_service();
    assert_eq!(f.approve(0, true).0, 200);
    let submit = f.service_submit("call:cancel", json!({"hang":true}), 5000);
    assert_eq!(f.service("submit", submit.clone()).0, 200);
    f.service_started(&submit);
    assert_eq!(
        f.service("cancel", Fixture::service_control(&submit)).1["status"],
        "cancelled"
    );
    assert_eq!(f.service_final(&submit)["status"], "cancelled");
    // A new invocation is dispatched only after cancellation's worker has released admission.
    let cleanup_deadline = Instant::now() + Duration::from_secs(3);
    while !f.runtime.process_ids().is_empty() {
        assert!(
            Instant::now() < cleanup_deadline,
            "cancelled child was not reaped"
        );
        thread::sleep(Duration::from_millis(5));
    }
    let timeout = f.service_submit("call:timeout", json!({"hang":true}), 100);
    assert_eq!(f.service("submit", timeout.clone()).0, 200);
    assert_eq!(
        f.service_final(&timeout)["error"],
        "local_art_service_timeout"
    );
    let large = f.service_submit("call:large", json!({"large":true}), 2000);
    assert_eq!(f.service("submit", large.clone()).0, 200);
    assert_eq!(f.service_final(&large)["error"], "local_art_result_limit");
}

#[test]
fn local_art_service_http_shutdown_joins_running_task_and_process() {
    let f = Fixture::with_service();
    assert_eq!(f.approve(0, true).0, 200);
    let submit = f.service_submit("call:shutdown", json!({"hang":true}), 5000);
    assert_eq!(f.service("submit", submit.clone()).0, 200);
    f.service_started(&submit);
    let runtime = f.runtime.clone();
    drop(f);
    assert!(runtime.process_ids().is_empty());
}

#[test]
fn local_art_service_http_retention_is_bounded_without_replaying_completed_calls() {
    let f = Fixture::with_service();
    assert_eq!(f.approve(0, true).0, 200);
    let first = f.service_submit("call:0", json!({}), 2000);
    for index in 0..32 {
        let mut submit = first.clone();
        submit["owner"]["callId"] = json!(format!("call:{index}"));
        let (status, reply) = f.service("submit", submit.clone());
        assert_eq!(status, 200, "{reply}");
        assert_eq!(f.service_final(&submit)["status"], "succeeded");
    }
    let mut excess = first.clone();
    excess["owner"]["callId"] = json!("call:excess");
    assert_eq!(f.service("submit", excess).0, 429);
    let (status, reply) = f.service("submit", first);
    assert_eq!(status, 200);
    assert_eq!(reply["output"]["calls"], 1);
}

#[test]
fn local_art_service_http_device_disable_cancels_without_polling() {
    let f = Fixture::with_service();
    assert_eq!(f.approve(0, true).0, 200);
    let submit = f.service_submit("call:device", json!({"hang":true}), 5000);
    assert_eq!(f.service("submit", submit.clone()).0, 200);
    // Accepted does not prove Command started: activate prewarms a shared runtime.
    // Observe execution before revocation; no poll drives cleanup after revocation.
    f.service_started(&submit);
    let (status, body) = admin(
        f.port,
        "PUT",
        &format!("/v1/devices/{}", f.device),
        Some(
            json!({"name":"Local Art client","kind":"computer","address":"127.0.0.1","enabled":false}),
        ),
    );
    assert_eq!(status, 200, "{body}");
    let deadline = Instant::now() + Duration::from_secs(3);
    while !f.runtime.process_ids().is_empty() {
        assert!(
            Instant::now() < deadline,
            "revocation did not reap service process"
        );
        thread::sleep(Duration::from_millis(10));
    }
    assert_ne!(f.service("poll", Fixture::service_control(&submit)).0, 200);
}

#[test]
fn local_art_service_http_device_disable_preserves_an_unclaimed_shared_runtime() {
    let f = Fixture::with_service();
    assert_eq!(f.approve(0, true).0, 200);
    let submit = f.service_submit("call:unclaimed", json!({}), 2000);
    let pids = f.runtime.process_ids();
    assert_eq!(
        pids.len(),
        1,
        "persistent plugin is already warm without submit"
    );
    let (status, body) = admin(
        f.port,
        "PUT",
        &format!("/v1/devices/{}", f.device),
        Some(
            json!({"name":"Local Art client","kind":"computer","address":"127.0.0.1","enabled":false}),
        ),
    );
    assert_eq!(status, 200, "{body}");
    assert_eq!(f.service("submit", submit).0, 401);
    assert_eq!(f.runtime.process_ids(), pids);
    // The Device owns its grant, not the shared plugin's idle process.
    let output = f
        .runtime
        .invoke(loom_capability_runtime::CapabilityInvocation {
            request_id: "independent-host-call".into(),
            command_id: "publisher.example/local-service.run".into(),
            input: json!({}),
            target: None,
            resource_refs: vec![],
            unit_attachments: vec![],
            staged_resources: vec![],
            user_gesture_token: None,
            timeout: Some(Duration::from_secs(2)),
        })
        .unwrap();
    assert_eq!(output.payload.unwrap()["output"]["calls"], 1);
    assert_eq!(f.runtime.process_ids(), pids);
}
