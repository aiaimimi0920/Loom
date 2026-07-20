# Loom Durable Run Evidence Store Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Persist Loom daemon capability runs and events in SQLite so evidence survives daemon restarts, interrupted runs become explicit terminal failures, and existing HTTP clients retain their current contracts.

**Architecture:** `loom_durable` gains a synchronous `RunEvidenceStore` boundary with in-memory and bundled-SQLite implementations. The daemon binary explicitly selects SQLite beneath the Loom control-plane root, while library tests remain in memory; every run transition and corresponding event commits in one transaction, and startup validates and recovers stale `running` records before serving traffic.

**Tech Stack:** Rust 2021, `rusqlite 0.40.1` with bundled SQLite, Serde/JSON, Chrono, Loom's synchronous HTTP daemon, PowerShell 5.1-compatible release smoke tooling.

---

## File Map

- Modify `Loom/Cargo.toml`: add the workspace `rusqlite` dependency.
- Modify `Loom/Cargo.lock`: lock bundled SQLite dependencies.
- Modify `Loom/crates/loom_durable/Cargo.toml`: add Chrono, Serde, and Rusqlite dependencies.
- Modify `Loom/crates/loom_durable/src/lib.rs`: export the run-evidence module.
- Create `Loom/crates/loom_durable/src/run_store.rs`: store contracts, validation, in-memory implementation, SQLite implementation, schema, recovery, and unit tests.
- Modify `Loom/apps/daemon/src/lib.rs`: store configuration, status metadata, durable route transitions, safe storage errors, recovery integration, and daemon tests.
- Modify `Loom/apps/daemon/src/main.rs`: select the persistent store for the real daemon binary.
- Modify `Loom/apps/daemon/tests/daemon_cli_contract.rs`: isolate control-plane roots and prove binary-created SQLite state.
- Create `Loom/scripts/Invoke-LoomRunPersistenceSmoke.ps1`: packaged restart and desktop auto-start smoke with UTF-8 evidence.
- Create `Loom/scripts/Test-LoomRunPersistenceSmokeContract.ps1`: parse and behavior contract for the packaged smoke script.
- Modify `Loom/README.md`: document persistent run evidence and local data behavior.
- Modify `Loom/docs/ARCHITECTURE.md`: document the durable HTTP run-evidence boundary and recovery flow.
- Modify `Loom/docs/GATEWAY_INTEGRATION.md`: document persisted Gateway success/failure evidence and restart interruption semantics.
- Create `docs/loom/progress/phase-40-run-event-persistence.md`: Phase 40 task and evidence tracker.
- Modify `docs/loom/progress/MASTER.md`: register Phase 40, current status, and final release evidence.

## Task 1: Define the Run-Evidence Contract and In-Memory Backend

**Files:**
- Modify: `Loom/crates/loom_durable/Cargo.toml`
- Modify: `Loom/crates/loom_durable/src/lib.rs`
- Create: `Loom/crates/loom_durable/src/run_store.rs`

- [ ] **Step 1: Add failing contract tests for the public store behavior**

Create `run_store.rs` with a `#[cfg(test)]` module that describes the intended API before defining it. Use these fixtures and assertions:

```rust
#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        InMemoryRunEvidenceStore, RunEventDraft, RunEvidenceStore, RunStoreError,
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
                RunEventDraft::new(
                    "capability_completed",
                    json!({ "status": "succeeded" }),
                )
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
        assert!(RunEventDraft::new("run_started", json!([])).is_err());
    }
}
```

Add only this module and `pub mod run_store;`/re-export declarations required for the compiler to reach it. Do not implement the types yet.

- [ ] **Step 2: Run the targeted test and verify RED**

Run:

```powershell
$env:CARGO_TARGET_DIR = "C:\t\loom-run-persistence"
cargo test --manifest-path Loom/Cargo.toml -p loom_durable run_store
```

Expected: compilation fails because `RunEvidenceStore`, `RunEventDraft`, `RunStoreError`, and `InMemoryRunEvidenceStore` are not defined.

- [ ] **Step 3: Add the public error, draft, status, and trait contracts**

Add these dependencies to `Loom/crates/loom_durable/Cargo.toml`:

```toml
chrono.workspace = true
serde.workspace = true
thiserror.workspace = true
```

Define the public contracts in `run_store.rs`:

```rust
use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
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
```

Add private helpers `validated_run_identity`, `event_value`, and `interrupt_run` so both backends use identical validation and recovery JSON.

- [ ] **Step 4: Implement the in-memory backend minimally**

Implement:

```rust
#[derive(Debug, Default)]
pub struct InMemoryRunEvidenceStore {
    runs: HashMap<String, Value>,
    events: HashMap<String, Vec<Value>>,
    next_sequence: u64,
}
```

Required behavior:

- `insert_run` validates the full event batch before mutating state, rejects duplicate IDs, assigns sequences in input order, then inserts run and events.
- `transition_run` requires an existing ID, validates the run and event before mutation, updates the canonical run, and appends one event.
- `get_events` returns `None` when the run does not exist and an ordered vector otherwise.
- `recover_interrupted_runs` finds current `running` records, changes each to `failed`, writes the approved `daemon_restarted` error, and appends `run_interrupted` in deterministic run-ID order.
- `status` returns `RunStoreStatus { mode: "memory", persistent: false }`.

- [ ] **Step 5: Add recovery and atomic validation tests**

Add:

```rust
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
    let events = store.get_events("run-b").expect("events").unwrap();
    assert_eq!(events.last().unwrap()["kind"], "run_interrupted");
}

#[test]
fn invalid_event_batch_leaves_no_partial_run() {
    let mut store = InMemoryRunEvidenceStore::default();
    let result = store.insert_run(
        sample_run("partial", "running"),
        vec![RunEventDraft {
            kind: String::new(),
            fields: serde_json::Map::new(),
        }],
    );
    assert!(result.is_err());
    assert_eq!(store.get_run("partial").expect("read"), None);
}
```

- [ ] **Step 6: Run the crate tests and format check**

Run:

```powershell
$env:CARGO_TARGET_DIR = "C:\t\loom-run-persistence"
cargo fmt --manifest-path Loom/Cargo.toml --all -- --check
cargo test --manifest-path Loom/Cargo.toml -p loom_durable
```

Expected: all existing durable tests and the new run-store contract tests pass.

- [ ] **Step 7: Commit the contract and memory backend**

```powershell
git add -- Loom/crates/loom_durable/Cargo.toml Loom/crates/loom_durable/src/lib.rs Loom/crates/loom_durable/src/run_store.rs
git commit -m "feat(loom): define durable run evidence store"
```

## Task 2: Implement the Bundled SQLite Backend

**Files:**
- Modify: `Loom/Cargo.toml`
- Modify: `Loom/Cargo.lock`
- Modify: `Loom/crates/loom_durable/Cargo.toml`
- Modify: `Loom/crates/loom_durable/src/run_store.rs`

- [ ] **Step 1: Add failing SQLite reopen, sequence, schema, and recovery tests**

Add a `unique_sqlite_path` test helper and these tests:

```rust
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
        let mut run = store.get_run("run-1").expect("read").unwrap();
        assert_eq!(run["status"], "failed");
        assert_eq!(run["error"]["code"], "daemon_restarted");
        run["status"] = json!("retrying");
        store
            .transition_run(
                run,
                RunEventDraft::new("run_action", json!({"action":"retrying"}))
                    .expect("draft"),
            )
            .expect("transition");
        let events = store.get_events("run-1").expect("events").unwrap();
        assert_eq!(events.iter().map(|event| event["sequence"].as_u64().unwrap()).collect::<Vec<_>>(), vec![1, 2, 3]);
    }
    remove_sqlite_files(&path);
}

#[test]
fn sqlite_store_rejects_newer_schema() {
    let path = unique_sqlite_path("newer-schema");
    let connection = rusqlite::Connection::open(&path).expect("open fixture");
    connection.pragma_update(None, "user_version", 2).expect("set schema");
    drop(connection);
    assert!(matches!(
        SqliteRunEvidenceStore::open(&path),
        Err(RunStoreError::UnsupportedSchema { found: 2, supported: 1 })
    ));
    remove_sqlite_files(&path);
}

#[test]
fn sqlite_duplicate_insert_leaves_existing_events_unchanged() {
    let path = unique_sqlite_path("duplicate");
    let mut store = SqliteRunEvidenceStore::open(&path).expect("open store");
    store
        .insert_run(sample_run("run-1", "running"), vec![started_event()])
        .expect("first insert");
    assert!(matches!(
        store.insert_run(sample_run("run-1", "running"), vec![started_event()]),
        Err(RunStoreError::DuplicateRun(id)) if id == "run-1"
    ));
    assert_eq!(store.get_events("run-1").expect("events").unwrap().len(), 1);
    remove_sqlite_files(&path);
}
```

Also add tests that create a row with malformed `run_json` and a row with array-valued `fields_json`; reopening must fail with `RunStoreError::Integrity`.

- [ ] **Step 2: Run the SQLite tests and verify RED**

Run:

```powershell
$env:CARGO_TARGET_DIR = "C:\t\loom-run-persistence"
cargo test --manifest-path Loom/Cargo.toml -p loom_durable sqlite_store
```

Expected: compilation fails because `SqliteRunEvidenceStore` and the Rusqlite dependency do not exist.

- [ ] **Step 3: Add the exact bundled SQLite dependency**

Add to `Loom/Cargo.toml`:

```toml
rusqlite = { version = "0.40.1", default-features = false, features = ["bundled"] }
```

Add to `Loom/crates/loom_durable/Cargo.toml`:

```toml
rusqlite.workspace = true
```

Run `cargo metadata --manifest-path Loom/Cargo.toml --format-version 1 --no-deps` once to update dependency validation, then allow Cargo to update `Loom/Cargo.lock` during the first test build.

