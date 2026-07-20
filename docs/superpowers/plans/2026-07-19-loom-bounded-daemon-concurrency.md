# Loom Bounded Daemon Concurrency Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Keep the packaged Loom daemon responsive during blocking capability work by adding a fixed worker pool, bounded queue, safe overload behavior, conservative route serialization, and graceful shutdown.

**Architecture:** A focused `request_executor` module provides a standard-library bounded executor. `LoomDaemon` keeps sole ownership of the listener while workers share `Arc<DaemonRuntime>`; known run-store/probe routes execute concurrently, and legacy file-backed routes retain ordering behind one serialized route lock. Library fixtures remain inline by default, while the production binary selects bounded workers from validated environment configuration.

**Tech Stack:** Rust 2021, standard-library threads and `sync_channel`, existing synchronous Loom HTTP parser/router, bundled SQLite run evidence, PowerShell 5.1-compatible packaged smoke tooling.

---

## File Map

- Create `Loom/apps/daemon/src/request_executor.rs`: executor configuration,
  status, queue, worker lifecycle, submission errors, and unit tests.
- Modify `Loom/apps/daemon/src/lib.rs`: daemon config, shared runtime,
  connection handling, route classification, overload responses, status, and
  integration tests.
- Modify `Loom/apps/daemon/src/main.rs`: production bounded executor selection.
- Modify `Loom/apps/daemon/tests/daemon_cli_contract.rs`: binary defaults and
  invalid executor environment contracts.
- Create `Loom/scripts/Invoke-LoomDaemonConcurrencySmoke.ps1`: packaged slow
  Gateway concurrency smoke and UTF-8 evidence.
- Create `Loom/scripts/Test-LoomDaemonConcurrencySmokeContract.ps1`: PowerShell
  parser and literal behavior contract.
- Modify `Loom/README.md`: production executor configuration and overload
  behavior.
- Modify `Loom/docs/ARCHITECTURE.md`: listener/worker/runtime ownership and
  serialized route boundary.
- Modify `Loom/docs/GATEWAY_INTEGRATION.md`: concurrent Gateway call behavior
  and remaining cancellation boundary.
- Create `docs/loom/progress/phase-41-bounded-daemon-concurrency.md`: task and
  evidence tracker.
- Modify `docs/loom/progress/MASTER.md`: register, validate, and close Phase 41.

## Task 1: Build the Bounded Executor Contract

**Files:**
- Create: `Loom/apps/daemon/src/request_executor.rs`
- Modify: `Loom/apps/daemon/src/lib.rs`

- [ ] **Step 1: Add failing executor tests before defining the executor**

Create `request_executor.rs` with this test module and only enough imports for
the compiler to reach it:

```rust
#[cfg(test)]
mod tests {
    use std::sync::{Arc, Condvar, Mutex};

    use super::{BoundedRequestExecutor, SubmitError};

    #[test]
    fn bounded_executor_runs_jobs_on_named_workers() {
        let names = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&names);
        let mut executor = BoundedRequestExecutor::new(
            "loom-request",
            2,
            4,
            move |value: usize| {
                captured.lock().expect("lock names").push((
                    value,
                    std::thread::current()
                        .name()
                        .expect("worker name")
                        .to_owned(),
                ));
            },
        )
        .expect("create executor");

        executor.try_submit(1).expect("submit first");
        executor.try_submit(2).expect("submit second");
        executor.shutdown().expect("shutdown executor");

        let names = names.lock().expect("read names");
        assert_eq!(names.len(), 2);
        assert!(names.iter().all(|(_, name)| name.starts_with("loom-request-")));
    }

    #[test]
    fn full_queue_returns_the_original_job() {
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let entered = Arc::new((Mutex::new(false), Condvar::new()));
        let worker_gate = Arc::clone(&gate);
        let worker_entered = Arc::clone(&entered);
        let mut executor = BoundedRequestExecutor::new(
            "loom-request",
            1,
            1,
            move |_value: usize| {
                let (entered_lock, entered_signal) = &*worker_entered;
                *entered_lock.lock().expect("lock entered") = true;
                entered_signal.notify_all();
                let (gate_lock, gate_signal) = &*worker_gate;
                let mut released = gate_lock.lock().expect("lock gate");
                while !*released {
                    released = gate_signal.wait(released).expect("wait gate");
                }
            },
        )
        .expect("create executor");

        executor.try_submit(1).expect("submit active job");
        let (entered_lock, entered_signal) = &*entered;
        let mut did_enter = entered_lock.lock().expect("read entered");
        while !*did_enter {
            did_enter = entered_signal.wait(did_enter).expect("wait entered");
        }
        drop(did_enter);
        executor.try_submit(2).expect("submit queued job");
        assert!(matches!(executor.try_submit(3), Err(SubmitError::Full(3))));

        let (gate_lock, gate_signal) = &*gate;
        *gate_lock.lock().expect("release gate") = true;
        gate_signal.notify_all();
        executor.shutdown().expect("shutdown executor");
    }

    #[test]
    fn panicking_job_does_not_kill_the_worker() {
        let completed = Arc::new(Mutex::new(Vec::new()));
        let worker_completed = Arc::clone(&completed);
        let mut executor = BoundedRequestExecutor::new(
            "loom-request",
            1,
            2,
            move |value: usize| {
                if value == 1 {
                    panic!("fixture panic");
                }
                worker_completed.lock().expect("lock completed").push(value);
            },
        )
        .expect("create executor");

        executor.try_submit(1).expect("submit panic");
        executor.try_submit(2).expect("submit recovery");
        executor.shutdown().expect("shutdown executor");
        assert_eq!(*completed.lock().expect("read completed"), vec![2]);
    }

    #[test]
    fn closed_executor_returns_the_original_job() {
        let mut executor = BoundedRequestExecutor::new(
            "loom-request",
            1,
            1,
            |_value: usize| {},
        )
        .expect("create executor");
        executor.close();
        assert!(matches!(executor.try_submit(7), Err(SubmitError::Closed(7))));
        executor.shutdown().expect("join executor");
    }
}
```

