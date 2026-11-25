use super::{model::*, Store};

fn report(generation: Option<&str>, revision: u64, language: &str) -> Report {
    Report {
        receiver_id: "process-one".into(),
        receiver_generation: generation.map(str::to_owned),
        language: language.into(),
        revision,
        supported_languages: vec!["en".into(), "zh-Hans".into()],
        result: None,
    }
}

fn request(generation: &str, id: &str, revision: u64) -> RequestInput {
    RequestInput {
        request_id: id.into(),
        receiver_generation: generation.into(),
        expected_revision: revision,
        language: "en".into(),
    }
}

fn claim(store: &mut Store, now: u64) -> String {
    store
        .report("device", 1, u64::MAX, report(None, 7, "zh-Hans"), now)
        .unwrap()
        .receiver_generation
}

#[test]
fn locale_requests_are_online_scoped_idempotent_and_never_replayed() {
    let mut store = Store::default();
    assert_eq!(
        store
            .request("device", 1, request("none", "r", 7), 1)
            .err()
            .unwrap()
            .1,
        "locale_receiver_unavailable"
    );
    let generation = claim(&mut store, 1);
    let input = request(&generation, "request-one", 7);
    assert!(store.request("device", 1, input.clone(), 2).unwrap().0);
    assert!(!store.request("device", 1, input.clone(), 3).unwrap().0);
    let mut different = input.clone();
    different.language = "zh-Hans".into();
    assert_eq!(
        store.request("device", 1, different, 4).err().unwrap().1,
        "locale_request_id_conflict"
    );
    assert_eq!(
        store
            .request("device", 1, request(&generation, "two", 7), 4)
            .err()
            .unwrap()
            .1,
        "locale_request_in_flight"
    );
    let mut applied = report(Some(&generation), 8, "en");
    applied.result = Some(Outcome {
        request_id: "request-one".into(),
        status: OutcomeStatus::Applied,
        applied_revision: Some(8),
        code: None,
    });
    store.report("device", 1, u64::MAX, applied, 5).unwrap();
    let (_, retried) = store.request("device", 1, input, 6).unwrap();
    assert_eq!(retried.request.unwrap().status, "applied");
    assert_eq!(
        store
            .request("device", 1, request(&generation, "two", 7), 6)
            .err()
            .unwrap()
            .1,
        "locale_revision_conflict"
    );
}

#[test]
fn locale_receiver_takeover_and_epoch_changes_never_transfer_pending_commands() {
    let mut store = Store::default();
    let generation = claim(&mut store, 1);
    store
        .request("device", 1, request(&generation, "one", 7), 2)
        .unwrap();
    let mut competing = report(None, 7, "en");
    competing.receiver_id = "process-two".into();
    assert_eq!(
        store
            .report("device", 1, u64::MAX, competing, 3)
            .err()
            .unwrap()
            .1,
        "locale_receiver_busy"
    );
    assert_eq!(
        store
            .report(
                "device",
                1,
                u64::MAX,
                report(Some(&generation), 7, "zh-Hans"),
                15_001
            )
            .err()
            .unwrap()
            .1,
        "locale_receiver_expired"
    );
    let next = claim(&mut store, 15_001);
    assert_ne!(generation, next);
    assert!(store.view("device", 1, 15_001).unwrap().request.is_none());
    assert_eq!(
        store
            .request("device", 1, request(&generation, "one", 7), 15_002)
            .err()
            .unwrap()
            .1,
        "locale_receiver_changed"
    );
    assert!(store.view("device", 2, 15_002).is_none());
    store.revoke("device");
    assert!(store.view("device", 1, 15_002).is_none());
}

