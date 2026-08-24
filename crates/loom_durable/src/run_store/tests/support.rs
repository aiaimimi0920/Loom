use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::json;

use crate::run_store::{RunEventDraft, RunEvidenceStore};

pub(super) fn sample_run(id: &str, status: &str) -> serde_json::Value {
    json!({
        "id": id,
        "capability": "brain.plan",
        "loom_session_id": "session-test",
        "status": status,
        "input": { "goal": "persist evidence" }
    })
}

pub(super) fn started_event() -> RunEventDraft {
    RunEventDraft::new(
        "run_started",
        json!({
            "capability": "brain.plan",
            "status": "running"
        }),
    )
    .expect("valid event draft")
}

pub(super) fn unique_sqlite_path(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after Unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "loom-run-store-{label}-{}-{nonce}.sqlite",
        std::process::id()
    ))
}

pub(super) fn remove_sqlite_files(path: &Path) {
    for suffix in ["", "-wal", "-shm"] {
        let target = PathBuf::from(format!("{}{}", path.display(), suffix));
        let _ = std::fs::remove_file(target);
    }
}

pub(super) fn exercise_store(store: &mut dyn RunEvidenceStore) {
    store
        .insert_run(sample_run("run-1", "running"), vec![started_event()])
        .expect("insert run");

    let run = store
        .get_run("run-1")
        .expect("read run")
        .expect("stored run");
    assert_eq!(run["status"], "running");

    let mut completed = run;
    completed["status"] = json!("succeeded");
    completed["output"] = json!({ "summary": "stored" });
    store
        .transition_run(
            completed.clone(),
            RunEventDraft::new("capability_completed", json!({ "status": "succeeded" }))
                .expect("valid completion event"),
        )
        .expect("transition run");

    assert_eq!(store.get_run("run-1").expect("read").unwrap(), completed);
    let events = store
        .get_events("run-1")
        .expect("read events")
        .expect("stored events");
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["sequence"], 1);
    assert_eq!(events[0]["kind"], "run_started");
    assert_eq!(events[1]["sequence"], 2);
    assert_eq!(events[1]["kind"], "capability_completed");
    assert_eq!(store.get_run("missing").expect("read missing"), None);
    assert_eq!(store.get_events("missing").expect("read missing"), None);
}