Add `mod request_executor;` to `lib.rs`. Do not define the imported types yet.

- [ ] **Step 2: Run the executor tests and verify RED**

```powershell
$env:CARGO_TARGET_DIR = "C:\t\loom-daemon-concurrency"
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon request_executor --lib
```

Expected: compilation fails because `BoundedRequestExecutor` and `SubmitError`
are undefined.

- [ ] **Step 3: Implement the minimal generic bounded executor**

Define:

```rust
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum SubmitError<T> {
    Full(T),
    Closed(T),
}

pub(crate) struct BoundedRequestExecutor<T: Send + 'static> {
    sender: Option<SyncSender<T>>,
    workers: Vec<JoinHandle<()>>,
}

impl<T: Send + 'static> BoundedRequestExecutor<T> {
    pub(crate) fn new<F>(
        thread_prefix: &str,
        workers: usize,
        queue_capacity: usize,
        handler: F,
    ) -> std::io::Result<Self>
    where
        F: Fn(T) + Send + Sync + 'static,
    {
        let (sender, receiver) = mpsc::sync_channel(queue_capacity);
        let receiver = Arc::new(Mutex::new(receiver));
        let handler = Arc::new(handler);
        let mut worker_handles = Vec::with_capacity(workers);
        for index in 0..workers {
            let receiver = Arc::clone(&receiver);
            let handler = Arc::clone(&handler);
            worker_handles.push(
                thread::Builder::new()
                    .name(format!("{thread_prefix}-{index}"))
                    .spawn(move || worker_loop(receiver, handler))?,
            );
        }
        Ok(Self {
            sender: Some(sender),
            workers: worker_handles,
        })
    }

    pub(crate) fn try_submit(&self, job: T) -> Result<(), SubmitError<T>> {
        let Some(sender) = self.sender.as_ref() else {
            return Err(SubmitError::Closed(job));
        };
        sender.try_send(job).map_err(|error| match error {
            TrySendError::Full(job) => SubmitError::Full(job),
            TrySendError::Disconnected(job) => SubmitError::Closed(job),
        })
    }

    pub(crate) fn close(&mut self) {
        self.sender.take();
    }

    pub(crate) fn shutdown(&mut self) -> std::io::Result<()> {
        self.close();
        for worker in self.workers.drain(..) {
            worker.join().map_err(|_| {
                std::io::Error::other("Loom request worker terminated unexpectedly")
            })?;
        }
        Ok(())
    }
}

fn worker_loop<T, F>(receiver: Arc<Mutex<Receiver<T>>>, handler: Arc<F>)
where
    T: Send + 'static,
    F: Fn(T) + Send + Sync + 'static,
{
    loop {
        let job = {
            let receiver = match receiver.lock() {
                Ok(receiver) => receiver,
                Err(_) => return,
            };
            match receiver.recv() {
                Ok(job) => job,
                Err(_) => return,
            }
        };
        let _ = catch_unwind(AssertUnwindSafe(|| handler(job)));
    }
}
```

Add a best-effort `Drop` implementation that closes the sender and joins
remaining workers without panicking.

- [ ] **Step 4: Run executor tests and formatting**

```powershell
$env:CARGO_TARGET_DIR = "C:\t\loom-daemon-concurrency"
cargo fmt --manifest-path Loom/Cargo.toml --all -- --check
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon request_executor --lib
```

Expected: four executor tests pass.

- [ ] **Step 5: Commit the executor primitive**

```powershell
git add -- Loom/apps/daemon/src/request_executor.rs Loom/apps/daemon/src/lib.rs
git commit -m "feat(loom): add bounded request executor"
```

## Task 2: Add Executor Configuration and Safe Status

**Files:**
- Modify: `Loom/apps/daemon/src/request_executor.rs`
- Modify: `Loom/apps/daemon/src/lib.rs`

- [ ] **Step 1: Add failing configuration and status tests**

Add module tests for exact defaults and validation:

```rust
#[test]
fn production_defaults_are_bounded_and_stable() {
    assert_eq!(
        RequestExecutorConfig::production_default(),
        RequestExecutorConfig::Bounded {
            workers: 4,
            queue_capacity: 32,
        }
    );
}

#[test]
fn executor_config_rejects_invalid_ranges() {
    assert!(RequestExecutorConfig::bounded(0, 32).is_err());
    assert!(RequestExecutorConfig::bounded(33, 32).is_err());
    assert!(RequestExecutorConfig::bounded(4, 0).is_err());
    assert!(RequestExecutorConfig::bounded(4, 1025).is_err());
}
```

