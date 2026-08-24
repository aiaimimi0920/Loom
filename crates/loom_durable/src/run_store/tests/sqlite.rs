use serde_json::json;

use crate::run_store::{
    core::{MAX_EVENT_FIELDS_JSON_BYTES, MAX_RUN_JSON_BYTES},
    RunEventDraft, RunEvidenceStore, RunStoreError, RunStoreStatus, SqliteRunEvidenceStore,
    RUN_STORE_SCHEMA_VERSION,
};

use super::support::{
    exercise_store, remove_sqlite_files, sample_run, started_event, unique_sqlite_path,
};

#[test]
fn sqlite_store_survives_reopen_and_continues_sequence() {
    let path = unique_sqlite_path("reopen");
    {
        let mut store = SqliteRunEvidenceStore::open(&path).expect("open store");
        store
            .insert_run(sample_run("run-1", "running"), vec![started_event()])
            .expect("insert run");
    }
    {
        let mut store = SqliteRunEvidenceStore::open(&path).expect("reopen store");
        assert_eq!(
            store.status(),
            RunStoreStatus {
                mode: "sqlite",
                persistent: true
            }
        );
        let mut run = store.get_run("run-1").expect("read").unwrap();
        assert_eq!(run["status"], "failed");
        assert_eq!(run["error"]["code"], "daemon_restarted");
        assert_eq!(run["input"]["goal"], "persist evidence");
        run["status"] = json!("retrying");
        store
            .transition_run(
                run,
                RunEventDraft::new("run_action", json!({"action":"retrying"})).expect("draft"),
            )
            .expect("transition");
        let events = store.get_events("run-1").expect("events").unwrap();
        assert_eq!(
            events
                .iter()
                .map(|event| event["sequence"].as_u64().unwrap())
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
    }
    remove_sqlite_files(&path);
}

#[test]
fn sqlite_store_satisfies_run_evidence_contract() {
    let path = unique_sqlite_path("contract");
    {
        let mut store = SqliteRunEvidenceStore::open(&path).expect("open store");
        exercise_store(&mut store);
    }
    remove_sqlite_files(&path);
}

#[test]
fn sqlite_invalid_event_batch_leaves_no_partial_run() {
    let path = unique_sqlite_path("invalid-batch");
    {
        let mut store = SqliteRunEvidenceStore::open(&path).expect("open store");
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
        assert!(matches!(result, Err(RunStoreError::InvalidEvent(_))));
        assert_eq!(store.get_run("partial").expect("read"), None);
        assert_eq!(store.get_events("partial").expect("events"), None);
    }
    remove_sqlite_files(&path);
}

#[test]
fn sqlite_store_applies_required_pragmas_and_schema() {
    let path = unique_sqlite_path("pragmas");
    {
        let store = SqliteRunEvidenceStore::open(&path).expect("open store");
        let foreign_keys: i64 = store
            .connection
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))
            .expect("foreign keys");
        let journal_mode: String = store
            .connection
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .expect("journal mode");
        let synchronous: i64 = store
            .connection
            .pragma_query_value(None, "synchronous", |row| row.get(0))
            .expect("synchronous");
        let busy_timeout: i64 = store
            .connection
            .pragma_query_value(None, "busy_timeout", |row| row.get(0))
            .expect("busy timeout");
        let user_version: i32 = store
            .connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("user version");

        assert_eq!(foreign_keys, 1);
        assert_eq!(journal_mode, "wal");
        assert_eq!(synchronous, 2);
        assert_eq!(busy_timeout, 5_000);
        assert_eq!(user_version, RUN_STORE_SCHEMA_VERSION);
    }
    remove_sqlite_files(&path);
}

#[test]
fn sqlite_store_rejects_newer_schema() {
    let path = unique_sqlite_path("newer-schema");
    let connection = rusqlite::Connection::open(&path).expect("open fixture");
    connection
        .pragma_update(None, "user_version", 2)
        .expect("set schema");
    drop(connection);
    assert!(matches!(
        SqliteRunEvidenceStore::open(&path),
        Err(RunStoreError::UnsupportedSchema {
            found: 2,
            supported: 1
        })
    ));
    remove_sqlite_files(&path);
}

#[test]
fn sqlite_duplicate_insert_leaves_existing_events_unchanged() {
    let path = unique_sqlite_path("duplicate");
    {
        let mut store = SqliteRunEvidenceStore::open(&path).expect("open store");
        store
            .insert_run(sample_run("run-1", "running"), vec![started_event()])
            .expect("first insert");
        assert!(matches!(
            store.insert_run(sample_run("run-1", "running"), vec![started_event()]),
            Err(RunStoreError::DuplicateRun(id)) if id == "run-1"
        ));
        assert_eq!(store.get_events("run-1").expect("events").unwrap().len(), 1);
    }
    remove_sqlite_files(&path);
}

#[test]
fn sqlite_store_rejects_malformed_run_json_on_reopen() {
    let path = unique_sqlite_path("malformed-run-json");
    {
        let mut store = SqliteRunEvidenceStore::open(&path).expect("open store");
        store
            .insert_run(sample_run("run-1", "succeeded"), vec![])
            .expect("insert run");
    }
    {
        let connection = rusqlite::Connection::open(&path).expect("open fixture");
        connection
            .execute(
                "UPDATE runs SET run_json = ?1 WHERE run_id = ?2",
                rusqlite::params!["not-json", "run-1"],
            )
            .expect("corrupt run JSON");
    }
    assert!(matches!(
        SqliteRunEvidenceStore::open(&path),
        Err(RunStoreError::Integrity(_))
    ));
    remove_sqlite_files(&path);
}

