# Loom Run and Event Persistence

## Goal

Make Loom daemon run and event evidence survive daemon restarts without
changing the existing HTTP response shapes. This phase establishes a durable
storage boundary for capability run evidence and provides truthful recovery for
runs that were still `running` when the previous daemon stopped.

The phase does not introduce an asynchronous execution queue. It creates the
storage and recovery contract that a later queue, cancellation, and retry phase
can use without reworking the run API again.

## Current Gap

The daemon currently keeps HTTP run evidence in a process-local structure:

```rust
struct RunStore {
    runs: HashMap<String, Value>,
    events: HashMap<String, Vec<Value>>,
    next_sequence: u64,
}
```

This store is initialized by `LoomDaemon::bind`, so all run and event evidence
is lost when the process exits. The `loom_durable` crate has an asynchronous
typed `EventStore` trait and an `InMemoryEventStore`, but that boundary is used
by the workflow runtime and does not represent the HTTP capability JSON shape.

Loom already resolves a local control-plane root from
`LOOM_CONTROL_PLANE_ROOT` or the user's application data directory. Existing
MCP, tool, workflow, and settings stores use that root for local persistence.

## Decisions

### Storage technology

Use SQLite through `rusqlite` with the `bundled` feature. The database is a
single local file and does not require a separately installed database service.
SQLite provides transactions, durable ordering, crash recovery primitives, and
a clear path for a future execution queue.

JSONL and per-run JSON files were rejected because they would require Loom to
implement its own transaction, compaction, sequence, and crash-recovery rules.

### Ownership

Add a dedicated synchronous run-evidence boundary in
`Loom/crates/loom_durable/src/run_store.rs`:

- `RunEvidenceStore` is the synchronous trait consumed by the daemon.
- `RunEventDraft` contains an event kind and object fields before a sequence
  is assigned.
- `InMemoryRunEvidenceStore` preserves fast, isolated library tests.
- `SqliteRunEvidenceStore` is used by the packaged daemon.

The existing asynchronous `EventStore<LoomEvent>` remains separate in this
phase. Unifying typed workflow events and capability-specific HTTP evidence is
a later design task, not an implicit part of this migration.

### Runtime selection

`DaemonConfig::localhost` and other library constructors continue to default to
the in-memory implementation. The daemon binary explicitly selects the
persistent implementation before calling `LoomDaemon::bind`.

The persistent path is resolved in this order:

1. `LOOM_RUN_STORE_PATH` when it is non-empty.
2. `<control-plane-root>\\runs\\loom-runs.sqlite3`, where the control-plane
   root is resolved from `LOOM_CONTROL_PLANE_ROOT` or the existing default.

The daemon must fail startup when the database cannot be opened, its schema is
newer than the supported schema, or recovery cannot validate existing records.
It must never silently fall back to memory after persistent mode was selected.

### Recovery

On opening a persistent store, all records with `status = running` are
recovered in one transaction:

- update the run to `status = failed`;
- add an error with code `daemon_restarted`;
- append a `run_interrupted` event;
- preserve the original run input and any already-recorded events.

The daemon does not replay a model call or tool execution. Without leases,
idempotency keys, and a worker queue, replay could duplicate side effects.

## Architecture

The daemon owns a shared trait object:

```rust
type SharedRunStore = Arc<Mutex<Box<dyn RunEvidenceStore>>>;
```

The trait exposes operations equivalent to the current `RunStore` while making
storage failures explicit:

```rust
pub trait RunEvidenceStore: Send {
    fn insert_run(&mut self, run: Value, events: Vec<RunEventDraft>) -> RunStoreResult<()>;
    fn transition_run(&mut self, run: Value, event: RunEventDraft) -> RunStoreResult<()>;
    fn get_run(&self, run_id: &str) -> RunStoreResult<Option<Value>>;
    fn get_events(&self, run_id: &str) -> RunStoreResult<Option<Vec<Value>>>;
    fn recover_interrupted_runs(&mut self) -> RunStoreResult<usize>;
    fn status(&self) -> RunStoreStatus;
}
```

The exact Rust error and status types may use the repository's established
`thiserror` style, but they must distinguish invalid data, schema errors, IO
errors, and SQLite errors so daemon startup and request handlers can map them
correctly.

The store validates that every run has a non-empty string `id` and `status`,
and that every event draft has an object field map. It owns sequence assignment
and never accepts a caller-provided sequence as authoritative.

## SQLite Schema

The store uses `PRAGMA user_version = 1` and creates the following schema on a
new database:

```sql
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
```

The connection enables foreign keys, WAL journaling, full synchronous writes,
and a five-second busy timeout. Timestamps are UTC milliseconds generated by
Loom and are storage metadata; the public run JSON remains the compatibility
source for the API response.

`fields_json` stores only event-specific object fields. A read reconstructs the
existing response shape by adding `sequence`, `kind`, and `run_id`. This avoids
the two-step placeholder update that would otherwise be needed to embed an
autoincremented sequence inside a full event JSON blob.

Unknown future schema versions fail with an actionable startup error. Version
1 has no destructive migration path.

