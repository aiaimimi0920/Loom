# Loom Bounded Daemon Concurrency

## Goal

Keep the packaged Loom daemon responsive while blocking Gateway, MCP, script,
workflow, OCR, or image work is in flight. The daemon will accept multiple HTTP
requests through a bounded worker executor instead of running every route on the
single accept loop.

This phase preserves the existing synchronous route implementations and public
HTTP response shapes. It adds explicit resource limits, overload behavior, and
graceful worker shutdown without introducing automatic replay or unsafe thread
cancellation.

## Current Gap

`LoomDaemon::serve_until` currently performs the complete connection lifecycle
on one thread:

1. accept one TCP connection;
2. read and parse the HTTP request;
3. call `route`;
4. wait for all blocking work to finish;
5. write the response;
6. return to `accept`.

The route layer is synchronous by design. A Gateway-backed `brain.plan` call can
wait for its configured timeout, and MCP, script, cloud, OCR, or workflow routes
can also perform blocking work. While any one of those calls is running, the
daemon cannot accept another HTTP connection, so `/health`, `/status`, run
evidence reads, the CLI, and the desktop can appear unavailable.

Phase 40 made run and event transitions durable and releases the SQLite store
mutex while Gateway work is in flight. That storage boundary is ready for
concurrent requests, but the HTTP accept loop still serializes all callers.

## Scope

Phase 41 adds:

- a fixed-size request worker pool;
- a bounded in-memory request queue;
- deterministic overload responses;
- a cloneable daemon runtime shared by workers;
- a conservative route concurrency classification;
- per-connection error and panic containment;
- graceful shutdown that drains accepted work;
- safe executor metadata in `/status`;
- source, packaged, and release smoke coverage.

## Non-Goals

- A Tokio, Hyper, Axum, or other asynchronous HTTP rewrite.
- Thread-per-connection execution.
- Durable queued jobs across daemon restart.
- Automatic retry or replay.
- Forced cancellation of blocking threads or provider calls.
- Cancellation leases, idempotency keys, or retry policies.
- Request priority beyond the explicitly reserved probe path.
- Provider routing, Gateway credentials, quota, or health policy inside Loom.
- Changes to Gateway, Platform, Hook, Talk, or Tea implementation.

Cancellation remains a separate phase. Rust cannot safely terminate an
arbitrary blocking worker thread, and provider/tool cancellation requires a
cooperative contract at every execution boundary. A later phase can add leases
and cancellation tokens after the daemon-wide worker ownership model exists.

## Options Considered

### Thread per connection

Rejected. It would make the daemon responsive quickly, but an unbounded number
of clients could create an unbounded number of threads. It provides no queue,
backpressure, deterministic overload response, or controlled shutdown.

### Full asynchronous HTTP rewrite

Rejected for this phase. Replacing the current server with Tokio plus an async
HTTP framework would change the listener, request parser, route signatures,
shutdown behavior, tests, and the relationship with the already threaded Hook
Bridge. Most existing work is intentionally blocking, so an async rewrite would
still require `spawn_blocking` or a worker boundary.

### Bounded synchronous worker executor

Approved. It preserves the tested route implementations, introduces a clear
resource boundary, and lets the accept loop continue while blocking work runs.
It is also the smallest architecture that can support a later cancellation and
retry design without another public API migration.

## Runtime Configuration

Add an internal request executor configuration:

```rust
enum RequestExecutorConfig {
    Inline,
    Bounded {
        workers: usize,
        queue_capacity: usize,
    },
}
```

Library-level `DaemonConfig::localhost` remains `Inline` by default. This keeps
existing unit tests deterministic and avoids starting four worker threads for
every unrelated daemon fixture.

The real `loom-daemon.exe` selects `Bounded` mode from environment variables:

| Variable | Default | Valid range |
| --- | ---: | ---: |
| `LOOM_DAEMON_WORKERS` | `4` | `1..=32` |
| `LOOM_DAEMON_QUEUE_CAPACITY` | `32` | `1..=1024` |

Blank values use defaults. Non-numeric or out-of-range values fail daemon
startup with an actionable error before binding the public serving loop.

Tests can use a builder such as
`DaemonConfig::with_bounded_request_executor(workers, queue_capacity)` to prove
one-worker and saturated-queue behavior without relying on process environment.

## Status Contract

`GET /status` adds one safe object:

```json
{
  "requestExecutor": {
    "mode": "bounded_workers",
    "workers": 4,
    "queueCapacity": 32
  }
}
```

Library inline mode reports:

```json
{
  "requestExecutor": {
    "mode": "inline",
    "workers": 1,
    "queueCapacity": 0
  }
}
```

The status response does not expose thread IDs, queued request bodies, request
paths, auth headers, tokens, or client addresses. Dynamic queue metrics are not
part of this phase because they would require a separate observability contract.

## Components

### `request_executor` module

Create `Loom/apps/daemon/src/request_executor.rs` with the bounded worker
implementation and focused unit tests.

The executor owns:

