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
- [x] P41.6 Complete full source and desktop validation.
- [x] P41.7 Generate and verify the formal release candidate.

## Runtime contract

- The production `loom-daemon.exe` uses `bounded_workers` with four workers
  and a queue capacity of thirty-two by default.
- `LOOM_DAEMON_WORKERS` accepts `1..=32`; `LOOM_DAEMON_QUEUE_CAPACITY` accepts
  `1..=1024`. Empty values use defaults, and invalid values fail before bind.
- Library constructors such as `DaemonConfig::localhost(...)` retain the
  `inline` executor default with `workers = 1` and `queueCapacity = 0`.
- `/status.requestExecutor` exposes only `mode`, `workers`, and
  `queueCapacity`; no queue contents or request data are exposed.
- Concurrent routes are health/status probes, capability discovery, run reads
  and events, run creation, run stop/retry, and approved `brain.plan` and
  `tea.ticket.decompose.v1` invokes.
- `/health` and `/status` bypass the normal queue. Other legacy control-plane
  and compatibility routes run on workers behind a serialized route lock.
- A full queue returns HTTP 503 `daemon_busy` with `retryable: true`; the
  request is rejected before route execution and creates no run or event.
- A closed executor returns HTTP 503 `daemon_shutting_down` when a response can
  still be written.
- Shutdown stops accepting, closes the sender, drains accepted and queued work,
  and joins workers without forced cancellation.
- Gateway provider routing remains owned by Gateway and is not moved into Loom.

## Implementation evidence

- `58b9dbe` adds the standard-library bounded request executor, named workers,
  panic isolation, queue rejection, draining shutdown, and unit tests.
- `09d6812` adds production configuration parsing, range validation, and safe
  executor status metadata.
- `1f3d23a` shares the daemon route runtime across the accept loop and workers.
- `bea647b` adds bounded dispatch, concurrent route classification, and the
  serialized legacy route boundary.
- `fb1299b` enables bounded workers in the production binary and validates
  invalid environment values before listener startup.
- `72100ba` adds the packaged concurrency smoke and its PowerShell contract.
- `ab9a52d` preserves valid empty-array persistence evidence and keeps cleanup
  failures diagnostic whether PowerShell supplies an `ErrorRecord` or an
  exception.
- `d45eccd` wires Windows console control events into the daemon shutdown
  channel, makes the accepted-request shutdown race return the documented
  retryable `daemon_shutting_down` response, and directly verifies queue-full
  no-run evidence plus clean `CTRL_BREAK_EVENT` binary exit.

## Current validation

- Rust formatting and workspace all-target compilation passed with
  `cargo fmt --manifest-path Loom/Cargo.toml --all -- --check` and
  `cargo check --manifest-path Loom/Cargo.toml --workspace --all-targets --locked`.
- `loom-daemon`: 110 library tests and 8 daemon CLI contract tests passed.
- `loom_durable`: 21 tests passed; `loom-cli`: 5 tests passed.
- Complete workspace validation passed with 269 tests, 0 failures.
- Desktop source validation passed: `npm run typecheck`, `npm run build`, and
  Tauri `cargo check --manifest-path Loom/apps/desktop/src-tauri/Cargo.toml
  --locked`.
- Debug packaged concurrency smoke passed with `requestExecutorMode =
  bounded_workers`, `workers = 2`, `queueCapacity = 4`, Gateway entry,
  health/status responsiveness, second approved capability completion, both
  successful runs, ordered events, and exact-path process cleanup.
- Debug evidence:
  `Loom/target/runtime-smoke/daemon-concurrency/20260719-212358-9bd2602c`.
- Failure-path smoke evidence also writes a failed summary before reporting
  the error:
  `Loom/target/runtime-smoke/daemon-concurrency-failure/20260719-212515-0211d6ac`.
- Desktop shell, ArtLoom parity, run-persistence smoke, and daemon-concurrency
  smoke PowerShell contracts passed after this progress entry was registered.
  Full source, desktop, package, and formal release validation is complete.

## Formal release closure

Phase 41 closed on 2026-07-20 with the formal candidate:

```text
release/Loom/20260720-163055-8e27b864
```

Candidate provenance and package identity:

- Git head: `8e27b864aa66f289728dcdbc61790a50d401e5b8`.
- Loom shutdown and overload correctness fix: `d45eccd`.
- Repository state: `gitDirty = true`, preserving unrelated parallel monorepo
  work.
- Approved Loom source state: `sourceGitDirty = false`.
- Approved source paths: `Loom`, `scripts/build-release-exes.ps1`.
- Packaged executables: `loom.exe`, `loom-daemon.exe`, and
  `loom-desktop.exe`.
- ZIP:
  `packages/Loom-20260720-163055-8e27b864-windows-x64.zip`.
- Independently verified ZIP SHA-256:
  `d7ac699a6ae615a6a70a23b108507e91b026941c15cc48bfe08e1db4474acc39`.
- The payload ZIP contains exactly the 24 executable/support files declared by
  the manifest. `manifest.json`, `checksums.sha256`, `BUILD_INFO.txt`, build
  logs, and ZIP sidecars remain outer candidate metadata by design.

Release-level evidence:

- Formal verifier passed with 31 checksum entries and
  `sourceGitDirty = false`:
  `Loom/target/runtime-smoke/20260720-164102-formal-release-verification/summary.json`.
- Independent artifact and evidence audit passed all 31 checksums, the exact
  24-file ZIP payload, 32 parseable JSON files, UTF-8 without BOM, token
  redaction, and zero remaining candidate processes:
  `Loom/target/runtime-smoke/20260720-164102-formal-release-verification/independent-audit.json`.
- Unified local release smoke passed across the existing Loom runtime, desktop,
  OCR, embedded Python, MCP, workflow, Hook, compatibility, local
  `brain.plan`, and bearer-token matrix:
  `output/smoke/runs/20260720-163643-Loom-18052-1df32550dddd4d58ae91ed5039d40543/release-local-apps-20260720-163055-8e27b864-Loom-summary.json`.
- Packaged persistence smoke passed:
  `Loom/target/runtime-smoke/20260720-163704-e6c967e6/summary.json`.
  It proved `persistedStatus = succeeded`, event sequences `[1, 2]` before and
  after restart, CLI exit code `0`, desktop sibling-daemon reuse, and no
  remaining candidate process. Empty cleanup evidence is valid UTF-8 JSON.
- Packaged Gateway planner smoke passed:
  `Loom/target/runtime-smoke/20260720-163735-7135275c/summary.json`.
  It proved `plannerSource = gateway`, resolved model propagation, a succeeded
  durable run, ordered `run_started, capability_completed` events, and complete
  daemon/Gateway cleanup.
- Packaged concurrency smoke passed:
  `Loom/target/runtime-smoke/daemon-concurrency/20260720-163753-8ef8748a/summary.json`.
  It proved `bounded_workers`, `workers = 2`, `queueCapacity = 4`, Gateway entry,
  health/status response and a second approved capability before Gateway
  release, two succeeded runs with ordered events, and complete process/job
  cleanup.

## Non-goals

- Automatic replay or retry of rejected, queued, or interrupted requests.
- Forced cancellation or cancellation leases for blocking Gateway/tool work.
- Removing the serialized compatibility/control-plane boundary in this phase.
- Moving provider routing, credential management, or provider runtime details
  from Gateway into Loom.

## Release status

Phase 41 is complete at 7/7 tasks. Its formal release candidate is
`release/Loom/20260720-163055-8e27b864`; source validation, formal verification,
unified local smoke, restart persistence, packaged Gateway planning, and
bounded concurrency smoke all passed.
