fn locale_report_payload() -> Value {
    json!({
        "receiverId": "locale-process-one",
        "language": "zh-Hans",
        "revision": 0,
        "supportedLanguages": ["en", "zh-Hans"],
    })
}

#[test]
fn otter_locale_http_requires_device_report_and_admin_exact_target() {
    let f = Fixture::new();
    let (id, token) = wall_http::pair(f.port, "Locale owner");
    let (_, other_token) = wall_http::pair(f.port, "Locale other");
    let report_path = "/v1/otter/locale-report";
    let body = locale_report_payload();
    assert_eq!(
        wall_http::public(f.port, "POST", report_path, Some(body.clone())).0,
        401
    );
    assert_eq!(
        wall_http::admin(f.port, "POST", report_path, Some(body.clone())).0,
        403
    );
    let mut forged = body.clone();
    forged["deviceId"] = json!(id);
    assert_eq!(
        wall_http::device(f.port, &other_token, "POST", report_path, Some(forged)).0,
        400
    );
    let (status, report) = wall_http::device(f.port, &token, "POST", report_path, Some(body));
    assert_eq!(status, 200, "{report}");
    assert_eq!(report["deviceId"], id);
    assert!(report["loomId"].as_str().unwrap().starts_with("loom-"));
    let generation = report["locale"]["receiverGeneration"].clone();
    let path = format!("/v1/devices/{id}/locale-requests");
    let input = json!({"requestId":"locale-request-one", "receiverGeneration":generation,
        "expectedRevision":0, "language":"en"});
    assert_eq!(
        wall_http::device(f.port, &token, "POST", &path, Some(input.clone())).0,
        403
    );
    let (status, accepted) = wall_http::admin(f.port, "POST", &path, Some(input.clone()));
    assert_eq!(status, 202, "{accepted}");
    assert_eq!(accepted["locale"]["request"]["status"], "pending");
    assert_eq!(wall_http::admin(f.port, "POST", &path, Some(input)).0, 200);
    let (_, other) = wall_http::device(
        f.port,
        &other_token,
        "GET",
        "/v1/otter/connection-info",
        None,
    );
    assert!(other["localeRequest"].is_null());
    let (_, own) = wall_http::device(f.port, &token, "GET", "/v1/otter/connection-info", None);
    assert_eq!(own["localeRequest"]["requestId"], "locale-request-one");
    assert_eq!(own["localeRequest"].as_object().unwrap().len(), 6);
    assert!(own["localeRequest"]["validForMs"].as_u64().unwrap() <= 15_000);
}

#[test]
fn otter_locale_http_applied_receipt_reads_back_actual_state_and_is_idempotent() {
    let f = Fixture::new();
    let (id, token) = wall_http::pair(f.port, "Locale receiver");
    let report_path = "/v1/otter/locale-report";
    let (_, claim) = wall_http::device(
        f.port,
        &token,
        "POST",
        report_path,
        Some(locale_report_payload()),
    );
    let generation = claim["locale"]["receiverGeneration"].clone();
    let path = format!("/v1/devices/{id}/locale-requests");
    let input = json!({"requestId":"locale-request-one", "receiverGeneration":generation,
        "expectedRevision":0, "language":"en"});
    assert_eq!(
        wall_http::admin(f.port, "POST", &path, Some(input.clone())).0,
        202
    );
    let mut report = locale_report_payload();
    report["receiverGeneration"] = generation;
    let (_, pending) = wall_http::device(f.port, &token, "POST", report_path, Some(report.clone()));
    assert_eq!(
        pending["locale"]["request"]["requestId"],
        "locale-request-one"
    );
    assert_eq!(pending["locale"]["request"].as_object().unwrap().len(), 6);
    report["revision"] = json!(1);
    report["language"] = json!("en");
    report["result"] = json!({"requestId":"locale-request-one", "status":"applied",
        "code":"applied", "appliedRevision":1});
    for _ in 0..2 {
        let (status, acknowledged) =
            wall_http::device(f.port, &token, "POST", report_path, Some(report.clone()));
        assert_eq!(status, 200, "{acknowledged}");
        assert!(acknowledged["locale"]["request"].is_null());
    }
    let (status, retried) = wall_http::admin(f.port, "POST", &path, Some(input));
    assert_eq!(status, 200, "{retried}");
    assert_eq!(retried["locale"]["request"]["status"], "applied");
    let (_, devices) = wall_http::admin(f.port, "GET", "/v1/devices", None);
    let device = devices["devices"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["id"] == id)
        .unwrap();
    assert_eq!(device["locale"]["language"], "en");
    assert_eq!(device["locale"]["revision"], 1);
    assert_eq!(device["locale"]["request"]["appliedRevision"], 1);
}

#[test]
fn otter_locale_http_revocation_clears_receiver_and_does_not_queue_offline_work() {
    let f = Fixture::new();
    let (id, token) = wall_http::pair(f.port, "Locale revoke");
    let report_path = "/v1/otter/locale-report";
    let (_, claim) = wall_http::device(
        f.port,
        &token,
        "POST",
        report_path,
        Some(locale_report_payload()),
    );
    let path = format!("/v1/devices/{id}/locale-requests");
    let input = json!({"requestId":"locale-request-one",
        "receiverGeneration":claim["locale"]["receiverGeneration"],
        "expectedRevision":0, "language":"en"});
    f.devices.lock().unwrap().revoke_device_sessions(&id);
    assert_eq!(
        wall_http::device(
            f.port,
            &token,
            "POST",
            report_path,
            Some(locale_report_payload())
        )
        .0,
        401
    );
    let (status, error) = wall_http::admin(f.port, "POST", &path, Some(input));
    assert_eq!(status, 409, "{error}");
    assert_eq!(error["error"]["code"], "locale_receiver_unavailable");
    let (_, devices) = wall_http::admin(f.port, "GET", "/v1/devices", None);
    let device = devices["devices"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["id"] == id)
        .unwrap();
    assert!(device["locale"].is_null());
}
