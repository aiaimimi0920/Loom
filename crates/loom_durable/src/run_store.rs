use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
    time::Duration,
};

use chrono::Utc;
use rusqlite::{params, Connection, ErrorCode, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use thiserror::Error;

pub type RunStoreResult<T> = Result<T, RunStoreError>;

#[derive(Debug, Error)]
pub enum RunStoreError {
    #[error("invalid run evidence: {0}")]
    InvalidRun(String),
    #[error("invalid run event: {0}")]
    InvalidEvent(String),
    #[error("run `{0}` already exists")]
    DuplicateRun(String),
    #[error("run `{0}` was not found")]
    RunNotFound(String),
    #[error("run store JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("run store IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("run store schema {found} is newer than supported schema {supported}")]
    UnsupportedSchema { found: i32, supported: i32 },
    #[error("run store integrity check failed: {0}")]
    Integrity(String),
    #[error("SQLite run store error: {0}")]
    Sqlite(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct RunEventDraft {
    pub kind: String,
    pub fields: Map<String, Value>,
}

impl RunEventDraft {
    pub fn new(kind: impl Into<String>, fields: Value) -> RunStoreResult<Self> {
        let kind = kind.into();
        if kind.trim().is_empty() {
            return Err(RunStoreError::InvalidEvent("kind is required".to_owned()));
        }
        let fields = fields
            .as_object()
            .cloned()
            .ok_or_else(|| RunStoreError::InvalidEvent("fields must be an object".to_owned()))?;
        Ok(Self { kind, fields })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunStoreStatus {
    pub mode: &'static str,
    pub persistent: bool,
}

pub trait RunEvidenceStore: Send {
    fn insert_run(&mut self, run: Value, events: Vec<RunEventDraft>) -> RunStoreResult<()>;
    fn transition_run(&mut self, run: Value, event: RunEventDraft) -> RunStoreResult<()>;
    fn get_run(&self, run_id: &str) -> RunStoreResult<Option<Value>>;
    fn get_events(&self, run_id: &str) -> RunStoreResult<Option<Vec<Value>>>;
    fn recover_interrupted_runs(&mut self) -> RunStoreResult<usize>;
    fn status(&self) -> RunStoreStatus;
}

fn validated_run_identity(run: &Value) -> RunStoreResult<(&str, &str)> {
    let object = run
        .as_object()
        .ok_or_else(|| RunStoreError::InvalidRun("run must be an object".to_owned()))?;
    let run_id = object
        .get("id")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| RunStoreError::InvalidRun("id must be a non-empty string".to_owned()))?;
    let status = object
        .get("status")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| RunStoreError::InvalidRun("status must be a non-empty string".to_owned()))?;
    Ok((run_id, status))
}

fn validate_event_draft(event: &RunEventDraft) -> RunStoreResult<()> {
    if event.kind.trim().is_empty() {
        return Err(RunStoreError::InvalidEvent("kind is required".to_owned()));
    }
    Ok(())
}

fn event_value(sequence: u64, run_id: &str, kind: &str, fields: Map<String, Value>) -> Value {
    let mut event = Value::Object(fields);
    let target = event.as_object_mut().expect("event fields object");
    target.insert("sequence".to_owned(), json!(sequence));
    target.insert("kind".to_owned(), Value::String(kind.to_owned()));
    target.insert("run_id".to_owned(), Value::String(run_id.to_owned()));
    event
}

fn interrupt_run(run: &mut Value) -> RunStoreResult<RunEventDraft> {
    let (_, status) = validated_run_identity(run)?;
    if status != "running" {
        return Err(RunStoreError::InvalidRun(
            "only running runs can be interrupted".to_owned(),
        ));
    }

    let object = run
        .as_object_mut()
        .expect("validated run must remain an object");
    object.insert("status".to_owned(), json!("failed"));
    object.insert(
        "error".to_owned(),
        json!({
            "code": "daemon_restarted",
            "message": "Run was interrupted by a daemon restart"
        }),
    );

    RunEventDraft::new(
        "run_interrupted",
        json!({
            "status": "failed",
            "error": { "code": "daemon_restarted" }
        }),
    )
}

fn sequence_values(start: u64, count: usize) -> RunStoreResult<Vec<u64>> {
    let count = u64::try_from(count)
        .map_err(|_| RunStoreError::Integrity("event batch is too large".to_owned()))?;
    start
        .checked_add(count)
        .ok_or_else(|| RunStoreError::Integrity("event sequence overflow".to_owned()))?;
    Ok((1..=count).map(|offset| start + offset).collect())
}

pub const RUN_STORE_SCHEMA_VERSION: i32 = 1;

const SCHEMA_V1: &str = r#"
CREATE TABLE runs (
    run_id TEXT PRIMARY KEY NOT NULL,
    status TEXT NOT NULL,
    run_json TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);
CREATE TABLE run_events (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    fields_json TEXT NOT NULL,
    FOREIGN KEY (run_id) REFERENCES runs(run_id) ON DELETE CASCADE
);
CREATE INDEX run_events_run_id_sequence
    ON run_events (run_id, sequence);
"#;

fn sqlite_error(error: rusqlite::Error) -> RunStoreError {
    RunStoreError::Sqlite(error.to_string())
}

fn stored_integrity(context: impl Into<String>, error: impl std::fmt::Display) -> RunStoreError {
    RunStoreError::Integrity(format!("{}: {error}", context.into()))
}

fn event_fields_for_storage(mut fields: Map<String, Value>) -> Map<String, Value> {
    fields.remove("sequence");
    fields.remove("kind");
    fields.remove("run_id");
    fields
}

fn validated_stored_run(
    indexed_run_id: &str,
    indexed_status: &str,
    run_json: &str,
) -> RunStoreResult<Value> {
    let run: Value = serde_json::from_str(run_json).map_err(|error| {
        stored_integrity(
            format!("run `{indexed_run_id}` contains invalid JSON"),
            error,
        )
    })?;
    let (run_id, status) = validated_run_identity(&run)
        .map_err(|error| stored_integrity(format!("run `{indexed_run_id}` is invalid"), error))?;
    if run_id != indexed_run_id {
        return Err(RunStoreError::Integrity(format!(
            "run index id `{indexed_run_id}` does not match stored id `{run_id}`"
        )));
    }
    if status != indexed_status {
        return Err(RunStoreError::Integrity(format!(
            "run `{indexed_run_id}` index status `{indexed_status}` does not match stored status `{status}`"
        )));
    }
    Ok(run)
}

fn validated_stored_event_fields(
    sequence: i64,
    run_id: &str,
    kind: &str,
    fields_json: &str,
) -> RunStoreResult<(u64, Map<String, Value>)> {
    let sequence = u64::try_from(sequence).map_err(|_| {
        RunStoreError::Integrity(format!("run event sequence `{sequence}` is not positive"))
    })?;
    if sequence == 0 {
        return Err(RunStoreError::Integrity(
            "run event sequence must be positive".to_owned(),
        ));
    }
    if run_id.trim().is_empty() {
        return Err(RunStoreError::Integrity(format!(
            "run event sequence `{sequence}` has an empty run id"
        )));
    }
    if kind.trim().is_empty() {
        return Err(RunStoreError::Integrity(format!(
            "run event sequence `{sequence}` has an empty kind"
        )));
    }
    let fields: Value = serde_json::from_str(fields_json).map_err(|error| {
        stored_integrity(
            format!("run event sequence `{sequence}` contains invalid fields JSON"),
            error,
        )
    })?;
    let fields = fields.as_object().cloned().ok_or_else(|| {
        RunStoreError::Integrity(format!(
            "run event sequence `{sequence}` fields must be an object"
        ))
    })?;
    if ["sequence", "kind", "run_id"]
        .iter()
        .any(|field| fields.contains_key(*field))
    {
        return Err(RunStoreError::Integrity(format!(
            "run event sequence `{sequence}` fields contain reserved metadata"
        )));
    }
    Ok((sequence, fields))
}

fn insert_sqlite_event(
    transaction: &Transaction<'_>,
    run_id: &str,
    event: RunEventDraft,
) -> RunStoreResult<()> {
    let fields = event_fields_for_storage(event.fields);
    let fields_json = serde_json::to_string(&fields)?;
    transaction
        .execute(
            "INSERT INTO run_events (run_id, kind, fields_json) VALUES (?1, ?2, ?3)",
            params![run_id, event.kind, fields_json],
        )
        .map_err(sqlite_error)?;
    Ok(())
}

#[derive(Debug)]
pub struct SqliteRunEvidenceStore {
    connection: Connection,
}

impl SqliteRunEvidenceStore {
    pub fn open(path: impl AsRef<Path>) -> RunStoreResult<Self> {
        let path = path.as_ref();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }

        let mut connection = Connection::open(path).map_err(sqlite_error)?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(sqlite_error)?;
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .map_err(sqlite_error)?;
        let journal_mode: String = connection
            .pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))
            .map_err(sqlite_error)?;
        if !journal_mode.eq_ignore_ascii_case("wal") {
            return Err(RunStoreError::Integrity(format!(
                "SQLite journal mode is `{journal_mode}` instead of `wal`"
            )));
        }
        connection
            .pragma_update(None, "synchronous", "FULL")
            .map_err(sqlite_error)?;

        let version: i32 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(sqlite_error)?;
        match version {
            0 => {
                let transaction = connection.transaction().map_err(sqlite_error)?;
                transaction.execute_batch(SCHEMA_V1).map_err(sqlite_error)?;
                transaction
                    .pragma_update(None, "user_version", RUN_STORE_SCHEMA_VERSION)
                    .map_err(sqlite_error)?;
                transaction.commit().map_err(sqlite_error)?;
            }
            RUN_STORE_SCHEMA_VERSION => {}
            found if found > RUN_STORE_SCHEMA_VERSION => {
                return Err(RunStoreError::UnsupportedSchema {
                    found,
                    supported: RUN_STORE_SCHEMA_VERSION,
                });
            }
            found => {
                return Err(RunStoreError::Integrity(format!(
                    "unsupported run store schema version `{found}`"
                )));
            }
        }

        Self::quick_check(&connection)?;
        Self::validate_existing_records(&connection)?;

        let mut store = Self { connection };
        store.recover_interrupted_runs()?;
        Ok(store)
    }

    fn quick_check(connection: &Connection) -> RunStoreResult<()> {
        let mut results = Vec::new();
        connection
            .pragma_query(None, "quick_check", |row| {
                results.push(row.get::<_, String>(0)?);
                Ok(())
            })
            .map_err(sqlite_error)?;
        if results.len() != 1 || results[0] != "ok" {
            return Err(RunStoreError::Integrity(format!(
                "PRAGMA quick_check returned {}",
                results.join("; ")
            )));
        }
        Ok(())
    }

    fn validate_existing_records(connection: &Connection) -> RunStoreResult<()> {
        let mut run_ids = HashSet::new();
        {
            let mut statement = connection
                .prepare("SELECT run_id, status, run_json FROM runs ORDER BY run_id")
                .map_err(sqlite_error)?;
            let rows = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .map_err(sqlite_error)?;
            for row in rows {
                let (run_id, status, run_json) = row.map_err(sqlite_error)?;
                validated_stored_run(&run_id, &status, &run_json)?;
                if !run_ids.insert(run_id.clone()) {
                    return Err(RunStoreError::Integrity(format!(
                        "duplicate indexed run id `{run_id}`"
                    )));
                }
            }
        }

        {
            let mut statement = connection
                .prepare(
                    "SELECT sequence, run_id, kind, fields_json FROM run_events ORDER BY sequence",
                )
                .map_err(sqlite_error)?;
            let rows = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                })
                .map_err(sqlite_error)?;
            for row in rows {
                let (sequence, run_id, kind, fields_json) = row.map_err(sqlite_error)?;
                if !run_ids.contains(&run_id) {
                    return Err(RunStoreError::Integrity(format!(
                        "run event sequence `{sequence}` references missing run `{run_id}`"
                    )));
                }
                validated_stored_event_fields(sequence, &run_id, &kind, &fields_json)?;
            }
        }

        let mut foreign_key_violations = 0_u64;
        connection
            .pragma_query(None, "foreign_key_check", |_| {
                foreign_key_violations += 1;
                Ok(())
            })
            .map_err(sqlite_error)?;
        if foreign_key_violations != 0 {
            return Err(RunStoreError::Integrity(format!(
                "SQLite foreign key check reported {foreign_key_violations} violation(s)"
            )));
        }
        Ok(())
    }

    fn read_run_row(&self, run_id: &str) -> RunStoreResult<Option<(String, String)>> {
        self.connection
            .query_row(
                "SELECT status, run_json FROM runs WHERE run_id = ?1",
                params![run_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(sqlite_error)
    }
}

impl RunEvidenceStore for SqliteRunEvidenceStore {
    fn insert_run(&mut self, run: Value, events: Vec<RunEventDraft>) -> RunStoreResult<()> {
        let (run_id, status) = validated_run_identity(&run)?;
        let run_id = run_id.to_owned();
        let status = status.to_owned();
        for event in &events {
            validate_event_draft(event)?;
        }
        let run_json = serde_json::to_string(&run)?;
        let now = Utc::now().timestamp_millis();

        let transaction = self.connection.transaction().map_err(sqlite_error)?;
        match transaction.execute(
            "INSERT INTO runs (run_id, status, run_json, created_at_ms, updated_at_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![run_id, status, run_json, now, now],
        ) {
            Ok(1) => {}
            Ok(changed) => {
                return Err(RunStoreError::Integrity(format!(
                    "inserting run `{run_id}` changed {changed} rows"
                )));
            }
            Err(error) if error.sqlite_error_code() == Some(ErrorCode::ConstraintViolation) => {
                return Err(RunStoreError::DuplicateRun(run_id));
            }
            Err(error) => return Err(sqlite_error(error)),
        }
        for event in events {
            insert_sqlite_event(&transaction, &run_id, event)?;
        }
        transaction.commit().map_err(sqlite_error)
    }

    fn transition_run(&mut self, run: Value, event: RunEventDraft) -> RunStoreResult<()> {
        let (run_id, status) = validated_run_identity(&run)?;
        let run_id = run_id.to_owned();
        let status = status.to_owned();
        validate_event_draft(&event)?;
        let run_json = serde_json::to_string(&run)?;
        let now = Utc::now().timestamp_millis();

        let transaction = self.connection.transaction().map_err(sqlite_error)?;
        let changed = transaction
            .execute(
                "UPDATE runs SET status = ?2, run_json = ?3, updated_at_ms = ?4 WHERE run_id = ?1",
                params![run_id, status, run_json, now],
            )
            .map_err(sqlite_error)?;
        if changed == 0 {
            return Err(RunStoreError::RunNotFound(run_id));
        }
        if changed != 1 {
            return Err(RunStoreError::Integrity(format!(
                "transitioning run `{run_id}` changed {changed} rows"
            )));
        }
        insert_sqlite_event(&transaction, &run_id, event)?;
        transaction.commit().map_err(sqlite_error)
    }

    fn get_run(&self, run_id: &str) -> RunStoreResult<Option<Value>> {
        let Some((status, run_json)) = self.read_run_row(run_id)? else {
            return Ok(None);
        };
        validated_stored_run(run_id, &status, &run_json).map(Some)
    }

    fn get_events(&self, run_id: &str) -> RunStoreResult<Option<Vec<Value>>> {
        if self.get_run(run_id)?.is_none() {
            return Ok(None);
        }
        let mut statement = self
            .connection
            .prepare(
                "SELECT sequence, kind, fields_json FROM run_events WHERE run_id = ?1 ORDER BY sequence",
            )
            .map_err(sqlite_error)?;
        let rows = statement
            .query_map(params![run_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(sqlite_error)?;
        let mut events = Vec::new();
        for row in rows {
            let (sequence, kind, fields_json) = row.map_err(sqlite_error)?;
            let (sequence, fields) =
                validated_stored_event_fields(sequence, run_id, &kind, &fields_json)?;
            events.push(event_value(sequence, run_id, &kind, fields));
        }
        Ok(Some(events))
    }

    fn recover_interrupted_runs(&mut self) -> RunStoreResult<usize> {
        let transaction = self.connection.transaction().map_err(sqlite_error)?;
        let running = {
            let mut statement = transaction
                .prepare(
                    "SELECT run_id, status, run_json FROM runs WHERE status = 'running' ORDER BY run_id",
                )
                .map_err(sqlite_error)?;
            let rows = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .map_err(sqlite_error)?;
            let mut running = Vec::new();
            for row in rows {
                running.push(row.map_err(sqlite_error)?);
            }
            running
        };

        let now = Utc::now().timestamp_millis();
        for (run_id, status, run_json) in &running {
            let mut run = validated_stored_run(run_id, status, run_json)?;
            let event = interrupt_run(&mut run)?;
            let run_json = serde_json::to_string(&run)?;
            let changed = transaction
                .execute(
                    "UPDATE runs SET status = 'failed', run_json = ?2, updated_at_ms = ?3 WHERE run_id = ?1 AND status = 'running'",
                    params![run_id, run_json, now],
                )
                .map_err(sqlite_error)?;
            if changed != 1 {
                return Err(RunStoreError::Integrity(format!(
                    "recovering run `{run_id}` changed {changed} rows"
                )));
            }
            insert_sqlite_event(&transaction, run_id, event)?;
        }
        transaction.commit().map_err(sqlite_error)?;
        Ok(running.len())
    }

    fn status(&self) -> RunStoreStatus {
        RunStoreStatus {
            mode: "sqlite",
            persistent: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use serde_json::json;

    use super::{
        InMemoryRunEvidenceStore, RunEventDraft, RunEvidenceStore, RunStoreError,
        SqliteRunEvidenceStore, RUN_STORE_SCHEMA_VERSION,
    };

    fn sample_run(id: &str, status: &str) -> serde_json::Value {
        json!({
            "id": id,
            "capability": "brain.plan",
            "loom_session_id": "session-test",
            "status": status,
            "input": { "goal": "persist evidence" }
        })
    }

    fn started_event() -> RunEventDraft {
        RunEventDraft::new(
            "run_started",
            json!({
                "capability": "brain.plan",
                "status": "running"
            }),
        )
        .expect("valid event draft")
    }

    fn unique_sqlite_path(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "loom-run-store-{label}-{}-{nonce}.sqlite",
            std::process::id()
        ))
    }

    fn remove_sqlite_files(path: &Path) {
        for suffix in ["", "-wal", "-shm"] {
            let target = PathBuf::from(format!("{}{}", path.display(), suffix));
            let _ = std::fs::remove_file(target);
        }
    }

    fn exercise_store(store: &mut dyn RunEvidenceStore) {
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
                super::RunStoreStatus {
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
}

#[derive(Debug, Default)]
pub struct InMemoryRunEvidenceStore {
    runs: HashMap<String, Value>,
    events: HashMap<String, Vec<Value>>,
    next_sequence: u64,
}

impl RunEvidenceStore for InMemoryRunEvidenceStore {
    fn insert_run(&mut self, run: Value, events: Vec<RunEventDraft>) -> RunStoreResult<()> {
        let (run_id, _) = validated_run_identity(&run)?;
        let run_id = run_id.to_owned();

        for event in &events {
            validate_event_draft(event)?;
        }
        if self.runs.contains_key(&run_id) {
            return Err(RunStoreError::DuplicateRun(run_id));
        }

        let sequences = sequence_values(self.next_sequence, events.len())?;
        let stored_events = events
            .into_iter()
            .zip(sequences)
            .map(|(event, sequence)| event_value(sequence, &run_id, &event.kind, event.fields))
            .collect::<Vec<_>>();

        if let Some(sequence) = stored_events
            .last()
            .and_then(|event| event["sequence"].as_u64())
        {
            self.next_sequence = sequence;
        }
        self.runs.insert(run_id.clone(), run);
        self.events.insert(run_id, stored_events);
        Ok(())
    }

    fn transition_run(&mut self, run: Value, event: RunEventDraft) -> RunStoreResult<()> {
        let (run_id, _) = validated_run_identity(&run)?;
        let run_id = run_id.to_owned();
        validate_event_draft(&event)?;
        if !self.runs.contains_key(&run_id) {
            return Err(RunStoreError::RunNotFound(run_id));
        }

        let sequence = sequence_values(self.next_sequence, 1)?
            .into_iter()
            .next()
            .expect("one sequence value");
        let stored_event = event_value(sequence, &run_id, &event.kind, event.fields);

        self.next_sequence = sequence;
        self.runs.insert(run_id.clone(), run);
        self.events.entry(run_id).or_default().push(stored_event);
        Ok(())
    }

    fn get_run(&self, run_id: &str) -> RunStoreResult<Option<Value>> {
        Ok(self.runs.get(run_id).cloned())
    }

    fn get_events(&self, run_id: &str) -> RunStoreResult<Option<Vec<Value>>> {
        if !self.runs.contains_key(run_id) {
            return Ok(None);
        }
        Ok(Some(self.events.get(run_id).cloned().unwrap_or_default()))
    }

    fn recover_interrupted_runs(&mut self) -> RunStoreResult<usize> {
        let mut running_ids = Vec::new();
        for (key, run) in &self.runs {
            let (run_id, status) = validated_run_identity(run)?;
            if run_id != key {
                return Err(RunStoreError::Integrity(format!(
                    "run index key `{key}` does not match run id `{run_id}`"
                )));
            }
            if status == "running" {
                running_ids.push(key.clone());
            }
        }
        running_ids.sort();

        let mut recoveries = Vec::with_capacity(running_ids.len());
        for run_id in &running_ids {
            let mut run = self.runs.get(run_id).cloned().ok_or_else(|| {
                RunStoreError::Integrity("run disappeared during recovery".to_owned())
            })?;
            let event = interrupt_run(&mut run)?;
            recoveries.push((run_id.clone(), run, event));
        }

        let sequences = sequence_values(self.next_sequence, recoveries.len())?;
        let stored_events = recoveries
            .iter()
            .zip(sequences)
            .map(|((run_id, _, event), sequence)| {
                event_value(sequence, run_id, &event.kind, event.fields.clone())
            })
            .collect::<Vec<_>>();

        if let Some(sequence) = stored_events
            .last()
            .and_then(|event| event["sequence"].as_u64())
        {
            self.next_sequence = sequence;
        }
        for ((run_id, run, _), event) in recoveries.into_iter().zip(stored_events) {
            self.runs.insert(run_id.clone(), run);
            self.events.entry(run_id).or_default().push(event);
        }
        Ok(running_ids.len())
    }

    fn status(&self) -> RunStoreStatus {
        RunStoreStatus {
            mode: "memory",
            persistent: false,
        }
    }
}
