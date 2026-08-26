# Loom Architecture

Loom is an independent Rust workspace and public repository. It is designed as
a headless runtime first: reusable library crates define behavior, while the
daemon, CLI, and desktop remain application shells.

## Boundaries

- Neuro Gateway owns provider routing, credentials, relay APIs, browser-worker
  assets, and model/provider implementation details.
- Neuro Platform owns public website, account, identity, quota, entitlement, and
  operator service policy.
- Hook owns foreground capture and desktop integration behavior.
- This Loom repository owns local agent planning, workflows, memory, durable orchestration,
  safe tool execution contracts, hooks, and Gateway client calls.

## Crates

- `loom_core`: shared IDs, errors, result type, messages, run/session state,
  and serializable runtime events.
- `loom_durable`: typed workflow event contracts, the synchronous HTTP
  run-evidence store boundary, bundled SQLite persistence, and actor mesh.
- `loom_agent`: markdown/YAML-frontmatter agent definitions and scoped
  resolution.
- `loom_workflow`: workflow graph model, validation, and deterministic executor.
- `loom_memory`: memory/retrieval contracts.
- `loom_sandbox`: deny-by-default safe execution policy and explicit allow
  contracts.
- `loom_protocol`: language-neutral framework/Art manifest, process+JSON ABI,
  lockfile, permission, diagnostics, and authoring contracts embedded from the
  public v1 Schemas.
- `loom_plugin_security`: canonical package digests, Ed25519 signatures,
  publisher trust, and revocation policy.
- `loom_process`: bounded child-process execution, output capture, timeout,
  process-tree termination, and resource diagnostics.
- `loom_tool_registry`: publisher-qualified framework/Art registry, secure ZIP
  install, immutable versions, dependency/runtime locks, rollback, lifecycle
  recovery, credentials, and host-mediated network policy.
- `loom_gateway`: bounded OpenAI-compatible client boundary for calling the
  external Neuro Gateway.
- `loom_hooks`: hook event contracts and disabled-by-default dispatch.

## Applications

- `loom-daemon`: headless runtime host with local health/status APIs.
- `loom`: CLI client with `status`, `agents list`, `workflows list`, and
  `run <workflow-id>`.
- `loom-plugin`: independent Plugin SDK CLI for scaffolding, signing,
  validating, packing, conformance testing, and trust-store administration.
- `apps/desktop`: Tauri desktop shell published as the single user-facing
  `Loom.exe` entry in Windows desktop packages.

### Windows package boundary

The desktop shell and daemon remain separate processes, but the release layout
keeps the daemon and its owned support files under one internal runtime tree:

```text
Loom.exe
runtime/
  loom-daemon.exe
  resources/ocr/*
packages/
  frameworks/*.zip
  arts/*.zip
```

`Loom.exe` resolves its daemon in this order: an explicit
`LOOM_DAEMON_EXECUTABLE`, `runtime/loom-daemon.exe` beside the packaged shell,
then the development `target/debug/loom-daemon.exe` fallback. The daemon's
executable-relative resource discovery therefore remains unchanged in the
packaged layout.

Framework and Art ZIPs stay outside both executables. The desktop validates the
packaged catalogs and applies the six repo-owned sample Arts once per catalog
hash through the public framework and Art install APIs. The migration marker is
stored in the writable control plane, not in the release directory. It records
completion of the current catalog bootstrap; it does not translate an obsolete
package layout. This gives
the registry useful initial cards without introducing framework or Art-specific
branches into Loom execution.

The CLI is intentionally a separate release artifact named
`Loom-CLI-<versionId>-windows-x64.zip`. That ZIP contains only `loom.exe`; it
is not placed beside `Loom.exe` in the desktop package. This separates the
normal desktop entry from scripting and headless operator tooling without
merging the application processes.

## Durable runtime model

Loom has two deliberate event boundaries:

1. `loom_core` emits typed `LoomEvent` values for workflow/session runtime
   behavior. The asynchronous `loom_durable::EventStore<LoomEvent>` and
   `InMemoryEventStore` remain the workflow-facing boundary.
2. `loom_durable::RunEvidenceStore` owns the capability-specific JSON returned
   by the daemon run APIs. It has in-memory and bundled-SQLite implementations.
3. Library constructors such as `DaemonConfig::localhost` use the in-memory
   run store by default. The real `loom-daemon.exe` explicitly selects SQLite.
4. `loom_durable::ActorMesh` remains the in-memory actor registration,
   mailbox, and state contract.

The two event stores are intentionally not unified in this phase. Typed
workflow events and HTTP capability evidence have different schemas and
ownership rules.