- [ ] **Step 4: Implement connection setup and schema version 1**

Add:

```rust
pub const RUN_STORE_SCHEMA_VERSION: i32 = 1;

pub struct SqliteRunEvidenceStore {
    connection: rusqlite::Connection,
}
```

`SqliteRunEvidenceStore::open` must:

1. Create the database parent directory when present.
2. Open the connection.
3. Set a five-second busy timeout.
4. Enable `foreign_keys`, `journal_mode = WAL`, and `synchronous = FULL`.
5. Read `user_version`.
6. Create the approved schema in one transaction when the version is zero.
7. Reject versions greater than one.
8. Run `PRAGMA quick_check` and require exactly `ok`.
9. Parse and validate every row in `runs` and `run_events`.
10. Recover interrupted runs before returning the store.

Use this schema verbatim:

```rust
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
```

Convert `rusqlite::Error` to `RunStoreError::Sqlite(error.to_string())` at the crate boundary.

- [ ] **Step 5: Implement transactional insert, transition, reads, and recovery**

`insert_run` must pre-validate the run and entire event batch, start one transaction, insert the run row, insert event rows in order, and commit. Map primary-key conflicts to `DuplicateRun`.

`transition_run` must pre-validate, require the run to exist, update `status`, `run_json`, and `updated_at_ms`, insert one event, and commit in the same transaction.

Reconstruct each public event with:

```rust
fn event_value(sequence: u64, run_id: &str, kind: &str, fields: Map<String, Value>) -> Value {
    let mut event = serde_json::json!({
        "sequence": sequence,
        "kind": kind,
        "run_id": run_id,
    });
    let target = event.as_object_mut().expect("event object");
    target.extend(fields);
    event
}
```

Recovery must select `status = 'running'` ordered by `run_id`, update every canonical JSON record, insert `run_interrupted`, and commit all recovered rows together.

`status` returns `RunStoreStatus { mode: "sqlite", persistent: true }`.

- [ ] **Step 6: Run SQLite and complete durable crate tests**

Run:

```powershell
$env:CARGO_TARGET_DIR = "C:\t\loom-run-persistence"
cargo fmt --manifest-path Loom/Cargo.toml --all -- --check
cargo test --manifest-path Loom/Cargo.toml -p loom_durable
```

Expected: all memory and SQLite contract tests pass, including reopen, recovery, corruption, and sequence tests.

- [ ] **Step 7: Commit the SQLite backend**

```powershell
git add -- Loom/Cargo.toml Loom/Cargo.lock Loom/crates/loom_durable/Cargo.toml Loom/crates/loom_durable/src/run_store.rs
git commit -m "feat(loom): persist run evidence in sqlite"
```

## Task 3: Wire Store Selection and Safe Status into the Daemon

**Files:**
- Modify: `Loom/apps/daemon/src/lib.rs`
- Modify: `Loom/apps/daemon/src/main.rs`
- Modify: `Loom/apps/daemon/tests/daemon_cli_contract.rs`

- [ ] **Step 1: Add failing daemon configuration and status tests**

Add tests requiring the library default and explicit SQLite modes:

```rust
#[test]
fn daemon_reports_in_memory_run_store_by_default() {
    let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
    let address = daemon.local_addr().expect("address");
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve"));
    let status = http_json_get(address.port(), "/status");
    assert_eq!(status["run_store"]["mode"], "memory");
    assert_eq!(status["run_store"]["persistent"], false);
    shutdown_tx.send(()).expect("shutdown");
    server.join().expect("server");
}

#[test]
fn daemon_reports_explicit_sqlite_run_store() {
    let root = unique_temp_dir("sqlite-status");
    let path = root.join("runs").join("loom-runs.sqlite3");
    let daemon = LoomDaemon::bind(
        DaemonConfig::localhost(0).with_sqlite_run_store(&path),
    )
    .expect("bind daemon");
    let address = daemon.local_addr().expect("address");
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve"));
    let status = http_json_get(address.port(), "/status");
    assert_eq!(status["run_store"]["mode"], "sqlite");
    assert_eq!(status["run_store"]["persistent"], true);
    shutdown_tx.send(()).expect("shutdown");
    server.join().expect("server");
    assert!(path.exists());
    fs::remove_dir_all(root).expect("cleanup");
}
```

Extend the help test with:

```rust
assert!(help.contains("LOOM_RUN_STORE_PATH"));
```

- [ ] **Step 2: Run targeted daemon tests and verify RED**

```powershell
$env:CARGO_TARGET_DIR = "C:\t\loom-run-persistence"
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon daemon_reports_ --lib
```

Expected: failure because `with_sqlite_run_store` and `run_store` status metadata do not exist.

- [ ] **Step 3: Add `RunStoreConfig`, path resolution, and daemon fields**

Add:

```rust
#[derive(Clone, Debug, Eq, PartialEq)]
enum RunStoreConfig {
    Memory,
    Sqlite(PathBuf),
}
```