Add daemon tests:

```rust
#[test]
fn daemon_reports_inline_request_executor_by_default() {
    let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
    let port = daemon.local_addr().expect("address").port();
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let server = thread::spawn(move || daemon.serve_until(shutdown_rx));

    let status = http_json_get(port, "/status");
    assert_eq!(status["requestExecutor"]["mode"], "inline");
    assert_eq!(status["requestExecutor"]["workers"], 1);
    assert_eq!(status["requestExecutor"]["queueCapacity"], 0);

    shutdown_tx.send(()).expect("shutdown");
    server.join().expect("server").expect("serve");
}

#[test]
fn daemon_reports_explicit_bounded_request_executor() {
    let daemon = LoomDaemon::bind(
        DaemonConfig::localhost(0).with_bounded_request_executor(2, 3),
    )
    .expect("bind daemon");
    let port = daemon.local_addr().expect("address").port();
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let server = thread::spawn(move || daemon.serve_until(shutdown_rx));

    let status = http_json_get(port, "/status");
    assert_eq!(status["requestExecutor"]["mode"], "bounded_workers");
    assert_eq!(status["requestExecutor"]["workers"], 2);
    assert_eq!(status["requestExecutor"]["queueCapacity"], 3);

    shutdown_tx.send(()).expect("shutdown");
    server.join().expect("server").expect("serve");
}
```

- [ ] **Step 2: Run targeted tests and verify RED**

```powershell
$env:CARGO_TARGET_DIR = "C:\t\loom-daemon-concurrency"
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon request_executor --lib
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon daemon_reports_inline_request_executor_by_default --lib
```

Expected: missing config, builder, and status types.

- [ ] **Step 3: Implement exact config and status contracts**

Add to `request_executor.rs`:

```rust
use serde::Serialize;
use thiserror::Error;

pub(crate) const DEFAULT_REQUEST_WORKERS: usize = 4;
pub(crate) const DEFAULT_REQUEST_QUEUE_CAPACITY: usize = 32;
const MAX_REQUEST_WORKERS: usize = 32;
const MAX_REQUEST_QUEUE_CAPACITY: usize = 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RequestExecutorConfig {
    Inline,
    Bounded {
        workers: usize,
        queue_capacity: usize,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RequestExecutorStatus {
    pub mode: &'static str,
    pub workers: usize,
    pub queue_capacity: usize,
}

#[derive(Debug, Error)]
pub(crate) enum RequestExecutorConfigError {
    #[error("LOOM_DAEMON_WORKERS must be between 1 and 32, got {0}")]
    InvalidWorkers(usize),
    #[error("LOOM_DAEMON_QUEUE_CAPACITY must be between 1 and 1024, got {0}")]
    InvalidQueueCapacity(usize),
    #[error("{name} must be an unsigned integer, got `{value}`")]
    InvalidEnvironment { name: &'static str, value: String },
}
```

Implement `Inline`, `production_default`, `bounded`, `from_env`, and `status`.
`from_env` reads `LOOM_DAEMON_WORKERS` and
`LOOM_DAEMON_QUEUE_CAPACITY`, treats blank values as defaults, and validates
before returning.

Add `request_executor: RequestExecutorConfig` to `DaemonConfig`, default it to
`Inline`, add `with_bounded_request_executor`, and add
`with_request_executor_from_env(self) -> Result<Self>`.

Add `request_executor: RequestExecutorStatus` to `StatusResponse` using the
existing camelCase response shape.

- [ ] **Step 4: Run config/status tests**

```powershell
$env:CARGO_TARGET_DIR = "C:\t\loom-daemon-concurrency"
cargo fmt --manifest-path Loom/Cargo.toml --all -- --check
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon request_executor --lib
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon daemon_reports_inline_request_executor_by_default --lib
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon daemon_reports_explicit_bounded_request_executor --lib
```

Expected: config tests and both status tests pass; bounded mode may still execute
inline until Task 4.

- [ ] **Step 5: Commit configuration and status**

```powershell
git add -- Loom/apps/daemon/src/request_executor.rs Loom/apps/daemon/src/lib.rs
git commit -m "feat(loom): configure daemon request executor"
```

## Task 3: Extract a Shared Runtime Without Changing Inline Behavior

**Files:**
- Modify: `Loom/apps/daemon/src/lib.rs`

- [ ] **Step 1: Add a failing runtime ownership test**

Add a test-only assertion that explicit bounded configuration can bind and serve
multiple sequential routes while the listener remains owned by `LoomDaemon`:

```rust
#[test]
fn daemon_runtime_remains_available_across_sequential_routes() {
    let daemon = LoomDaemon::bind(
        DaemonConfig::localhost(0).with_bounded_request_executor(2, 4),
    )
    .expect("bind daemon");
    let port = daemon.local_addr().expect("address").port();
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let server = thread::spawn(move || daemon.serve_until(shutdown_rx));

    assert_eq!(http_json_get(port, "/health")["status"], "ok");
    assert_eq!(http_json_get(port, "/status")["status"], "ready");
    assert!(http_json_get(port, "/v1/capabilities")["capabilities"].is_array());

    shutdown_tx.send(()).expect("shutdown");
    server.join().expect("server").expect("serve");
}
```

- [ ] **Step 2: Run the test before refactoring**