- one bounded `sync_channel` sender;
- one shared receiver;
- a fixed vector of named worker threads;
- a worker callback that handles one `RequestJob`;
- explicit shutdown and join behavior.

`try_send` is required. Request submission must never block the accept loop while
waiting for queue space.

The executor returns the original job when submission fails so the accept loop
can write a structured overload response to that connection.

### `DaemonRuntime`

Move the route dependencies out of `LoomDaemon` into a shared runtime object:

```rust
struct DaemonRuntime {
    hook_settings: HookSettings,
    run_store: SharedRunStore,
    auth_token: Option<String>,
    config_registry: Arc<ConfigRegistry>,
    config_store: FileDocumentStore,
    mcp_servers: SharedMcpServerStore,
    tool_registry: ToolRegistry,
    workflow_store: WorkflowStore,
    hook_bridge: SharedHookBridgeRuntime,
    artloom_settings: SharedArtLoomCompatSettingsStore,
    shared_images: SharedImageStoreHandle,
    ocr_provider: OcrProviderHandle,
    settings_base_url: String,
    mcp_registry_endpoint: String,
    brain_planner: SharedBrainPlanner,
    run_store_status: RunStoreStatus,
    request_executor_status: RequestExecutorStatus,
    serialized_route_lock: Mutex<()>,
}
```

Workers hold `Arc<DaemonRuntime>`. The listener remains owned only by
`LoomDaemon` and is never shared with route workers.

### `RequestJob`

A request job contains:

- the accepted `TcpStream`;
- the already parsed `ParsedHttpRequest`;
- no copied auth token or secret-bearing diagnostic text.

The current request parser remains on the accept thread. Phase 41 addresses
blocking route execution, not slow-header clients. Existing two-second read
timeouts, header/body limits, and empty-probe behavior remain unchanged.

## Route Concurrency Policy

The old single-threaded server also serialized file-backed control-plane reads
and writes. Broadly parallelizing every existing route could introduce races in
configuration, tool, workflow, and compatibility file stores that currently do
not own internal locks.

Phase 41 therefore uses a conservative concurrency classification.

### Concurrent-safe routes

These routes do not hold the serialized route lock:

- `GET /health`;
- `GET /status`;
- `GET /v1/capabilities`;
- `GET /v1/runs/{runId}`;
- `GET /v1/runs/{runId}/events`;
- `POST /v1/runs`;
- run stop/retry routes backed only by `RunEvidenceStore`;
- `POST /v1/invoke` for the currently registered `brain.plan` and
  `tea.ticket.decompose.v1` capabilities.

The capability allowlist is explicit. A future capability is serialized until
its shared-state behavior is reviewed and deliberately opted into concurrent
execution.

### Serialized compatibility and control-plane routes

All remaining routes acquire `serialized_route_lock` around `route`. This keeps
the prior ordering behavior for configuration, MCP registry, tool registry,
workflow store, Hook compatibility, shared image, OCR, Python, cloud, and other
file-backed or stateful surfaces.

Serialized routes still run on worker threads. A long serialized route no
longer prevents `/health`, `/status`, run reads, or approved capability invokes
from using other workers.

This is an intentional transition boundary. Later phases may replace the single
serialized lock with per-store locks after each store has atomic read/write
contracts.

## Connection Lifecycle

For bounded mode:

1. The accept loop accepts a connection.
2. It applies the existing blocking/read-timeout configuration.
3. It reads and parses the request with existing size and syntax rules.
4. `/health` and `/status` may be handled immediately as reserved probes so they
   remain available even when the normal queue is full.
5. Other parsed requests become `RequestJob` values.
6. `try_send` submits the job to the bounded queue.
7. A worker applies the route concurrency policy, calls `route`, and writes the
   response.
8. Per-connection read, route, or write failures are logged and contained; they
   do not terminate the accept loop.

Inline mode follows the same connection handler directly without a queue. The
public behavior remains equivalent to the current library default.

## Overload Contract

When the bounded queue is full, the daemon returns HTTP 503:

```json
{
  "error": {
    "code": "daemon_busy",
    "message": "Loom daemon request queue is full",
    "retryable": true
  }
}
```

The rejected request is not executed and does not create a run or event. Loom
does not parse client-supplied retry timing and does not perform an automatic
retry.

If the executor is already shutting down, return HTTP 503 with code
`daemon_shutting_down` and `retryable = true` when a response can still be
written.

Probe requests remain outside the normal bounded queue. They continue to use
the existing authentication rules: `/health` stays public, while `/status`
retains the current bearer-token requirement for non-loopback operation.

## Panic and Error Containment

Every worker catches panics around one request job with
`catch_unwind(AssertUnwindSafe(...))`. A panicking job returns a generic HTTP
500 response when the stream remains writable:

```json
{
  "error": {
    "code": "request_worker_failed",
    "message": "Loom could not complete the request"
  }
}
```

Panic payloads and internal route errors are written only to stderr and never
included in the HTTP body. The worker loop survives and accepts the next job.