Add `run_store: RunStoreConfig` to `DaemonConfig`, default it to `Memory`, and expose:

```rust
#[must_use]
pub fn with_sqlite_run_store(mut self, path: impl Into<PathBuf>) -> Self {
    self.run_store = RunStoreConfig::Sqlite(path.into());
    self
}

#[must_use]
pub fn default_run_store_path() -> PathBuf {
    std::env::var_os("LOOM_RUN_STORE_PATH")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| {
            default_control_plane_root()
                .join("runs")
                .join("loom-runs.sqlite3")
        })
}
```

Replace the local daemon `RunStore` with:

```rust
type SharedRunStore = Arc<Mutex<Box<dyn RunEvidenceStore>>>;
```

Construct the selected backend in `LoomDaemon::bind`, call `recover_interrupted_runs` before returning, and cache its `RunStoreStatus` in a new `run_store_status` field.

- [ ] **Step 4: Add status and help output**

Add `run_store: RunStoreStatus` to `StatusResponse` with `#[serde(rename_all = "camelCase")]` behavior matching the approved JSON. Pass the cached status through `route` and include it in `GET /status`.

Add to daemon help:

```text
  LOOM_RUN_STORE_PATH  SQLite run evidence path [default: <control-plane>\\runs\\loom-runs.sqlite3]
```

Do not include the resolved absolute path in status.

- [ ] **Step 5: Make the real binary select persistence**

Update `main.rs`:

```rust
use loom_daemon::{
    daemon_help_text, daemon_version_text, default_run_store_path, DaemonConfig, LoomDaemon,
};

let mut config = DaemonConfig::bind_host(host, port)
    .with_brain_planner_from_env()?
    .with_sqlite_run_store(default_run_store_path());
```

This preserves memory mode for direct library tests while making the packaged binary persistent by default.

- [ ] **Step 6: Isolate all daemon CLI contract processes**

In every existing `Command::new(exe)` block in `daemon_cli_contract.rs`, set a unique control-plane root:

```rust
.env("LOOM_CONTROL_PLANE_ROOT", temp_dir.join("control-plane"))
```

Add a new binary contract:

```rust
#[test]
fn daemon_binary_creates_sqlite_run_store_under_control_plane_root() {
    let temp_dir = unique_temp_dir("sqlite-store");
    let manifest_dir = temp_dir.join("capabilities");
    let control_plane_root = temp_dir.join("control-plane");
    let exe = env!("CARGO_BIN_EXE_loom-daemon");
    let child = Command::new(exe)
        .env("LOOM_DAEMON_HOST", "127.0.0.1")
        .env("LOOM_DAEMON_PORT", "0")
        .env("LOOM_CAPABILITY_MANIFEST_DIR", &manifest_dir)
        .env("LOOM_CONTROL_PLANE_ROOT", &control_plane_root)
        .spawn()
        .expect("spawn daemon");
    let _manifest = wait_for_manifest(&manifest_dir.join("loom.json"));
    stop_child(child);
    assert!(control_plane_root
        .join("runs")
        .join("loom-runs.sqlite3")
        .exists());
    fs::remove_dir_all(temp_dir).expect("cleanup");
}
```

- [ ] **Step 7: Run daemon status and CLI contracts**

```powershell
$env:CARGO_TARGET_DIR = "C:\t\loom-run-persistence"
cargo fmt --manifest-path Loom/Cargo.toml --all -- --check
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon daemon_reports_ --lib
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon --test daemon_cli_contract
```

Expected: status reports the correct mode, the real binary creates the database in the isolated root, and all CLI contracts pass.

- [ ] **Step 8: Commit daemon store selection**

```powershell
git add -- Loom/apps/daemon/src/lib.rs Loom/apps/daemon/src/main.rs Loom/apps/daemon/tests/daemon_cli_contract.rs
git commit -m "feat(loom): configure persistent daemon run storage"
```

## Task 4: Make Every Run Transition Durable and Canonical

**Files:**
- Modify: `Loom/apps/daemon/src/lib.rs`

- [ ] **Step 1: Add failing restart persistence tests**

Add a helper that starts a daemon on an explicit SQLite path, returns its port and shutdown handles, then write:

```rust
#[test]
fn daemon_reads_brain_plan_run_after_restart() {
    let root = unique_temp_dir("run-restart");
    let path = root.join("runs.sqlite3");
    let (port, shutdown, server) = start_daemon_with_store(&path, BrainPlannerConfig::LocalTemplate);
    let invoke = http_json_post(
        port,
        "/v1/invoke",
        r#"{"requestId":"persist-1","caller":"hook","capability":"brain.plan","input":{"goal":"survive restart"}}"#,
    );
    let run_id = invoke["output"]["runId"].as_str().unwrap().to_owned();
    shutdown.send(()).expect("shutdown");
    server.join().expect("server");

    let (port, shutdown, server) = start_daemon_with_store(&path, BrainPlannerConfig::LocalTemplate);
    let run = http_json_get(port, &format!("/v1/runs/{run_id}"));
    let events = http_json_get(port, &format!("/v1/runs/{run_id}/events"));
    assert_eq!(run["status"], "succeeded");
    assert_eq!(events["events"][0]["kind"], "run_started");
    assert_eq!(events["events"][1]["kind"], "capability_completed");
    shutdown.send(()).expect("shutdown");
    server.join().expect("server");
    fs::remove_dir_all(root).expect("cleanup");
}
```