```powershell
$env:CARGO_TARGET_DIR = "C:\t\loom-daemon-concurrency"
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon daemon_runtime_remains_available_across_sequential_routes --lib
```

Expected: the test may pass behaviorally; retain it as a characterization test
before moving fields. Do not change its assertions during the refactor.

- [ ] **Step 3: Introduce `DaemonRuntime` and one contained connection handler**

Create:

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

pub struct LoomDaemon {
    listener: TcpListener,
    runtime: Arc<DaemonRuntime>,
    request_executor: RequestExecutorConfig,
}
```

Move field construction in `bind` into `DaemonRuntime`. Use
`Arc::new(built_in_registry())` because `ConfigRegistry` is not cloneable.

Add:

```rust
fn route_with_runtime(
    runtime: &DaemonRuntime,
    request: &ParsedHttpRequest,
) -> Result<(u16, String)> {
    route(
        request,
        &runtime.hook_settings,
        &runtime.run_store,
        runtime.run_store_status,
        &runtime.brain_planner,
        runtime.auth_token.as_deref(),
        runtime.config_registry.as_ref(),
        &runtime.config_store,
        &runtime.mcp_servers,
        &runtime.tool_registry,
        &runtime.workflow_store,
        &runtime.hook_bridge,
        &runtime.artloom_settings,
        &runtime.shared_images,
        &runtime.ocr_provider,
        &runtime.settings_base_url,
        &runtime.mcp_registry_endpoint,
        runtime.request_executor_status,
    )
}
```

Add one inline connection helper that contains request-level errors instead of
using `?` through the accept loop:

```rust
fn handle_parsed_request(
    mut stream: TcpStream,
    request: ParsedHttpRequest,
    runtime: &DaemonRuntime,
) {
    let response = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        route_with_runtime(runtime, &request)
    }));
    let (status, body) = match response {
        Ok(Ok(response)) => response,
        Ok(Err(error)) => {
            eprintln!("loom request routing failed: {error:#}");
            request_worker_failed_response()
        }
        Err(_) => {
            eprintln!("loom request worker panicked");
            request_worker_failed_response()
        }
    };
    if let Err(error) = write_response(&mut stream, status, &body) {
        eprintln!("loom response write failed: {error:#}");
    }
}
```

Keep `serve_until` executing this helper inline in Task 3. Do not create worker
threads yet.

- [ ] **Step 4: Run the full daemon library tests after the structural refactor**

```powershell
$env:CARGO_TARGET_DIR = "C:\t\loom-daemon-concurrency"
cargo fmt --manifest-path Loom/Cargo.toml --all -- --check
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon --lib
```

Expected: all current daemon library tests plus new characterization/status
tests pass with unchanged HTTP bodies.

- [ ] **Step 5: Commit the runtime extraction**

```powershell
git add -- Loom/apps/daemon/src/lib.rs
git commit -m "refactor(loom): share daemon route runtime"
```

## Task 4: Dispatch Through Bounded Workers and Preserve Route Ordering

**Files:**
- Modify: `Loom/apps/daemon/src/lib.rs`
- Modify: `Loom/apps/daemon/src/request_executor.rs`

- [ ] **Step 1: Add failing slow-planner responsiveness test**

Inside daemon tests define a blocking planner using two condition variables:

```rust
struct BlockingBrainPlanner {
    entered: Arc<(Mutex<bool>, Condvar)>,
    release: Arc<(Mutex<bool>, Condvar)>,
}

impl BrainPlanner for BlockingBrainPlanner {
    fn plan(&self, _request: BrainPlanRequest) -> Result<BrainPlanResult, BrainPlannerError> {
        let (entered_lock, entered_signal) = &*self.entered;
        *entered_lock.lock().expect("enter planner") = true;
        entered_signal.notify_all();
        let (release_lock, release_signal) = &*self.release;
        let mut released = release_lock.lock().expect("lock release");
        while !*released {
            released = release_signal.wait(released).expect("wait release");
        }
        Ok(BrainPlanResult {
            summary: "concurrent plan".to_owned(),
            steps: vec!["complete".to_owned()],
            source: BrainPlanSource::Gateway,
            model: Some("fixture-model".to_owned()),
        })
    }