#[test]
fn locale_timeout_is_unknown_and_late_receipt_preserves_newer_local_setting() {
    let mut store = Store::default();
    let generation = claim(&mut store, 1);
    store
        .request("device", 1, request(&generation, "one", 7), 2)
        .unwrap();
    store
        .report(
            "device",
            1,
            u64::MAX,
            report(Some(&generation), 7, "zh-Hans"),
            10_000,
        )
        .unwrap();
    let pending = store.view("device", 1, 15_002).unwrap().request.unwrap();
    assert_eq!(pending.status, "unknown");
    assert_eq!(pending.valid_for_ms, 0);
    // The remote write committed revision 8; a subsequent tray edit committed 9.
    let mut applied = report(Some(&generation), 9, "zh-Hans");
    applied.result = Some(Outcome {
        request_id: "one".into(),
        status: OutcomeStatus::Applied,
        applied_revision: Some(8),
        code: None,
    });
    let view = store
        .report("device", 1, u64::MAX, applied, 15_003)
        .unwrap();
    assert_eq!(view.language, "zh-Hans");
    assert_eq!(view.revision, 9);
    assert_eq!(view.request.unwrap().status, "applied");
    assert_eq!(
        store
            .report(
                "device",
                1,
                u64::MAX,
                report(Some(&generation), 8, "en"),
                15_004
            )
            .err()
            .unwrap()
            .1,
        "locale_report_conflict"
    );
}

#[test]
fn locale_receipts_and_receivers_are_bounded_without_evicting_live_work() {
    let mut store = Store::default();
    let generation = claim(&mut store, 1);
    for index in 0..32 {
        let id = format!("request-{index}");
        store
            .request("device", 1, request(&generation, &id, 7), 2)
            .unwrap();
        let mut rejected = report(Some(&generation), 7, "zh-Hans");
        rejected.result = Some(Outcome {
            request_id: id,
            status: OutcomeStatus::Rejected,
            applied_revision: None,
            code: Some("save_conflict".into()),
        });
        store.report("device", 1, u64::MAX, rejected, 3).unwrap();
    }
    assert_eq!(
        store
            .request("device", 1, request(&generation, "overflow", 7), 4)
            .err()
            .unwrap()
            .1,
        "locale_capacity"
    );
    for index in 1..256 {
        store
            .report(
                &format!("device-{index}"),
                1,
                u64::MAX,
                report(None, 0, "en"),
                1,
            )
            .unwrap();
    }
    assert_eq!(
        store
            .report("overflow", 1, u64::MAX, report(None, 0, "en"), 2)
            .err()
            .unwrap()
            .1,
        "locale_capacity"
    );
    assert!(store
        .report(
            "overflow",
            1,
            u64::MAX,
            report(None, 0, "en"),
            RETAIN_MS + 10
        )
        .is_ok());
}

#[test]
fn locale_reports_validate_protocol_bounds_and_confirmed_applied_revision() {
    let mut invalid = report(None, MAX_REVISION + 1, "en");
    assert!(invalid.validate().is_err());
    invalid.revision = 0;
    invalid.supported_languages.push("en".into());
    assert!(invalid.validate().is_err());
    assert!(serde_json::from_str::<Report>(r#"{"receiverId":"a","language":"en","revision":0,"supportedLanguages":["en"],"deviceId":"forged"}"#).is_err());
    let mut store = Store::default();
    let generation = claim(&mut store, 1);
    store
        .request("device", 1, request(&generation, "one", 7), 2)
        .unwrap();
    let mut wrong = report(Some(&generation), 9, "en");
    wrong.result = Some(Outcome {
        request_id: "one".into(),
        status: OutcomeStatus::Applied,
        applied_revision: Some(9),
        code: None,
    });
    assert_eq!(
        store
            .report("device", 1, u64::MAX, wrong, 3)
            .err()
            .unwrap()
            .1,
        "locale_result_conflict"
    );
    assert_eq!(store.view("device", 1, 3).unwrap().revision, 7);
}

#[test]
fn locale_session_expiry_cannot_keep_receiver_active() {
    let mut store = Store::default();
    let generation = store
        .report("device", 1, 10, report(None, 7, "en"), 1)
        .unwrap()
        .receiver_generation;
    assert!(!store.view("device", 1, 10).unwrap().active);
    assert_eq!(
        store
            .request("device", 1, request(&generation, "one", 7), 10)
            .err()
            .unwrap()
            .1,
        "locale_receiver_unavailable"
    );
}