## Transaction Flows

### Create run

The daemon starts a run with one transaction containing:

1. Insert the canonical run JSON and its status.
2. Insert each initial event draft in order.
3. Commit both the run and events together.

The response is not returned until the transaction commits.

### Complete or fail run

The daemon first computes the planner result outside the store lock. It then
uses one transaction to:

1. Update the canonical run JSON and status.
2. Insert `capability_completed` or `capability_failed`.
3. Commit the state transition and event together.

If the transaction fails, the HTTP response is a generic `run_store_failed`
error and never claims planner success.

### Stop or retry

The request continues to require a body containing `run.id` for compatibility,
and the path/body IDs must match. The daemon then loads the stored canonical
run, applies only the requested status transition, and persists that canonical
record with a `run_action` event. Other fields supplied by the caller are
ignored rather than copied into the database.

### Read run or events

Reads query the canonical run by ID and events ordered by ascending sequence.
Unknown IDs continue to return the existing `run_not_found` response.

## Recovery Flow

Startup validation runs `PRAGMA quick_check` and parses every stored run JSON
and event field payload before the daemon begins serving HTTP requests. Stored
run IDs and statuses must match the indexed columns, and every event field
payload must be an object. Validation failure prevents startup.

Recovery then runs after validation and before the daemon begins serving HTTP
requests. It operates in one transaction so a failure cannot leave only some
stale runs recovered.

For each `running` row, the store parses the canonical JSON, replaces its
status, adds:

```json
{
  "code": "daemon_restarted",
  "message": "Run was interrupted by a daemon restart"
}
```

and appends an event with this public shape:

```json
{
  "kind": "run_interrupted",
  "status": "failed",
  "error": {
    "code": "daemon_restarted"
  }
}
```

Malformed run JSON, an absent ID/status, or an invalid event payload aborts the
transaction and prevents startup. No record is silently discarded.

## Error and Security Boundary

- Startup errors may include an operator-facing path and schema diagnostic.
- Request-time errors use `run_store_failed` and a bounded generic message.
- SQL text, filesystem paths, credentials, and raw database errors are not
  returned in public HTTP bodies.
- Gateway tokens, complete generated prompts, and raw Gateway request bodies
  are not part of the run store input and remain excluded.
- User-provided run `input` and `context` are persisted because they are already
  part of the existing run response contract. The database is local application
  data and is not encrypted at rest in this phase.
- The configured path is operator-controlled; pointing it at a shared or
  network filesystem is outside the supported local-first deployment boundary.
- A failed request-time storage operation must not terminate the daemon's
  accept loop.

## API Compatibility

The following routes and their existing JSON shapes remain unchanged:

- `POST /v1/invoke`
- `POST /v1/runs`
- `GET /v1/runs/{runId}`
- `GET /v1/runs/{runId}/events`
- `POST /v1/runs/{runId}/stop`
- `POST /v1/runs/{runId}/retry`

`GET /status` receives additive, non-secret metadata:

```json
{
  "run_store": {
    "mode": "memory" | "sqlite",
    "persistent": false | true
  }
}
```

No desktop credential ownership, run listing endpoint, retention API, or
database path is added in this phase.

## Test and Acceptance Matrix

### Durable crate

- Both implementations satisfy the same behavior tests for insert, update,
  read, event ordering, and unknown IDs.
- SQLite data remains readable after dropping and reopening the store.
- Sequence values continue after reopen.
- Duplicate run IDs and invalid event batches leave no partial run or event
  rows.
- Duplicate IDs, malformed JSON, malformed event fields, unsupported schema, and
  corrupt rows produce explicit errors.
- Recovery converts stale `running` rows and appends `run_interrupted`.

### Daemon

- A first daemon writes local success and Gateway failure evidence, exits, and a
  second daemon reads both records by their original IDs.
- The failed Gateway record remains bounded and contains no auth token.
- A forged stop/retry body cannot alter canonical run fields.
- Persistent storage errors return HTTP 500 without stopping the daemon.
- Bearer authorization behavior is unchanged across restart.
- Library tests that construct `DaemonConfig::localhost` remain isolated in
  memory.

### Release

- The packaged daemon uses SQLite by default and writes beneath the isolated
  control-plane root in smoke tests.
- A two-process packaged restart smoke proves run/event recovery.
- Existing Gateway planner, CLI, desktop sibling-daemon, OCR, MCP, workflow,
  Hook Bridge, and ArtLoom parity smokes remain green.
- A Phase 40 release candidate is written only beneath
  `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom`.
- Progress documentation records the candidate, scoped provenance, checksum,
  recovery smoke, and known non-goals.

## Non-Goals

- Asynchronous or worker-backed request execution.
- Automatic retry or replay of interrupted runs.
- Cancellation leases, idempotency keys, or queue scheduling.
- Run listing, pagination, retention, compaction, export, or encryption.
- Provider routing, quota policy, or Gateway credential management.
- Unifying the typed workflow `EventStore` with HTTP capability evidence.
- Changes to Gateway, Platform, Hook, Talk, or Tea implementation.