    fn status(&self) -> BrainPlannerStatus {
        BrainPlannerStatus {
            mode: "gateway",
            configured: true,
            model: Some("fixture-model".to_owned()),
            timeout_seconds: Some(30),
        }
    }
}
```

Add `daemon_serves_probes_while_brain_plan_is_blocked`:

1. bind with two workers and queue capacity four;
2. start `POST /v1/invoke` on a client thread;
3. wait until the blocking planner entered;
4. call `/health` and `/status` from the main test thread;
5. assert both return before releasing the planner;
6. release and assert the invoke succeeds.

- [ ] **Step 2: Add failing deterministic queue saturation test**

Bind one worker with queue capacity one and the blocking planner. Start one
active invoke, submit a second invoke so it occupies the queue, then send a
third invoke and assert:

```rust
assert!(response.starts_with("HTTP/1.1 503 Service Unavailable"));
let body = response_json_body(&response);
assert_eq!(body["error"]["code"], "daemon_busy");
assert_eq!(body["error"]["retryable"], true);
```

After releasing the planner, assert the rejected request ID does not appear in
any run response and that the daemon still answers `/health`.

- [ ] **Step 3: Add failing route classification tests**

Define `RequestConcurrencyClass::{Concurrent, Serialized}` and table-driven
tests requiring:

```rust
let concurrent = [
    ("GET", "/health", None),
    ("GET", "/status", None),
    ("GET", "/v1/capabilities", None),
    ("GET", "/v1/runs/run-1", None),
    ("GET", "/v1/runs/run-1/events", None),
    ("POST", "/v1/invoke", Some("brain.plan")),
    ("POST", "/v1/invoke", Some("tea.ticket.decompose.v1")),
];
let serialized = [
    ("GET", "/v1/workflows", None),
    ("PUT", "/v1/workflows/workflow-1", None),
    ("POST", "/v1/tools/tool-1/execute", None),
    ("POST", "/v1/invoke", Some("future.capability")),
];
```

The invoke classifier parses only the existing `capability` string from the
already bounded JSON body. Invalid bodies remain serialized and then follow the
existing bad-request route behavior.

- [ ] **Step 4: Run the new tests and verify RED**

```powershell
$env:CARGO_TARGET_DIR = "C:\t\loom-daemon-concurrency"
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon daemon_serves_probes_while_brain_plan_is_blocked --lib
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon daemon_returns_busy_when_request_queue_is_full --lib
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon request_concurrency_classification_is_conservative --lib
```

Expected: probes block behind the first invoke, no 503 overload path exists, and
classification is undefined.

- [ ] **Step 5: Implement request jobs, classification, and overload responses**

Add:

```rust
struct RequestJob {
    stream: TcpStream,
    request: ParsedHttpRequest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RequestConcurrencyClass {
    Concurrent,
    Serialized,
}

fn request_concurrency_class(request: &ParsedHttpRequest) -> RequestConcurrencyClass {
    // Match only the explicit approved routes from the design.
}
```

Before calling `route_with_runtime`, acquire
`runtime.serialized_route_lock` only for `Serialized` requests. Never hold this
lock while handling `/health`, `/status`, run-store routes, or approved invoke
capabilities.

Add exact responses:

```rust
fn daemon_busy_response() -> (u16, String) {
    structured_error(
        503,
        json!({
            "code": "daemon_busy",
            "message": "Loom daemon request queue is full",
            "retryable": true,
        }),
    )
    .expect("serialize daemon busy response")
}

fn daemon_shutting_down_response() -> (u16, String) {
    structured_error(
        503,
        json!({
            "code": "daemon_shutting_down",
            "message": "Loom daemon is shutting down",
            "retryable": true,
        }),
    )
    .expect("serialize daemon shutdown response")
}
```

Add `503 => "Service Unavailable"` to `write_response`.

- [ ] **Step 6: Implement bounded serving and reserved probes**

For `RequestExecutorConfig::Bounded`, create the executor once before the
accept loop:

```rust
let runtime = Arc::clone(&self.runtime);
let mut executor = BoundedRequestExecutor::new(
    "loom-request",
    workers,
    queue_capacity,
    move |job: RequestJob| handle_request_job(job, &runtime),
)?;
```

After parsing a request:

- process `GET /health` and `GET /status` immediately through the same contained
  handler so they bypass a full queue;
- call `try_submit` for every other request;
- on `Full(job)`, write `daemon_busy` to `job.stream`;
- on `Closed(job)`, write `daemon_shutting_down` to `job.stream`.

When shutdown arrives, stop accepting, call `executor.shutdown()`, and return.
Inline mode continues to call `handle_request_job` directly.

Contain stream setup, request read, and response write failures per connection.
They must log and continue instead of escaping through `serve_until`.

- [ ] **Step 7: Add serialized-route overlap proof**

Add a test-only route execution observer around the serialized lock. Start two
file-backed route requests with a fixture barrier and assert the maximum
serialized-route active count is exactly one, while a simultaneous `/health`
request still returns.

Do not add production queue metrics for this test; keep the observer under
`#[cfg(test)]`.

- [ ] **Step 8: Run all daemon tests and clippy for the affected crate**

```powershell
$env:CARGO_TARGET_DIR = "C:\t\loom-daemon-concurrency"
cargo fmt --manifest-path Loom/Cargo.toml --all -- --check
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon --locked
cargo clippy --manifest-path Loom/Cargo.toml -p loom-daemon --all-targets --locked -- -D warnings
```

If clippy reaches the existing `loom_core` `double_must_use` warning before the
daemon crate, rerun the affected crate check with
`-A clippy::double_must_use -D warnings` and record the pre-existing warning
separately. Do not modify unrelated `loom_core` APIs in this phase.

- [ ] **Step 9: Commit bounded serving**

```powershell
git add -- Loom/apps/daemon/src/lib.rs Loom/apps/daemon/src/request_executor.rs
git commit -m "feat(loom): serve requests through bounded workers"
```

## Task 5: Enable Production Defaults and Binary Contracts

**Files:**
- Modify: `Loom/apps/daemon/src/main.rs`
- Modify: `Loom/apps/daemon/src/lib.rs`
- Modify: `Loom/apps/daemon/tests/daemon_cli_contract.rs`

- [ ] **Step 1: Add failing binary default and invalid-env tests**

Add a small `http_json_get(base_url, path)` helper to the CLI contract using
`TcpStream` and the manifest's loopback base URL.

Add:

```rust
#[test]
fn daemon_binary_uses_bounded_request_executor_by_default() {
    let temp_dir = unique_temp_dir("bounded-executor-default");
    let manifest_dir = temp_dir.join("capabilities");
    let exe = env!("CARGO_BIN_EXE_loom-daemon");
    let child = Command::new(exe)
        .env("LOOM_DAEMON_HOST", "127.0.0.1")
        .env("LOOM_DAEMON_PORT", "0")
        .env("LOOM_CAPABILITY_MANIFEST_DIR", &manifest_dir)
        .env("LOOM_CONTROL_PLANE_ROOT", temp_dir.join("control-plane"))
        .env_remove("LOOM_DAEMON_WORKERS")
        .env_remove("LOOM_DAEMON_QUEUE_CAPACITY")
        .spawn()
        .expect("spawn daemon");

    let manifest = wait_for_manifest(&manifest_dir.join("loom.json"));
    let status = http_json_get(
        manifest["transport"]["baseUrl"].as_str().expect("base url"),
        "/status",
    );
    stop_child(child);
    assert_eq!(status["requestExecutor"]["mode"], "bounded_workers");
    assert_eq!(status["requestExecutor"]["workers"], 4);
    assert_eq!(status["requestExecutor"]["queueCapacity"], 32);
}
```

Add table-driven process tests for `LOOM_DAEMON_WORKERS=0`, `33`, `bad` and
queue capacity `0`, `1025`, `bad`. Each process must exit nonzero and stderr
must name the invalid environment variable without starting a listener.

- [ ] **Step 2: Run binary tests and verify RED**

```powershell
$env:CARGO_TARGET_DIR = "C:\t\loom-daemon-concurrency"
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon daemon_binary_uses_bounded_request_executor_by_default --test daemon_cli_contract
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon daemon_binary_rejects_invalid_request_executor_environment --test daemon_cli_contract
```

Expected: production binary still reports inline mode and does not reject the
new environment variables.

- [ ] **Step 3: Select bounded configuration in `main.rs`**

Change construction to:

```rust
let mut config = DaemonConfig::bind_host(host, port)
    .with_brain_planner_from_env()?
    .with_request_executor_from_env()?
    .with_sqlite_run_store(default_run_store_path());
```

Update `daemon_help_text` with both environment variables and defaults.

- [ ] **Step 4: Isolate all existing CLI contract tests from parent env**

Every child daemon fixture must either remove
`LOOM_DAEMON_WORKERS`/`LOOM_DAEMON_QUEUE_CAPACITY` or set deliberate values.
This prevents the developer shell from changing contract behavior.

- [ ] **Step 5: Run the complete binary contract**

```powershell
$env:CARGO_TARGET_DIR = "C:\t\loom-daemon-concurrency"
cargo fmt --manifest-path Loom/Cargo.toml --all -- --check
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon --test daemon_cli_contract --locked
```

Expected: existing manifest/Gateway/SQLite tests and new executor tests pass.

- [ ] **Step 6: Commit production selection**

```powershell
git add -- Loom/apps/daemon/src/main.rs Loom/apps/daemon/src/lib.rs Loom/apps/daemon/tests/daemon_cli_contract.rs
git commit -m "feat(loom): enable bounded daemon workers"
```

## Task 6: Add the Packaged Concurrency Smoke

**Files:**
- Create: `Loom/scripts/Test-LoomDaemonConcurrencySmokeContract.ps1`
- Create: `Loom/scripts/Invoke-LoomDaemonConcurrencySmoke.ps1`

- [ ] **Step 1: Write the failing PowerShell parser contract**

The contract parses the smoke with
`System.Management.Automation.Language.Parser` and asserts these literals:

```powershell
Assert-Contains '[string]$PackageDir' $raw "Smoke must accept a package directory."
Assert-Contains 'LOOM_DAEMON_WORKERS' $raw "Smoke must configure bounded workers."
Assert-Contains 'LOOM_DAEMON_QUEUE_CAPACITY' $raw "Smoke must configure queue capacity."
Assert-Contains 'requestExecutor' $raw "Smoke must inspect executor status."
Assert-Contains 'gatewayRequestEntered' $raw "Smoke must prove the Gateway call entered."
Assert-Contains 'probeCompletedBeforeGatewayRelease' $raw "Smoke must prove probe responsiveness."
Assert-Contains 'secondCapabilityCompletedBeforeGatewayRelease' $raw "Smoke must prove another capability can finish."
Assert-Contains 'candidateProcessesAfterCleanup' $raw "Smoke must prove cleanup."
Assert-Contains 'UTF8Encoding' $raw "Smoke evidence must be UTF-8 without BOM."
Assert-Contains 'ExecutablePath' $raw "Cleanup must use exact executable paths."
```

Print `Loom daemon concurrency smoke contract passed.` on success.

- [ ] **Step 2: Run the contract and verify RED**

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Loom\scripts\Test-LoomDaemonConcurrencySmokeContract.ps1
```

Expected: failure because the smoke script does not exist.

- [ ] **Step 3: Implement the packaged smoke**

The script accepts:

```powershell
param(
    [Parameter(Mandatory = $true)]
    [string]$PackageDir,
    [string]$EvidenceRoot = ""
)
```

Use an isolated `LOOM_CONTROL_PLANE_ROOT`, `APPDATA`, and `LOCALAPPDATA`.
Start a local `HttpListener` or TCP fixture job that:

- accepts `POST /v1/chat/completions`;
- records a redacted request;
- sets a thread-safe entered signal;
- waits for a release signal;
- returns a strict two-step JSON planner result.

Start packaged `loom-daemon.exe` with:

```powershell
LOOM_DAEMON_WORKERS = "2"
LOOM_DAEMON_QUEUE_CAPACITY = "4"
LOOM_GATEWAY_MODEL = "concurrency-smoke"
LOOM_GATEWAY_BASE_URL = $gatewayBaseUrl
LOOM_GATEWAY_TOKEN = "loom-concurrency-smoke-token"
```

Launch the first `brain.plan` request in a background PowerShell job. Wait until
the Gateway fixture records entry. Before releasing it:

- call `/health` and `/status` with strict short timeouts;
- assert status reports `bounded_workers`, workers `2`, queue capacity `4`;
- invoke `tea.ticket.decompose.v1` and require success;
- record that both operations completed before Gateway release.

Release the Gateway fixture, wait for the first invoke, query both run/event
records, and require ordered `run_started,capability_completed` events.

The final summary contains:

```powershell
$summary = [ordered]@{
    schemaVersion = 1
    status = "passed"
    packageDir = $packageFullPath
    requestExecutorMode = [string]$status.requestExecutor.mode
    workers = [int]$status.requestExecutor.workers
    queueCapacity = [int]$status.requestExecutor.queueCapacity
    gatewayRequestEntered = $gatewayRequestEntered
    probeCompletedBeforeGatewayRelease = $probeCompletedBeforeGatewayRelease
    secondCapabilityCompletedBeforeGatewayRelease = $secondCapabilityCompletedBeforeGatewayRelease
    gatewayRunStatus = [string]$gatewayRun.status
    gatewayEventKinds = @($gatewayEvents.events | ForEach-Object { [string]$_.kind })
    secondRunStatus = [string]$secondRun.status
    candidateProcessesAfterCleanup = $candidateProcessesAfterCleanup
    daemonStopped = $daemonStopped
    gatewayJobStopped = $gatewayJobStopped
    evidenceDir = $evidenceRunDir
}
```

Follow the Phase 40 smoke pattern: exact `ExecutablePath` snapshots, exact PID
cleanup, failed summary in `finally`, redacted logs, UTF-8 without BOM, and no
cleanup of pre-existing candidate processes.

- [ ] **Step 4: Build debug binaries and run the contract/smoke**

```powershell
$env:CARGO_TARGET_DIR = "C:\t\loom-daemon-concurrency"
cargo build --manifest-path Loom/Cargo.toml -p loom-daemon -p loom-cli --locked
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Loom\scripts\Test-LoomDaemonConcurrencySmokeContract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Loom\scripts\Invoke-LoomDaemonConcurrencySmoke.ps1 -PackageDir "C:\t\loom-daemon-concurrency\debug"
```

Expected: the smoke proves both probe responsiveness and a second completed
capability before Gateway release, then cleans all processes/jobs.

- [ ] **Step 5: Commit packaged concurrency evidence tooling**

```powershell
git add -- Loom/scripts/Test-LoomDaemonConcurrencySmokeContract.ps1 Loom/scripts/Invoke-LoomDaemonConcurrencySmoke.ps1
git commit -m "test(loom): smoke bounded daemon concurrency"
```

## Task 7: Document and Register Phase 41

**Files:**
- Modify: `Loom/README.md`
- Modify: `Loom/docs/ARCHITECTURE.md`
- Modify: `Loom/docs/GATEWAY_INTEGRATION.md`
- Create: `docs/loom/progress/phase-41-bounded-daemon-concurrency.md`
- Modify: `docs/loom/progress/MASTER.md`

- [ ] **Step 1: Update runtime documentation**

Document:

- production defaults and environment variables;
- inline library default;
- `requestExecutor` status shape;
- HTTP 503 `daemon_busy` and no-run semantics;
- concurrent-safe route allowlist and serialized legacy route boundary;
- graceful drain without forced cancellation;
- Gateway calls no longer blocking probes when worker capacity exists.

Replace the README/architecture wording that says the HTTP server remains
synchronous while a Gateway call is running. Preserve the explicit statement
that provider routing remains outside Loom.

- [ ] **Step 2: Create Phase 41 progress tracking at 5/7 tasks**

Use:

```markdown
# Phase 41: Bounded Daemon Concurrency

## Goal

Keep Loom health, status, run evidence, and approved capabilities responsive
while blocking work executes, with bounded resource usage and no automatic
replay.

## Tasks

- [x] P41.1 Implement the bounded request executor.
- [x] P41.2 Add executor configuration and safe status metadata.
- [x] P41.3 Extract shared daemon route runtime.
- [x] P41.4 Dispatch approved routes through bounded workers.
- [x] P41.5 Add packaged concurrency smoke tooling.
- [ ] P41.6 Complete full source and desktop validation.
- [ ] P41.7 Generate and verify the formal release candidate.
```

Register Phase 41 as active in `MASTER.md` without changing Phase 40 release
provenance or the parity-required Phase 38 wording.

- [ ] **Step 3: Run documentation-bearing contracts**

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-desktop-shell-contract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Loom\scripts\Test-LoomRunPersistenceSmokeContract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Loom\scripts\Test-LoomDaemonConcurrencySmokeContract.ps1
git diff --check -- Loom docs/loom
```

- [ ] **Step 4: Commit implementation documentation**

```powershell
git add -- Loom/README.md Loom/docs/ARCHITECTURE.md Loom/docs/GATEWAY_INTEGRATION.md docs/loom/progress/phase-41-bounded-daemon-concurrency.md docs/loom/progress/MASTER.md
git commit -m "docs(loom): document bounded daemon concurrency"
```

## Task 8: Validate, Release, and Close Phase 41

**Files:**
- Verify source and package outputs.
- Modify after validation/release:
  `docs/loom/progress/phase-41-bounded-daemon-concurrency.md`
- Modify after validation/release: `docs/loom/progress/MASTER.md`

- [ ] **Step 1: Run the full source matrix**

```powershell
$env:CARGO_TARGET_DIR = "C:\t\loom-daemon-concurrency"
cargo fmt --manifest-path Loom/Cargo.toml --all -- --check
cargo check --manifest-path Loom/Cargo.toml --workspace --all-targets --locked
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon --locked
cargo test --manifest-path Loom/Cargo.toml -p loom_durable --locked
cargo test --manifest-path Loom/Cargo.toml -p loom-cli --locked
cargo test --manifest-path Loom/Cargo.toml --workspace --locked
```

Run desktop validation:

```powershell
Push-Location .\Loom\apps\desktop
try {
    npm run typecheck
    npm run build
}
finally {
    Pop-Location
}

$env:CARGO_TARGET_DIR = "C:\t\loom-daemon-concurrency-tauri"
cargo check --manifest-path .\Loom\apps\desktop\src-tauri\Cargo.toml --locked
```

Run all four PowerShell contracts and debug concurrency, persistence, and
Gateway smokes. Record exact test counts and evidence paths.

- [ ] **Step 2: Mark Phase 41 source validation complete at 6/7**

Update only the two progress documents and commit:

```powershell
git add -- docs/loom/progress/phase-41-bounded-daemon-concurrency.md docs/loom/progress/MASTER.md
git commit -m "docs(loom): record daemon concurrency validation"
```

- [ ] **Step 3: Confirm the approved release source scope is clean**

```powershell
git status --porcelain --untracked-files=all -- Loom scripts/build-release-exes.ps1
```

Expected: no output. Do not build while this scope is dirty.

- [ ] **Step 4: Build only Loom beneath the required release root**

```powershell
$shortSha = (git rev-parse --short=8 HEAD).Trim()
$versionId = "$(Get-Date -Format 'yyyyMMdd-HHmmss')-$shortSha"
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\build-release-exes.ps1 `
  -VersionId $versionId `
  -Apps Loom
```

Output must remain under:

```text
C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\<versionId>
```

- [ ] **Step 5: Run the formal release matrix**

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-release.ps1 -VersionId $versionId -Apps Loom
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\smoke-release-local-apps.ps1 -VersionId $versionId -Apps Loom
$packageDir = Join-Path (Resolve-Path .\release\Loom).Path $versionId
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Loom\scripts\Invoke-LoomRunPersistenceSmoke.ps1 -PackageDir $packageDir
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Loom\scripts\Invoke-LoomGatewayBrainPlanSmoke.ps1 -PackageDir $packageDir
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Loom\scripts\Invoke-LoomDaemonConcurrencySmoke.ps1 -PackageDir $packageDir
```

Expected concurrency evidence:

```text
status = passed
requestExecutorMode = bounded_workers
workers = 2
queueCapacity = 4
gatewayRequestEntered = true
probeCompletedBeforeGatewayRelease = true
secondCapabilityCompletedBeforeGatewayRelease = true
candidateProcessesAfterCleanup = []
daemonStopped = true
gatewayJobStopped = true
```

- [ ] **Step 6: Verify package identity and ZIP checksum**

Read `manifest.json`, require `sourceGitDirty = false`, exact source paths
`Loom` plus `scripts/build-release-exes.ps1`, and packaged `loom.exe`,
`loom-daemon.exe`, `loom-desktop.exe`. Compute the ZIP SHA-256 independently.

- [ ] **Step 7: Close Phase 41 without rebuilding**

Update Phase 41 to 7/7 with candidate version, full release Git head,
repository `gitDirty`, `sourceGitDirty`, exact source paths, ZIP hash, formal
verifier, unified smoke, persistence smoke, Gateway smoke, and concurrency
smoke evidence. Make Phase 41 the last completed phase in `MASTER.md` while
preserving all historical Phase 38 through Phase 40 wording.

Progress documents are outside the approved release source paths, so do not
rebuild after this closure-only commit.

- [ ] **Step 8: Run final gates and commit closure**

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-desktop-shell-contract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Loom\scripts\Test-LoomRunPersistenceSmokeContract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Loom\scripts\Test-LoomDaemonConcurrencySmokeContract.ps1
git status --porcelain --untracked-files=all -- Loom scripts/build-release-exes.ps1 docs/loom
git diff --check -- Loom docs/loom
```

Commit only progress documents:

```powershell
git add -- docs/loom/progress/phase-41-bounded-daemon-concurrency.md docs/loom/progress/MASTER.md
git commit -m "docs(loom): close bounded daemon concurrency phase"
```
