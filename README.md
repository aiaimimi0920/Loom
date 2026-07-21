# Loom

[![CI](https://github.com/aiaimimi0920/Loom/actions/workflows/ci.yml/badge.svg)](https://github.com/aiaimimi0920/Loom/actions/workflows/ci.yml)
[![Build Windows](https://github.com/aiaimimi0920/Loom/actions/workflows/build-windows.yml/badge.svg)](https://github.com/aiaimimi0920/Loom/actions/workflows/build-windows.yml)
[![Docker](https://github.com/aiaimimi0920/Loom/actions/workflows/docker.yml/badge.svg)](https://github.com/aiaimimi0920/Loom/actions/workflows/docker.yml)

Loom is Neuro's daemon-first AI brain and orchestration runtime.

Repository: https://github.com/aiaimimi0920/Loom

It owns the local runtime contracts for:

- agent definitions and deterministic resolution,
- workflow graph execution,
- durable run/session events,
- memory and retrieval interfaces,
- safe execution boundaries,
- hook event dispatch, and
- Gateway-backed model access.

Loom is not a Gateway replacement. Gateway continues to own provider routing,
credentials, relay APIs, and provider/runtime details. Platform continues to
own account, quota, entitlement, and public web surfaces. Hook continues to own
foreground capture/integration behavior.

Loom now also ships a desktop workbench under `apps/desktop`. The visible user
entry in a packaged desktop release is `Loom.exe`; it connects to the local
Loom service on loopback and can start `runtime\loom-daemon.exe` automatically
when the service is not already running. The CLI is published separately in a
`Loom-CLI-*.zip` artifact so users do not need to choose between three
executables in the desktop package. The old ArtLoom/ArtHook names remain only
where they are compatibility protocol names.

## Workspace

```text
./
├── apps/
│   ├── daemon/    # loom-daemon
│   ├── cli/       # loom
│   └── desktop/   # Loom Tauri desktop shell
├── crates/
│   ├── loom_core
│   ├── loom_durable
│   ├── loom_agent
│   ├── loom_workflow
│   ├── loom_memory
│   ├── loom_sandbox
│   ├── loom_gateway
│   └── loom_hooks
├── examples/
│   ├── agents/
│   ├── workflows/
│   └── artloom/
└── docs/
```

## CLI and daemon smoke

Build the app binaries:

```powershell
cargo build --locked -p loom-daemon -p loom-cli
```

## Desktop shell

The desktop shell is the normal UI for end users. In a packaged release, start
Loom by double-clicking or running the single desktop entry:

```powershell
.\Loom.exe
```

`Loom.exe` will try to start `runtime\loom-daemon.exe` when the local service
is offline. If you need to debug the backend manually, run the runtime daemon
first and then open the desktop:

```powershell
.\runtime\loom-daemon.exe
.\Loom.exe
```

The CLI entry for advanced scripting is available from the separate
`Loom-CLI-*.zip` release artifact. It is not copied into the desktop package
root. The desktop package contains only the user-facing `Loom.exe` plus its
internal runtime sidecar and support files.

The desktop shell restores the independent Loom window. It is implemented
separately from the Rust workspace so normal daemon/CLI checks do not pull in
Tauri dependencies.

Install and verify the desktop frontend:

```powershell
Push-Location .\apps\desktop
npm ci
npm test
npm run typecheck
npm run build
Pop-Location
```

Check the Tauri wrapper:

```powershell
cargo check --locked --manifest-path .\apps\desktop\src-tauri\Cargo.toml
```

Run the desktop during development:

```powershell
Push-Location .\apps\desktop
npm run tauri dev
Pop-Location
```

By default the shell reads `http://127.0.0.1:8765`. Override that with
`LOOM_DAEMON_URL` when testing against another loopback daemon port.
Set `LOOM_DAEMON_EXECUTABLE` to an explicit `loom-daemon.exe` path when the
daemon is not under the packaged `runtime` directory. Debug builds also check
the repository's `target\debug\loom-daemon.exe` after the explicit override
and packaged runtime location.

The UI names the ArtHook compatibility route as **截图同步**. Internally the
daemon still exposes Hook Bridge APIs because old ArtHook/ArtLoom clients expect
those method names and port behavior. Hook-generated live workflow data is
stored as `latest.yaml` and surfaced in the desktop as `hook-live` / `Hook
实时工作流`.

For the normal user path, open **截图同步** to inspect the real Hook canvas:
node placement, image previews, and links are rendered directly in the Loom
workbench. Click **打开可视化工作流** or a node to enter the full visual
workflow canvas. YAML, cURL, raw JSON, protocol methods, session paths, IPC,
and shared-memory diagnostics remain available only inside the collapsed
**高级技术信息** disclosure.

When running an isolated desktop smoke, set `LOOM_HOOK_BRIDGE_PORT` (or
`LOOM_HOOK_BRIDGE_URL`) alongside `LOOM_DAEMON_URL`; the desktop passes that
bridge address to its Hook event client so the smoke does not reuse a user's
port 19820 instance.

Start the daemon on an isolated local port:

```powershell
$env:LOOM_DAEMON_HOST = "127.0.0.1"
$env:LOOM_DAEMON_PORT = "48766"
.\target\debug\loom-daemon.exe
```

The daemon defaults to loopback binding. Non-loopback bind hosts such as
`0.0.0.0` require `LOOM_DAEMON_TOKEN`; all routes except `/health` must include
`Authorization: Bearer <token>`.

In another shell:

```powershell
.\target\debug\loom.exe status --daemon-url http://127.0.0.1:48766
.\target\debug\loom.exe agents list --examples-dir .\examples
.\target\debug\loom.exe workflows list --examples-dir .\examples
.\target\debug\loom.exe run sample.three_node --examples-dir .\examples
```

## Local capability API

`loom-daemon` exposes local capability discovery and unified invocation:

```http
GET /v1/capabilities
POST /v1/invoke
```

The current unified capability is `brain.plan`:

```json
{
  "requestId": "uuid",
  "caller": "hook",
  "capability": "brain.plan",
  "input": {
    "goal": "Create a concise plan",
    "constraints": ["optional constraint"]
  }
}
```

The response includes a Loom run record. Run state and evidence can be
retrieved through:

```http
GET /v1/runs/{run_id}
GET /v1/runs/{run_id}/events
```

Errors are structured and include codes such as `unknown_capability`,
`invalid_input`, `gateway_planner_failed`, `run_not_found`,
`run_store_failed`, and `unauthorized`.

### Gateway-backed `brain.plan`

The default planner is deterministic and offline. Set a non-empty
`LOOM_GATEWAY_MODEL` to explicitly route planning through the external Neuro
Gateway:

```powershell
$env:LOOM_GATEWAY_MODEL = "planner-model"
$env:LOOM_GATEWAY_BASE_URL = "http://127.0.0.1:4200"
$env:LOOM_GATEWAY_TOKEN = "<gateway bearer token>"
$env:LOOM_GATEWAY_TIMEOUT_SECS = "60"
.\target\debug\loom-daemon.exe
```

`LOOM_GATEWAY_BASE_URL` defaults to `http://127.0.0.1:4200` and the timeout is
bounded to 1 through 300 seconds. A configured Gateway is required to answer
successfully; timeout, unavailable Gateway, non-2xx response, malformed JSON,
or an invalid plan returns HTTP 502 with
`gateway_planner_failed` instead of silently using the local template.

Successful output identifies its source in `output.planner.source` as
`local_template` or `gateway`, and includes the resolved Gateway model when
available. Every valid request creates a queryable `running` run first, then
stores either `succeeded` plus `capability_completed` or `failed` plus
`capability_failed` evidence. Gateway tokens and prompts are not persisted in
status, runs, events, or public error responses.

Check the operator-visible mode without probing Gateway:

```powershell
Invoke-RestMethod http://127.0.0.1:8765/status | ConvertTo-Json -Depth 10
```

For a packaged, isolated verification that does not require the desktop
executable:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\Invoke-LoomGatewayBrainPlanSmoke.ps1 `
  -PackageDir .\release\Loom\<versionId>
```

## Bounded daemon request execution

The production `loom-daemon.exe` uses a bounded request executor. By default it
starts four request workers and accepts up to thirty-two additional queued
requests. Queue capacity counts jobs waiting in the queue; active worker jobs
are bounded separately by the worker count. Submission uses non-blocking
`try_send`, so a full queue does not make the accept loop wait for capacity:

```text
LOOM_DAEMON_WORKERS=4
LOOM_DAEMON_QUEUE_CAPACITY=32
```

Empty or unset values use these defaults. `LOOM_DAEMON_WORKERS` accepts `1` to
`32`, and `LOOM_DAEMON_QUEUE_CAPACITY` accepts `1` to `1024`. An invalid
non-empty value fails daemon startup before the listener is bound and names the
invalid environment variable in the error.

`GET /status` reports safe executor metadata without exposing queue contents or
request data:

```json
{
  "requestExecutor": {
    "mode": "bounded_workers",
    "workers": 4,
    "queueCapacity": 32
  }
}
```

Library constructors such as `DaemonConfig::localhost(...)` intentionally keep
the `inline` executor default (`workers: 1`, `queueCapacity: 0`) so embedded
callers and library tests retain their existing behavior. Library callers may
opt into bounded mode explicitly with
`DaemonConfig::with_bounded_request_executor(...)`; only the production binary
opts into bounded mode automatically through the environment.

The concurrent route allowlist is deliberately narrow: `/health`, `/status`,
`/v1/capabilities`, run reads and events, run creation, run stop/retry, and
`/v1/invoke` for `brain.plan` and `tea.ticket.decompose.v1`. Only `/health` and
`/status` are reserved probes outside the normal queue. The other allowlisted
routes still use a worker and bounded queue capacity. Other file-backed
control-plane and compatibility routes also run on a worker but acquire a
serialized route boundary until their stores have stronger per-store locking.

When the bounded queue is full, Loom returns HTTP `503 Service Unavailable`
with `error.code = "daemon_busy"` and `retryable = true`. The rejected request
is not executed and creates no run or event. If shutdown arrives while the
accept thread is still reading an already accepted request, that request
receives `daemon_shutting_down` with the same retryable 503 contract. Loom does
not automatically replay or interpret client retry timing.

The packaged Windows binary maps Ctrl+C, Ctrl+Break, and console close/logoff/
shutdown events to the daemon shutdown channel. Shutdown then stops accepting
connections, closes the request sender, drains all accepted and queued work,
and joins every worker. It does not forcibly cancel a blocking Gateway call;
the existing Gateway timeout remains the boundary for that work.

Consequently, a Gateway-backed `brain.plan` call blocks only its assigned worker,
not the accept loop. `/health` and `/status` bypass the normal queue, while
another approved capability can proceed when a worker is available. The
packaged check is:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\Invoke-LoomDaemonConcurrencySmoke.ps1 `
  -PackageDir .\release\Loom\<versionId>
```

This changes request scheduling only. Gateway continues to own provider
routing, credential selection, relay APIs, and provider/runtime details; Loom
does not move Gateway provider routing into the daemon.

## Persistent run evidence

The packaged daemon stores capability runs and events in bundled SQLite below
the Loom control-plane root:

```text
<LOOM_CONTROL_PLANE_ROOT>\runs\loom-runs.sqlite3
```

Set `LOOM_RUN_STORE_PATH` to override the database file. Library-level daemon
tests use an in-memory store unless they explicitly select SQLite, while the
real `loom-daemon.exe` selects SQLite by default. `GET /status` reports only
safe metadata such as `mode = sqlite` and `persistent = true`; it does not
expose the configured path.

Run creation and every status/event transition commit atomically. Runs left in
`running` state by a daemon interruption are marked `failed` with
`daemon_restarted` and receive a final `run_interrupted` event on the next
startup. Loom never automatically replays an interrupted model or tool call.

Use the packaged restart smoke to verify two daemon instances and the desktop
sibling-daemon path against one isolated database:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\Invoke-LoomRunPersistenceSmoke.ps1 `
  -PackageDir .\release\Loom\<versionId>
```

## Configuration ownership claims

`loom-daemon` exposes the minimal configuration-ownership claim endpoint used by
independent local apps such as Tea:

```http
GET /v1/configuration/claims?app=tea
```

By default Loom does not claim Tea configuration and returns:

```json
{
  "app": "tea",
  "managed": false,
  "panel_url": null,
  "reason": "Loom has not claimed this app configuration"
}
```

For local suite testing or deployments where Loom should centralize app
settings, set:

```powershell
$env:LOOM_MANAGED_CONFIG_APPS = "tea,hook,talk"
$env:LOOM_SETTINGS_BASE_URL = "loom://settings"
```

When `tea` is listed, the claim returns `managed: true` and
`panel_url: "loom://settings/tea"`. Tea uses this to route settings buttons to
Loom and to reject competing Tea-local configuration writes while Loom is the
active owner.

## Tea run API

`loom-daemon` exposes the minimal HTTP run contract consumed by Tea:

```http
POST /v1/runs
{ "ticket": { "id": "<ticket uuid>", "title": "...", "description": "..." } }
```

It returns a Tea-compatible `Run` JSON object with `id`, `ticket_id`,
`loom_session_id`, `status`, and optional `evidence`.

```http
POST /v1/runs/{run_id}/stop
POST /v1/runs/{run_id}/retry
{ "run": <Run> }
```

These return the updated canonical stored `Run`. The request body must contain
the matching `run.id`, but caller-supplied input, output, and evidence fields do
not overwrite stored history. This contract lets Tea dispatch approved
work-order runs to Loom over HTTP while keeping Tea as the ticket/event source
of truth.

## Container image

```powershell
docker build -t loom:local .
docker run --rm -p 48766:8765 loom:local
```

The container remains daemon-first: `loom-daemon` is the container entrypoint
and `loom` is retained inside the image for operator and health-check scripts.
The desktop shell is not part of the server image.

## Release tooling

Standalone release packages default to `.\release\Loom`. The Neuro parent
release boundary remains available through the explicit `-OutputRoot` option:

```powershell
.\scripts\build-release.ps1 -VersionId local-release
.\scripts\verify-release.ps1 -PackageDir .\release\Loom\local-release

.\scripts\build-release.ps1 `
  -VersionId parent-release `
  -OutputRoot C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom
```

Each candidate contains the desktop payload under one visible entry:

```text
Loom.exe
runtime\loom-daemon.exe
runtime\resources\...
runtime\bin\...
runtime\python\...
packages\Loom-<versionId>-windows-x64.zip
packages\Loom-<versionId>-windows-x64.zip.sha256
packages\Loom-CLI-<versionId>-windows-x64.zip
packages\Loom-CLI-<versionId>-windows-x64.zip.sha256
```

The desktop ZIP contains `Loom.exe` and the runtime tree. The CLI ZIP contains
only `loom.exe`, allowing command-line use without adding a second executable
to the desktop package root.

`verify-release.ps1` validates the complete candidate boundary: the desktop
root must contain exactly `Loom.exe`, the CLI artifact metadata must agree with
its ZIP record and bytes/hash, each ZIP must contain its exact payload, both
`.sha256` sidecars must contain the matching lowercase hash and filename, and
`checksums.sha256` must cover every other package file. The focused negative
coverage for these rules is `scripts/tests/Test-ReleaseIntegrityTamper.ps1`.

## Validation

```powershell
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
```

Targeted useful gates:

```powershell
cargo test --locked -p loom_agent -p loom_workflow
cargo test --locked -p loom_memory
cargo test --locked -p loom_gateway -p loom_sandbox -p loom_hooks
cargo test --locked -p loom-daemon -p loom-cli
cargo test --locked -p loom_workflow --test artloom_conversion
```

## Docs

- `docs/ARCHITECTURE.md`
- `docs/MIGRATION_MAP.md`
- `docs/WORKFLOW_CONTRACT.md`
- `docs/AGENT_DEFINITIONS.md`
- `docs/GATEWAY_INTEGRATION.md`