Add equivalent tests for a failed Gateway run and a pre-seeded `running` run that must return as `failed` with a final `run_interrupted` event after bind.

- [ ] **Step 2: Add a failing canonical stop/retry test**

Extend `daemon_validates_stop_and_retry_path_run_ids` with a forged body:

```rust
let forged = serde_json::json!({
    "run": {
        "id": run_id,
        "status": "succeeded",
        "input": { "goal": "forged" },
        "output": { "summary": "forged" }
    }
});
let stopped = http_json_post(
    address.port(),
    &format!("/v1/runs/{run_id}/stop"),
    &forged.to_string(),
);
assert_eq!(stopped["status"], "stopped");
assert_eq!(stopped["input"]["goal"], "validate run id");
assert_ne!(stopped["output"]["summary"], "forged");
```

- [ ] **Step 3: Add a failing safe storage-error test**

Inside the daemon test module, define a `FailingRunEvidenceStore` whose methods return `RunStoreError::Integrity("fixture failure".to_owned())`. Call the invoke route with this store and assert:

```rust
assert_eq!(status, 500);
let body: serde_json::Value = serde_json::from_str(&body).expect("json");
assert_eq!(body["error"]["code"], "run_store_failed");
assert!(!body.to_string().contains("fixture failure"));
```

The test must call a second public route after the failure and prove routing still works, demonstrating that the error did not escape and terminate the server loop.

- [ ] **Step 4: Run the tests and verify RED**

```powershell
$env:CARGO_TARGET_DIR = "C:\t\loom-run-persistence"
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon daemon_reads_brain_plan_run_after_restart --lib
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon daemon_validates_stop_and_retry_path_run_ids --lib
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon run_store_failure_returns_safe_http_error --lib
```

Expected: restart reads fail because routes still use the removed memory store operations, forged fields are accepted, and storage errors propagate as `anyhow`.

- [ ] **Step 5: Replace route mutations with store transactions**

For every run-producing route:

- Build `RunEventDraft` values instead of pre-assigning sequence numbers.
- Call `insert_run` before a blocking Gateway request begins.
- Call `transition_run` for terminal success/failure.
- Return success only after the transaction commits.
- Map store errors to a safe HTTP response instead of using `?` through the accept loop.

Add:

```rust
fn run_store_failed(error: RunStoreError) -> Result<(u16, String)> {
    eprintln!("loom run store operation failed: {error}");
    structured_error(
        500,
        json!({
            "code": "run_store_failed",
            "message": "Loom run evidence could not be stored"
        }),
    )
}
```

Do not include the underlying error in the HTTP body.

- [ ] **Step 6: Make stop/retry load canonical storage state**

After path/body ID validation, replace `let mut run = request.run` with:

```rust
let mut store = match lock_run_store(run_store) {
    Ok(store) => store,
    Err(error) => return run_store_failed(error),
};
let Some(mut run) = match store.get_run(path_run_id) {
    Ok(run) => run,
    Err(error) => return run_store_failed(error),
} else {
    return run_not_found(path_run_id);
};
run["status"] = json!(status);
```

Persist the canonical run and `run_action` through one `transition_run` call.

- [ ] **Step 7: Run all daemon run and Gateway tests**

```powershell
$env:CARGO_TARGET_DIR = "C:\t\loom-run-persistence"
cargo fmt --manifest-path Loom/Cargo.toml --all -- --check
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon daemon_invokes_brain_plan_and_serves_run_and_events --lib
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon daemon_invokes_gateway_brain_plan_and_forwards_input --lib
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon daemon_records_failed_gateway_brain_plan_with_run_evidence --lib
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon daemon_reads_brain_plan_run_after_restart --lib
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon daemon_validates_stop_and_retry_path_run_ids --lib
```

Expected: existing response/event contracts remain unchanged, restart tests pass, and forged data is rejected.

- [ ] **Step 8: Commit durable daemon transitions**

```powershell
git add -- Loom/apps/daemon/src/lib.rs
git commit -m "feat(loom): preserve run evidence across restart"
```

## Task 5: Add a Packaged Restart and Desktop Auto-Start Smoke

**Files:**
- Create: `Loom/scripts/Test-LoomRunPersistenceSmokeContract.ps1`
- Create: `Loom/scripts/Invoke-LoomRunPersistenceSmoke.ps1`

- [ ] **Step 1: Write the failing PowerShell contract**

Create `Test-LoomRunPersistenceSmokeContract.ps1` that resolves the sibling smoke script, parses it with `System.Management.Automation.Language.Parser`, and asserts all of these literal contracts:

```powershell
Assert-Contains '[string]$PackageDir' $raw "Smoke must accept a package directory."
Assert-Contains 'LOOM_CONTROL_PLANE_ROOT' $raw "Smoke must isolate the control-plane root."
Assert-Contains 'loom-runs.sqlite3' $raw "Smoke must assert the default database path."
Assert-Contains '/v1/runs/' $raw "Smoke must query persisted runs."
Assert-Contains 'loom-desktop.exe' $raw "Smoke must retain desktop auto-start coverage."
Assert-Contains 'ExecutablePath' $raw "Smoke cleanup must identify candidate processes by exact path."
Assert-Contains 'UTF8Encoding' $raw "Smoke evidence must be UTF-8 without BOM."
Assert-Contains 'candidateProcessesAfterCleanup' $raw "Smoke must record process cleanup."
Assert-Contains 'desktopAliveDuringAssertions' $raw "Smoke must prove the desktop remained alive."
```

Use local `Assert-Contains` and `Assert-Equal` helpers and print:

```text
Loom run persistence smoke contract passed.
```

- [ ] **Step 2: Run the contract and verify RED**

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Loom\scripts\Test-LoomRunPersistenceSmokeContract.ps1
```

Expected: failure because `Invoke-LoomRunPersistenceSmoke.ps1` does not exist.

- [ ] **Step 3: Implement the packaged persistence smoke**

The script must be Windows PowerShell 5.1-compatible and accept:

```powershell
param(
    [Parameter(Mandatory = $true)]
    [string]$PackageDir,
    [string]$EvidenceRoot = ""
)
```

Implement these exact phases with one shared isolated control-plane root:

1. Resolve `loom.exe`, `loom-daemon.exe`, and `loom-desktop.exe` from `PackageDir`.
2. Allocate a dynamic port and start daemon A with only `LOOM_CONTROL_PLANE_ROOT`, isolated `APPDATA`/`LOCALAPPDATA`, and cleared `LOOM_GATEWAY_*`/`LOOM_DAEMON_TOKEN`.
3. Assert `/status.run_store = { mode: "sqlite", persistent: true }`.
4. Invoke local `brain.plan`, save its `runId`, run JSON, and two events.
5. Stop daemon A by its exact PID and assert the default database exists at `control-plane\\runs\\loom-runs.sqlite3`.
6. Start daemon B on a second dynamic port with the same roots.
7. Query the original run and events and assert they match daemon A evidence and retain ordered sequence values.
8. Run `loom.exe status --daemon-url <daemon-B-url>` and require exit code zero.
9. Stop daemon B.
10. Start only `loom-desktop.exe` on a third dynamic `LOOM_DAEMON_URL`, identify candidate desktop/daemon processes by exact `ExecutablePath`, and prove one sibling daemon starts with the same SQLite status.
11. Stop only the exact PIDs created by the smoke.
12. Write `summary.json`, response JSON, and process snapshots with `UTF8Encoding(false)`.

The final summary must include:

```powershell
$summary = [ordered]@{
    schemaVersion = 1
    status = "passed"
    packageDir = $packageFullPath
    databasePath = $databasePath
    firstDaemonPid = $firstDaemonPid
    secondDaemonPid = $secondDaemonPid
    runId = $runId
    firstEventSequences = @($firstEvents.events | ForEach-Object { [int64]$_.sequence })
    persistedEventSequences = @($persistedEvents.events | ForEach-Object { [int64]$_.sequence })
    persistedStatus = [string]$persistedRun.status
    cliExitCode = [int]$cliResult.exitCode
    desktopPid = $desktopPid
    desktopDaemonPid = $desktopDaemonPid
    desktopAliveDuringAssertions = $desktopAliveDuringAssertions
    siblingParentMatched = $siblingParentMatched
    candidateProcessesAfterCleanup = $candidateProcessesAfterCleanup
    evidenceDir = $evidenceRunDir
}
```

On failure, write a failed summary in `finally`, redact authorization/token text, and still clean exact candidate PIDs.

- [ ] **Step 4: Run the PowerShell contract and debug-package smoke**

Build debug binaries and use the target directory as the package directory:

```powershell
$env:CARGO_TARGET_DIR = "C:\t\loom-run-persistence"
cargo build --manifest-path Loom/Cargo.toml -p loom-daemon -p loom-cli
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Loom\scripts\Test-LoomRunPersistenceSmokeContract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Loom\scripts\Invoke-LoomRunPersistenceSmoke.ps1 -PackageDir "C:\t\loom-run-persistence\debug"
```

Expected: contract passes; persistence and CLI assertions pass. The desktop phase may be skipped only for this debug-directory run because `loom-desktop.exe` is produced by the Tauri target, not Cargo's shared target. The script must report `desktopSkipped = true` only when the desktop executable is absent; formal release runs must require it.

- [ ] **Step 5: Commit the smoke tooling**

```powershell
git add -- Loom/scripts/Test-LoomRunPersistenceSmokeContract.ps1 Loom/scripts/Invoke-LoomRunPersistenceSmoke.ps1
git commit -m "test(loom): smoke durable run evidence"
```

## Task 6: Document the Persistent Runtime and Start Phase 40 Tracking

**Files:**
- Modify: `Loom/README.md`
- Modify: `Loom/docs/ARCHITECTURE.md`
- Modify: `Loom/docs/GATEWAY_INTEGRATION.md`
- Create: `docs/loom/progress/phase-40-run-event-persistence.md`
- Modify: `docs/loom/progress/MASTER.md`

- [ ] **Step 1: Update product and architecture documentation**

Add to `Loom/README.md`:

```markdown
## Persistent run evidence