Connection-level read/write failures are also contained. Only listener-level
accept failures or executor startup/join failures may terminate
`serve_until`.

## Run Evidence Semantics

Phase 40 run evidence remains authoritative:

- each accepted capability invoke creates its run only when its worker begins
  normal route execution;
- concurrent invocations use the existing `SharedRunStore` mutex for short
  insert/transition/read transactions;
- Gateway work continues without holding the run-store mutex;
- queue rejection creates no run because no capability execution began;
- a process crash still converts durable `running` rows to
  `daemon_restarted` on the next startup;
- queued but not yet started HTTP requests are not durable and are never
  replayed.

Existing stop/retry canonicalization and event ordering are unchanged.

## Shutdown Semantics

The public `serve_until(self, Receiver<()>)` signature remains unchanged.

When shutdown is received:

1. stop accepting new connections;
2. close the bounded sender;
3. let workers finish all already accepted jobs, including queued jobs;
4. join every request worker;
5. stop existing subordinate runtimes through their current contracts;
6. return success only when workers joined cleanly.

There is no forced timeout in this phase. A blocking Gateway call remains
bounded by `LOOM_GATEWAY_TIMEOUT_SECS`; other blocking tools retain their
existing execution timeouts or process behavior. Forced cancellation belongs to
the later cancellation phase.

A daemon process killed by the OS still relies on Phase 40 startup recovery for
any run that reached `running` state.

## Test Strategy

### Executor unit tests

- submitted jobs execute on named workers;
- multiple workers execute independent blocked jobs concurrently;
- one worker plus one queued job rejects the third job deterministically;
- dropping/closing the sender drains queued jobs and joins workers;
- a panicking job does not kill its worker;
- a closed executor returns the original rejected job.

### Daemon tests

- library default status reports inline mode;
- explicit bounded config reports workers and queue capacity;
- invalid worker and queue values fail startup/config parsing;
- a blocked Gateway planner request does not block `/health` or `/status`;
- two approved capability invokes can be in flight concurrently;
- queue saturation returns `daemon_busy` and creates no run evidence;
- serialized file-backed routes do not overlap;
- worker route errors and panics do not stop the daemon;
- shutdown drains accepted jobs and leaves no request worker alive;
- existing bearer-token behavior is unchanged.

### Regression matrix

- all `loom-daemon`, `loom_durable`, and workspace tests;
- desktop typecheck, build, and Tauri check;
- desktop shell and ArtLoom parity contracts;
- Phase 40 persistence and Gateway smokes;
- no change to Hook Bridge websocket threading behavior.

## Packaged Smoke

Add `Loom/scripts/Invoke-LoomDaemonConcurrencySmoke.ps1` and a parser/contract
test.

The smoke uses an isolated control-plane root and a local mock Gateway whose
response is deliberately held. It must prove:

1. the packaged daemon reports bounded executor mode;
2. a Gateway-backed `brain.plan` request is in flight;
3. `/health` and authenticated `/status` respond before the Gateway is released;
4. a second approved capability request completes through another worker;
5. the first Gateway request completes after release;
6. both runs and ordered events are queryable;
7. all exact-path candidate processes and fixture jobs stop;
8. evidence is UTF-8 without BOM and contains no token.

Queue saturation is tested deterministically in Rust rather than by timing a
formal package with multiple external processes.

## File Map

- Create `Loom/apps/daemon/src/request_executor.rs`.
- Modify `Loom/apps/daemon/src/lib.rs` for runtime sharing, dispatch,
  concurrency classification, overload responses, status, and tests.
- Modify `Loom/apps/daemon/src/main.rs` for production environment selection.
- Modify `Loom/apps/daemon/tests/daemon_cli_contract.rs` for binary defaults and
  invalid configuration.
- Create `Loom/scripts/Invoke-LoomDaemonConcurrencySmoke.ps1`.
- Create `Loom/scripts/Test-LoomDaemonConcurrencySmokeContract.ps1`.
- Modify `Loom/README.md`, `Loom/docs/ARCHITECTURE.md`, and
  `Loom/docs/GATEWAY_INTEGRATION.md`.
- Create `docs/loom/progress/phase-41-bounded-daemon-concurrency.md`.
- Modify `docs/loom/progress/MASTER.md`.

No new Rust dependency is planned. The executor uses the standard library
threading and channel primitives already used throughout the daemon.

## Acceptance Criteria

Phase 41 is complete only when:

- the packaged binary uses bounded workers by default;
- one blocking Gateway request no longer blocks `/health`, `/status`, or a
  second approved capability request;
- queue overload is deterministic, bounded, and returns safe HTTP 503;
- file-backed compatibility routes preserve serialized behavior;
- worker panics and per-connection failures do not terminate the daemon;
- graceful shutdown drains and joins all accepted request work;
- Phase 40 SQLite recovery and canonical run history remain green;
- all source, desktop, contract, packaged, and release tests pass;
- the release artifact is written only beneath
  `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom`;
- progress documentation records the candidate, scoped provenance, checksum,
  concurrency smoke, and known non-goals.