The SQLite run store defaults to
`<LOOM_CONTROL_PLANE_ROOT>\runs\loom-runs.sqlite3`; `LOOM_RUN_STORE_PATH`
overrides the file. It enables foreign keys, WAL journaling, full synchronous
writes, and a five-second busy timeout. Schema version 1 stores canonical run
JSON separately from ordered event field payloads.

Run insertion and every run/event transition commit in one transaction. Store
startup performs `PRAGMA quick_check`, validates all run and event JSON, and
rejects unsupported future schemas. Existing `running` records are changed to
`failed` with `daemon_restarted` and receive `run_interrupted` in one recovery
transaction. Interrupted model or tool calls are never replayed automatically.

## Memory runtime model

`loom_memory` defines Loom's v1 memory and retrieval contract:

1. `MemoryRecord` captures session-scoped content, optional run/message links,
   tags, and string metadata.
2. `MemoryStore` appends records and queries them by session or run.
3. `MemoryQuery` performs session-scoped retrieval across content, tags, and
   metadata without leaking records from other sessions.
4. `InMemoryMemoryStore` is the deterministic test/smoke implementation until a
   persistent archival or GraphRAG backend is added.

The memory crate intentionally exposes contracts and in-memory behavior only in
v1. Durable archival storage can be added behind the same trait after workflow
and daemon integration stabilize.

## Workflow runtime model

`loom_workflow` stores a directed acyclic graph:

1. Each node has a stable string id.
2. Each node currently maps to one `Agent` action with an `ActorId`.
3. Edges define execution order.
4. Validation rejects missing entry nodes, missing edge endpoints, cycles, and
   unreachable nodes.
5. The executor records run start, actor-node events, and run finish events in
   the durable event store.

The v1 executor is deterministic and test-first. It is suitable for CLI smoke
and daemon integration before persistent storage or real agent/model dispatch
is introduced.

## Plugin runtime model

Frameworks and Arts are control-plane packages, not code linked into Loom's
default binaries. The four repo-owned framework IDs are `process`, `cloud_api`,
`mcp`, and `workflow`. Command, script, and Python Arts all use `process` and
select their package-local entry through `art.runtime.json`. A third-party
package uses the same public `loom.framework.v1` process+JSON ABI and canonical
`publisher/id` identity as the repo-owned packages.

The package roots are:

```text
<control-plane>/
  frameworks/<publisher>/<id>/
    versions/<version>-<digest-prefix>/
    active.json
    locks/
    lifecycle.json
  arts/<publisher>/<id>/
    versions/<version>-<digest-prefix>/
    active.json
    locks/
    state/
    cache/
    outputs/
```

Install and upgrade validate a securely extracted staging tree, signature and
trust policy, dependency resolution, self-test, and canonical digest before an
immutable version becomes active. Activation is journaled. Rollback repeats
identity, signature/trust/revocation, digest, lockfile, and runtime checks.
Uninstall first atomically renames the live package to a same-parent tombstone,
then commits registry removal; startup restores or deletes interrupted
tombstones according to the durable registry state.

Hook does not load plugin code and does not contain per-plugin branches. It
consumes generic capability metadata from Loom. HTTP and `loom.hook.v1`
executions all run inside Loom and create durable run
evidence. Package-backed Arts are revalidated immediately before every launch,
so an active pointer, payload, lockfile, signature, or revocation change cannot
bypass the normal HTTP execution path.

The source immutability invariant is explicit: framework/Art install,
execution, upgrade, rollback, disable, uninstall, and crash recovery may write
only to control-plane package/state roots and evidence stores. They never edit
the Loom or Hook repository/source tree. Plugin authors therefore require no
source access to either product.

Permissions combine enforced and declared boundaries. Loom enforces package
containment, immutable code versus writable state, bounded process trees,
timeout/output limits, scoped credential brokering, and
host-mediated HTTP policy. Direct arbitrary process network/filesystem access,
GPU, and clipboard are not fully OS-denied by current Job Object/process-group
isolation. Windows Job Objects also enforce memory and active-process limits;
Unix process groups currently expose those declarations as advisory.
`LOOM_PLUGIN_PERMISSION_MODE=audit` is the current default;
`strict` fails closed for those unenforceable declarations. The doctor API and
Desktop expose the same enforcement matrix rather than claiming a complete OS
sandbox.

Release builds generate the Plugin SDK, CycloneDX and SPDX SBOMs, and build
provenance as independently checksummed evidence. The public Release exposes the
desktop ZIP and the Plugin SDK ZIP, each with its checksum sidecar; the CLI and
remaining evidence stay in the workflow/package verification boundary. Formal
release workflows require a clean source tree and request GitHub build/SBOM
attestations.

## Integration model