The packaged daemon stores capability runs and events in SQLite beneath the
Loom control-plane root. Set `LOOM_RUN_STORE_PATH` to override the database
file. Library-level daemon tests use an in-memory store unless they explicitly
select SQLite.

Runs left in `running` state by a daemon interruption are marked `failed` with
`daemon_restarted` and receive a `run_interrupted` event on the next startup.
Loom does not automatically replay interrupted model or tool calls.
```

Replace the process-local paragraph in `Loom/docs/ARCHITECTURE.md` with the approved `RunEvidenceStore` ownership, SQLite path, transaction, recovery, and typed-event separation rules.

Add to `Loom/docs/GATEWAY_INTEGRATION.md` that Gateway-backed success and failure runs survive restart, tokens/prompts remain excluded, and interrupted calls are terminalized without replay.

- [ ] **Step 2: Create the Phase 40 progress document**

Create `phase-40-run-event-persistence.md` with seven tasks:

```markdown
# Phase 40: Durable Run and Event Persistence

## Goal

Persist Loom capability run evidence across daemon restarts and recover stale
running records without replaying side effects.

## Tasks

- [x] P40.1 Define the run-evidence store contract and memory backend.
- [x] P40.2 Implement bundled SQLite schema, validation, and recovery.
- [x] P40.3 Wire persistent binary configuration and safe status metadata.
- [x] P40.4 Make run transitions transactional and canonical.
- [x] P40.5 Add packaged restart and desktop auto-start smoke tooling.
- [ ] P40.6 Complete full workspace and release validation.
- [ ] P40.7 Generate and verify the formal release candidate.

## Non-goals

- Async workers, automatic replay, cancellation leases, retention, export, and encryption.
```

Register Phase 40 in `MASTER.md` as `(5/7 tasks)` and make it the active phase without changing the historical Phase 39 evidence.

- [ ] **Step 3: Run documentation-bearing contracts**

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-desktop-shell-contract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Loom\scripts\Test-LoomRunPersistenceSmokeContract.ps1
git diff --check -- Loom docs/loom
```

Expected: all contracts pass and no whitespace errors are reported.

- [ ] **Step 4: Commit implementation documentation**

```powershell
git add -- Loom/README.md Loom/docs/ARCHITECTURE.md Loom/docs/GATEWAY_INTEGRATION.md docs/loom/progress/phase-40-run-event-persistence.md docs/loom/progress/MASTER.md
git commit -m "docs(loom): document durable run persistence"
```

## Task 7: Run the Full Source Validation Matrix

**Files:**
- Verify only; no planned edits.

- [ ] **Step 1: Run formatting and workspace compilation**

```powershell
$env:CARGO_TARGET_DIR = "C:\t\loom-run-persistence"
cargo fmt --manifest-path Loom/Cargo.toml --all -- --check
cargo check --manifest-path Loom/Cargo.toml --workspace --all-targets --locked
```

Expected: both commands exit zero.

- [ ] **Step 2: Run durable, daemon, CLI, and complete workspace tests**

```powershell
$env:CARGO_TARGET_DIR = "C:\t\loom-run-persistence"
cargo test --manifest-path Loom/Cargo.toml -p loom_durable --locked
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon --locked
cargo test --manifest-path Loom/Cargo.toml -p loom-cli --locked
cargo test --manifest-path Loom/Cargo.toml --workspace --locked
```

Expected: all unit, integration, and doc tests pass.

- [ ] **Step 3: Run desktop validation**

```powershell
Push-Location .\Loom\apps\desktop
try {
    npm run typecheck
    npm run build
}
finally {
    Pop-Location
}

$env:CARGO_TARGET_DIR = "C:\t\loom-run-persistence-tauri"
cargo check --manifest-path .\Loom\apps\desktop\src-tauri\Cargo.toml --locked
```

Expected: TypeScript, Rsbuild, and Tauri checks pass.

