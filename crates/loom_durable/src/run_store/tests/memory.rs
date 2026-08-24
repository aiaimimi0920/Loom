use serde_json::json;

use crate::run_store::{
    core::{MAX_EVENT_BATCH, MAX_RUN_JSON_BYTES, MAX_RUN_STORE_JSON_DEPTH},
    InMemoryRunEvidenceStore, RunEventDraft, RunEvidenceStore, RunStoreError,
};

use super::support::{exercise_store, sample_run, started_event};

#[test]
fn in_memory_store_satisfies_run_evidence_contract() {
    exercise_store(&mut InMemoryRunEvidenceStore::default());
}

#[test]
fn store_rejects_invalid_run_and_event_shapes() {
    let mut store = InMemoryRunEvidenceStore::default();
    assert!(matches!(
        store.insert_run(json!({"status":"running"}), vec![]),
        Err(RunStoreError::InvalidRun(_))
    ));
    assert!(matches!(
        store.insert_run(json!({"id":"run-1","status":"  "}), vec![]),
        Err(RunStoreError::InvalidRun(_))
    ));
    assert!(RunEventDraft::new("run_started", json!([])).is_err());
    assert!(RunEventDraft::new("  ", json!({})).is_err());
}

#[test]
fn store_rejects_run_and_event_documents_outside_the_json_budget() {
    let mut store = InMemoryRunEvidenceStore::default();
    let oversized_run = json!({
        "id": "oversized",
        "status": "running",
        "payload": "x".repeat(MAX_RUN_JSON_BYTES)
    });
    assert!(matches!(
        store.insert_run(oversized_run, vec![]),
        Err(RunStoreError::InvalidRun(_))
    ));
    assert_eq!(store.get_run("oversized").expect("read"), None);

    let mut nested = json!(true);
    for _ in 0..=MAX_RUN_STORE_JSON_DEPTH {
        nested = json!({ "nested": nested });
    }
    let event = RunEventDraft::new("run_started", json!({ "payload": nested }))
        .expect("event object shape");
    assert!(matches!(
        store.insert_run(sample_run("too-deep", "running"), vec![event]),
        Err(RunStoreError::InvalidEvent(_))
    ));
    assert_eq!(store.get_run("too-deep").expect("read"), None);

    let oversized_batch = vec![started_event(); MAX_EVENT_BATCH + 1];
    assert!(matches!(
        store.insert_run(sample_run("too-many-events", "running"), oversized_batch),
        Err(RunStoreError::InvalidEvent(_))
    ));
    assert_eq!(store.get_run("too-many-events").expect("read"), None);
}

#[test]
fn in_memory_recovery_terminalizes_running_runs() {
    let mut store = InMemoryRunEvidenceStore::default();
    store
        .insert_run(sample_run("run-b", "running"), vec![started_event()])
        .expect("insert running run");
    store
        .insert_run(sample_run("run-a", "succeeded"), vec![])
        .expect("insert completed run");

    assert_eq!(store.recover_interrupted_runs().expect("recover"), 1);
    let recovered = store.get_run("run-b").expect("read").unwrap();
    assert_eq!(recovered["status"], "failed");
    assert_eq!(recovered["error"]["code"], "daemon_restarted");
    assert_eq!(
        recovered["error"]["message"],
        "Run was interrupted by a daemon restart"
    );
    let events = store.get_events("run-b").expect("events").unwrap();
    assert_eq!(events.last().unwrap()["kind"], "run_interrupted");
    assert_eq!(events.last().unwrap()["status"], "failed");
    assert_eq!(events.last().unwrap()["error"]["code"], "daemon_restarted");
}

#[test]
fn in_memory_recovery_orders_interruption_events_by_run_id() {
    let mut store = InMemoryRunEvidenceStore::default();
    store
        .insert_run(sample_run("run-b", "running"), vec![])
        .expect("insert run b");
    store
        .insert_run(sample_run("run-a", "running"), vec![])
        .expect("insert run a");

    assert_eq!(store.recover_interrupted_runs().expect("recover"), 2);
    assert_eq!(
        store.get_events("run-a").expect("events a").unwrap()[0]["sequence"],
        1
    );
    assert_eq!(
        store.get_events("run-b").expect("events b").unwrap()[0]["sequence"],
        2
    );
}

#[test]
fn invalid_event_batch_leaves_no_partial_run() {
    let mut store = InMemoryRunEvidenceStore::default();
    let result = store.insert_run(
        sample_run("partial", "running"),
        vec![
            started_event(),
            RunEventDraft {
                kind: String::new(),
                fields: serde_json::Map::new(),
            },
        ],
    );
    assert!(result.is_err());
    assert_eq!(store.get_run("partial").expect("read"), None);

    store
        .insert_run(
            sample_run("after-invalid", "running"),
            vec![started_event()],
        )
        .expect("insert after invalid batch");
    assert_eq!(
        store.get_events("after-invalid").expect("events").unwrap()[0]["sequence"],
        1
    );
}

#[test]
fn event_metadata_is_owned_by_the_store() {
    let mut store = InMemoryRunEvidenceStore::default();
    let draft = RunEventDraft::new(
        "run_started",
        json!({
            "sequence": 999,
            "run_id": "forged",
            "kind": "forged",
            "status": "running"
        }),
    )
    .expect("valid event draft");

    store
        .insert_run(sample_run("run-1", "running"), vec![draft])
        .expect("insert run");
    let events = store.get_events("run-1").expect("events").unwrap();
    let event = &events[0];
    assert_eq!(event["sequence"], 1);
    assert_eq!(event["run_id"], "run-1");
    assert_eq!(event["kind"], "run_started");
    assert_eq!(event["status"], "running");
}

#[test]
fn invalid_transition_does_not_mutate_the_canonical_run_or_events() {
    let mut store = InMemoryRunEvidenceStore::default();
    store
        .insert_run(sample_run("run-1", "running"), vec![started_event()])
        .expect("insert run");

    assert!(matches!(
        store.transition_run(
            sample_run("run-1", "succeeded"),
            RunEventDraft {
                kind: String::new(),
                fields: serde_json::Map::new(),
            },
        ),
        Err(RunStoreError::InvalidEvent(_))
    ));
    assert_eq!(
        store.get_run("run-1").expect("read").unwrap()["status"],
        "running"
    );
    assert_eq!(store.get_events("run-1").expect("events").unwrap().len(), 1);
}

#[test]
fn duplicate_insert_and_missing_transition_are_rejected() {
    let mut store = InMemoryRunEvidenceStore::default();
    store
        .insert_run(sample_run("run-1", "running"), vec![started_event()])
        .expect("insert run");

    assert!(matches!(
        store.insert_run(sample_run("run-1", "running"), vec![started_event()]),
        Err(RunStoreError::DuplicateRun(id)) if id == "run-1"
    ));
    assert_eq!(store.get_events("run-1").expect("events").unwrap().len(), 1);

    assert!(matches!(
        store.transition_run(
            sample_run("missing", "failed"),
            RunEventDraft::new("run_action", json!({"status":"failed"}))
                .expect("valid event"),
        ),
        Err(RunStoreError::RunNotFound(id)) if id == "missing"
    ));
}

#[test]
fn status_serializes_with_the_public_shape() {
    let store = InMemoryRunEvidenceStore::default();
    assert_eq!(
        serde_json::to_value(store.status()).expect("serialize status"),
        json!({"mode":"memory","persistent":false})
    );
}