`loom_gateway` is intentionally a client boundary. Loom forwards model work to
the external Neuro Gateway and does not duplicate provider routing, credential
selection, browser-worker execution, or quota logic.

`loom_sandbox` is deny-by-default for its mediated command boundary. Generic
plugin processes additionally use `loom_process` controls and the permission
mode described above; this is not equivalent to a complete AppContainer or
Linux namespace sandbox.

`loom_hooks` is disabled by default. Enabled dispatch serializes hook events to
registered handlers, but hooks do not affect runtime unless configured.

### Daemon request execution model

`LoomDaemon` retains sole ownership of the `TcpListener`. Workers receive
already parsed `RequestJob` values and share an `Arc<DaemonRuntime>`; the
listener is not shared with route workers.

The production `loom-daemon.exe` uses a bounded request executor with four
workers and a queue capacity of thirty-two by default. `LOOM_DAEMON_WORKERS`
may be set from `1` through `32`, and `LOOM_DAEMON_QUEUE_CAPACITY` from `1`
through `1024`; empty values use the defaults. Invalid non-empty values fail
before the listener is bound. The safe `/status` field is:

```json
{
  "requestExecutor": {
    "mode": "bounded_workers",
    "workers": 4,
    "queueCapacity": 32
  }
}
```

`DaemonConfig::localhost(...)` and other library constructors remain `inline`
by default (`workers: 1`, `queueCapacity: 0`). The production binary selects
the bounded configuration explicitly, keeping embedded callers and library
tests stable while packaged operation has bounded resource usage.

The listener still performs the existing request parsing, header/body size
checks, and read timeout handling on the accept thread. Phase 41 moves blocking
route execution, not slow-header client handling. Parsed jobs are submitted with
non-blocking `try_send` and dispatched according to a conservative route policy.
Concurrent-safe routes are `/health`, `/status`,
`/v1/capabilities`, run reads/events, run creation, run stop/retry, and
`/v1/invoke` for `brain.plan` and `tea.ticket.decompose.v1`. `/health` and
`/status` are reserved probes and bypass the normal queue so they remain
available during worker pressure. All other file-backed control-plane routes
use one serialized route lock while still running on a worker. This preserves
deterministic ordering for configuration, MCP, tool, workflow, Hook, image, OCR,
Python, and cloud stores.

The queue capacity counts jobs waiting in addition to active worker jobs.
Queue saturation returns HTTP 503 with `daemon_busy` and `retryable: true`
before route execution; the accept loop does not wait for a worker slot. It
therefore creates no run or event. If shutdown arrives while an already
accepted connection is still being read, the executor sender closes before
submission and the request receives `daemon_shutting_down` with the same
retryable 503 shape. Worker panics and route errors are contained per request;
they do not terminate the accept loop.

The packaged Windows binary translates Ctrl+C, Ctrl+Break, and console close,
logoff, or shutdown events into the existing shutdown channel. The serve loop
then stops accepting connections, closes the sender, drains accepted and queued
jobs, and joins all request workers. There is no forced cancellation in this
phase. A blocking Gateway call remains bounded by the configured Gateway
timeout, and Phase 40 startup recovery remains responsible for process-level
interruption.

### Gateway-backed brain planning

`loom-daemon` keeps the `brain.plan` provider boundary in
`apps/daemon/src/brain_plan.rs`:

1. With no non-empty `LOOM_GATEWAY_MODEL`, `LocalTemplatePlanner` returns the
   existing deterministic three-step plan.
2. With a configured model, `GatewayPlanner` sends a non-streaming
   `/v1/chat/completions` request through `loom_gateway`.
3. The planner accepts only a strict JSON `{summary, steps}` response and
   returns typed failures for transport or validation errors.
4. The daemon owns the running-to-terminal run/event transition and adds
   non-secret `planner.source` and optional `planner.model` metadata.

The daemon stores these run records through `RunEvidenceStore`. Packaged runs
survive daemon restart, and Gateway success/failure transitions commit with
their corresponding completion/failure events. The typed workflow event store
remains separate. A Gateway call occupies one bounded request worker rather
than the accept loop, so `/health`, `/status`, and another approved capability
can continue to respond whenever reserved or remaining worker capacity exists.
This is bounded request execution, not automatic replay or forced cancellation.
Gateway provider routing remains outside Loom.

## Loom and Hook Art protocol

Loom and Hook communicate through `loom.hook.v1`, with `loom.surface.v1` for
distributed interactive surfaces. Loom is the execution and resource authority;
Hook renders capability-declared nodes and sends typed actions. The public Rust
types and JSON Schema are in `crates/loom_protocol/src/hook.rs` and
`protocol/schemas/hook-message.v1.schema.json`. There is no alternate ArtLoom
workflow or execution adapter.