- [ ] **Step 4: Run PowerShell contracts and debug persistence smoke**

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-desktop-shell-contract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Loom\scripts\Test-LoomRunPersistenceSmokeContract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Loom\scripts\Invoke-LoomRunPersistenceSmoke.ps1 -PackageDir "C:\t\loom-run-persistence\debug"
```

Expected: contracts pass, debug restart evidence passes, and no debug daemon PID remains.

- [ ] **Step 5: Inspect the final source boundary**

```powershell
git status --porcelain --untracked-files=all -- Loom scripts/build-release-exes.ps1 docs/loom
git diff --check -- Loom docs/loom
git log -8 --oneline
```

Expected: implementation files are committed, Loom release source scope is clean, and unrelated Gateway/Platform/Tea/Hook changes remain untouched.

- [ ] **Step 6: Mark Phase 40 validation complete**

Update Phase 40 to `(6/7 tasks)` and record exact test counts and debug evidence path. Commit only the progress documents:

```powershell
git add -- docs/loom/progress/phase-40-run-event-persistence.md docs/loom/progress/MASTER.md
git commit -m "docs(loom): record run persistence validation"
```

## Task 8: Build, Verify, and Close the Phase 40 Release

**Files:**
- Release output: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\<versionId>`
- Modify after release: `docs/loom/progress/phase-40-run-event-persistence.md`
- Modify after release: `docs/loom/progress/MASTER.md`

- [ ] **Step 1: Confirm the approved source scope is clean**

```powershell
git status --porcelain --untracked-files=all -- Loom scripts/build-release-exes.ps1
```

Expected: no output. Do not build a formal candidate while this scope is dirty.

- [ ] **Step 2: Generate a unique version ID and build only Loom**

```powershell
$shortSha = (git rev-parse --short=8 HEAD).Trim()
$versionId = "$(Get-Date -Format 'yyyyMMdd-HHmmss')-$shortSha"
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release-exes.ps1 -VersionId $versionId -Apps Loom
```

Expected: output is written only to `release\Loom\$versionId`; the manifest records repository-wide `gitDirty`, `sourceGitDirty=false`, and exact source paths `Loom` plus `scripts/build-release-exes.ps1`.

- [ ] **Step 3: Run formal verification and the broad local release smoke**

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-release.ps1 -VersionId $versionId -Apps Loom
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\smoke-release-local-apps.ps1 -VersionId $versionId -Apps Loom
```

Expected: formal status `passed`, 31 or more checksum entries depending on generated support files, and the existing Loom runtime matrix remains green.

- [ ] **Step 4: Run the packaged persistence, Gateway, and desktop checks**

```powershell
$packageDir = Join-Path (Resolve-Path .\release\Loom).Path $versionId
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Loom\scripts\Invoke-LoomRunPersistenceSmoke.ps1 -PackageDir $packageDir
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Loom\scripts\Invoke-LoomGatewayBrainPlanSmoke.ps1 -PackageDir $packageDir
```

Expected persistence summary:

```text
status = passed
persistedStatus = succeeded
firstEventSequences = persistedEventSequences
cliExitCode = 0
desktopAliveDuringAssertions = true
siblingParentMatched = true
candidateProcessesAfterCleanup = []
```

Expected Gateway summary remains `plannerSource=gateway`, run succeeded, event order is `run_started,capability_completed`, and all processes/jobs stop.

- [ ] **Step 5: Verify package identity and checksum**

```powershell
$manifest = Get-Content -Raw -Encoding UTF8 (Join-Path $packageDir "manifest.json") | ConvertFrom-Json
$zip = Get-ChildItem -LiteralPath (Join-Path $packageDir "packages") -Filter '*.zip' -File | Select-Object -Single
$zipHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $zip.FullName).Hash.ToLowerInvariant()
$manifest | ConvertTo-Json -Depth 20
$zipHash
```

Record the full Git head, scoped provenance, executable list, ZIP path, and SHA-256 in Phase 40.

- [ ] **Step 6: Close Phase 40 documentation without rebuilding**

Update Phase 40 to `[x] (7/7 tasks)` and replace release-pending text with:

- candidate version and Git head;
- `gitDirty` and `sourceGitDirty` values;
- exact source paths;
- ZIP SHA-256;
- formal verifier evidence;
- broad release smoke evidence;
- packaged persistence smoke evidence;
- packaged Gateway smoke evidence;
- desktop auto-start result from the persistence smoke.

Update `MASTER.md` so Phase 40 is the last completed phase. Preserve all historical Phase 38 and Phase 39 provenance wording required by the parity contract.

Because these progress documents are outside the approved Loom release source paths, the candidate does not need rebuilding after this closure commit.

- [ ] **Step 7: Run final contracts and commit closure**

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-desktop-shell-contract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Loom\scripts\Test-LoomRunPersistenceSmokeContract.ps1
git status --porcelain --untracked-files=all -- Loom scripts/build-release-exes.ps1
git diff --check -- Loom docs/loom
```

Commit only the progress documents:

```powershell
git add -- docs/loom/progress/phase-40-run-event-persistence.md docs/loom/progress/MASTER.md
git commit -m "docs(loom): close durable run persistence phase"
```

Expected: all contracts pass, source scope is clean, no candidate daemon/desktop PID remains, and Phase 40 is fully auditable.
