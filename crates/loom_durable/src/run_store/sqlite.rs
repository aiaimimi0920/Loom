use std::{collections::HashSet, path::Path, time::Duration};

use chrono::Utc;
use rusqlite::{
    params, Connection, ErrorCode, OpenFlags, OptionalExtension, Transaction, TransactionBehavior,
};
use serde_json::{Map, Value};

use super::sqlite_path::{prepare_database_path, restrict_database_file};
use super::{
    core::{
        event_value, interrupt_run, serialized_run_document, validate_event_batch,
        validate_event_draft, validated_run_identity, MAX_EVENT_FIELDS_JSON_BYTES,
        MAX_RUN_JSON_BYTES, MAX_RUN_STORE_JSON_DEPTH,
    },
    RunEventDraft, RunEvidenceStore, RunStoreError, RunStoreResult, RunStoreStatus,
    RUN_STORE_SCHEMA_VERSION,
};

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
    let run = loom_security::json::parse_within_limits(
        run_json,
        &format!("run `{indexed_run_id}` JSON"),
        MAX_RUN_JSON_BYTES,
        MAX_RUN_STORE_JSON_DEPTH,
    )
    .map_err(|error| stored_integrity(format!("run `{indexed_run_id}` is invalid"), error))?;
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
    let fields = loom_security::json::parse_within_limits(
        fields_json,
        &format!("run event sequence `{sequence}` fields JSON"),
        MAX_EVENT_FIELDS_JSON_BYTES,
        MAX_RUN_STORE_JSON_DEPTH,
    )
    .map_err(|error| {
        stored_integrity(
            format!("run event sequence `{sequence}` fields are invalid"),
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
    pub(super) connection: Connection,
}

impl SqliteRunEvidenceStore {
    pub fn open(path: impl AsRef<Path>) -> RunStoreResult<Self> {
        let path = prepare_database_path(path.as_ref())?;
        let flags = OpenFlags::default() | OpenFlags::SQLITE_OPEN_NOFOLLOW;
        let mut connection = Connection::open_with_flags(&path, flags).map_err(sqlite_error)?;
        restrict_database_file(&path)?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(sqlite_error)?;
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .map_err(sqlite_error)?;
        let current_journal_mode: String = connection
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .map_err(sqlite_error)?;
        let journal_mode = if current_journal_mode.eq_ignore_ascii_case("wal") {
            current_journal_mode
        } else {
            connection
                .pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))
                .map_err(sqlite_error)?
        };
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
                let transaction = connection
                    .transaction_with_behavior(TransactionBehavior::Immediate)
                    .map_err(sqlite_error)?;
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
        validate_event_batch(&events)?;
        let run_json = serialized_run_document(&run)?;
        let now = Utc::now().timestamp_millis();

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sqlite_error)?;
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
        let run_json = serialized_run_document(&run)?;
        let now = Utc::now().timestamp_millis();

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sqlite_error)?;
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
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sqlite_error)?;
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
            let run_json = serialized_run_document(&run)?;
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