#[test]
fn sqlite_store_rejects_array_event_fields_on_reopen() {
    let path = unique_sqlite_path("array-fields");
    {
        let mut store = SqliteRunEvidenceStore::open(&path).expect("open store");
        store
            .insert_run(sample_run("run-1", "succeeded"), vec![started_event()])
            .expect("insert run");
    }
    {
        let connection = rusqlite::Connection::open(&path).expect("open fixture");
        connection
            .execute(
                "UPDATE run_events SET fields_json = ?1 WHERE run_id = ?2",
                rusqlite::params!["[]", "run-1"],
            )
            .expect("corrupt event fields");
    }
    assert!(matches!(
        SqliteRunEvidenceStore::open(&path),
        Err(RunStoreError::Integrity(_))
    ));
    remove_sqlite_files(&path);
}

#[test]
fn sqlite_store_rejects_oversized_persisted_json_on_reopen() {
    let run_path = unique_sqlite_path("oversized-run-json");
    {
        let mut store = SqliteRunEvidenceStore::open(&run_path).expect("open store");
        store
            .insert_run(sample_run("run-1", "succeeded"), vec![])
            .expect("insert run");
    }
    {
        let connection = rusqlite::Connection::open(&run_path).expect("open fixture");
        let oversized = format!(
            r#"{{"id":"run-1","status":"succeeded","payload":"{}"}}"#,
            "x".repeat(MAX_RUN_JSON_BYTES)
        );
        connection
            .execute(
                "UPDATE runs SET run_json = ?1 WHERE run_id = ?2",
                rusqlite::params![oversized, "run-1"],
            )
            .expect("oversize run JSON");
    }
    assert!(matches!(
        SqliteRunEvidenceStore::open(&run_path),
        Err(RunStoreError::Integrity(_))
    ));
    remove_sqlite_files(&run_path);

    let event_path = unique_sqlite_path("oversized-event-json");
    {
        let mut store = SqliteRunEvidenceStore::open(&event_path).expect("open store");
        store
            .insert_run(sample_run("run-1", "succeeded"), vec![started_event()])
            .expect("insert run");
    }
    {
        let connection = rusqlite::Connection::open(&event_path).expect("open fixture");
        let oversized = format!(
            r#"{{"payload":"{}"}}"#,
            "x".repeat(MAX_EVENT_FIELDS_JSON_BYTES)
        );
        connection
            .execute(
                "UPDATE run_events SET fields_json = ?1 WHERE run_id = ?2",
                rusqlite::params![oversized, "run-1"],
            )
            .expect("oversize event JSON");
    }
    assert!(matches!(
        SqliteRunEvidenceStore::open(&event_path),
        Err(RunStoreError::Integrity(_))
    ));
    remove_sqlite_files(&event_path);
}

#[test]
fn sqlite_immediate_transactions_preserve_concurrent_writers() {
    use std::sync::{Arc, Barrier};

    const WRITERS: usize = 8;
    let path = unique_sqlite_path("concurrent-writers");
    drop(SqliteRunEvidenceStore::open(&path).expect("initialize store"));
    let barrier = Arc::new(Barrier::new(WRITERS));
    let mut writers = Vec::new();
    for index in 0..WRITERS {
        let path = path.clone();
        let barrier = Arc::clone(&barrier);
        writers.push(std::thread::spawn(move || {
            let mut store = SqliteRunEvidenceStore::open(&path).expect("open writer store");
            barrier.wait();
            store
                .insert_run(sample_run(&format!("run-{index:02}"), "succeeded"), vec![])
                .expect("insert concurrent run");
        }));
    }
    for writer in writers {
        writer.join().expect("writer thread");
    }
    let store = SqliteRunEvidenceStore::open(&path).expect("reopen store");
    for index in 0..WRITERS {
        assert!(store
            .get_run(&format!("run-{index:02}"))
            .expect("read run")
            .is_some());
    }
    remove_sqlite_files(&path);
}

#[cfg(unix)]
#[test]
fn sqlite_store_rejects_symlinked_database_and_uses_private_mode() {
    use std::os::unix::fs::{symlink, PermissionsExt as _};

    let outside = unique_sqlite_path("outside-target");
    drop(rusqlite::Connection::open(&outside).expect("create outside database"));
    let linked = unique_sqlite_path("linked-database");
    symlink(&outside, &linked).expect("create database symlink");
    assert!(SqliteRunEvidenceStore::open(&linked).is_err());

    let private = unique_sqlite_path("private-mode");
    drop(SqliteRunEvidenceStore::open(&private).expect("create private database"));
    assert_eq!(
        std::fs::metadata(&private)
            .expect("database metadata")
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    let _ = std::fs::remove_file(&linked);
    remove_sqlite_files(&outside);
    remove_sqlite_files(&private);
}

#[cfg(windows)]
#[test]
fn sqlite_store_rejects_symlinked_database_when_windows_allows_fixture() {
    use std::io::ErrorKind;
    use std::os::windows::fs::symlink_file;

    let outside = unique_sqlite_path("outside-target-windows");
    drop(rusqlite::Connection::open(&outside).expect("create outside database"));
    let linked = unique_sqlite_path("linked-database-windows");
    match symlink_file(&outside, &linked) {
        Ok(()) => assert!(SqliteRunEvidenceStore::open(&linked).is_err()),
        Err(error)
            if error.kind() == ErrorKind::PermissionDenied
                || error.raw_os_error() == Some(1314) => {}
        Err(error) => panic!("create database symlink: {error}"),
    }
    let _ = std::fs::remove_file(&linked);
    remove_sqlite_files(&outside);
}
