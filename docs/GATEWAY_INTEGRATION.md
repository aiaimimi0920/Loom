# Loom Gateway Integration

Loom calls the external Neuro Gateway through the `loom_gateway` crate. The
Gateway remains the owner of provider routing, credential selection and
refresh, model/provider implementation, browser-worker execution, request
auditing, and quota/health policy. For this integration, Loom owns the planner
request shape, validation, local run evidence, and local daemon request
scheduling; Gateway owns provider routing and provider execution.

## Configuration

Gateway-backed `brain.plan` is opt-in. `LOOM_GATEWAY_MODEL` must be a
non-empty string; when it is absent or blank, Loom uses its deterministic
local template planner and does not contact Gateway.

| Environment variable | Meaning |
| --- | --- |
| `LOOM_GATEWAY_MODEL` | Model or alias passed to Gateway. Enables Gateway mode. |
| `LOOM_GATEWAY_BASE_URL` | Gateway origin. Defaults to `http://127.0.0.1:4200`. |
| `LOOM_GATEWAY_TOKEN` | Optional bearer token sent only in the HTTP Authorization header. |
| `LOOM_GATEWAY_TIMEOUT_SECS` | Blocking request timeout, default 60 seconds, bounded to 1 through 300. |

The base URL must be an `http` or `https` origin without credentials, a path,
query, or fragment. The transport limits a response body to 1 MiB. These
limits apply before planner output validation.

## Transport contract

`GatewayClient` sends one non-streaming OpenAI-compatible request:

```http
POST /v1/chat/completions
Authorization: Bearer <LOOM_GATEWAY_TOKEN>
Content-Type: application/json
```

The JSON body contains:

```json
{
  "model": "planner-model",
  "messages": [
    {"role": "system", "content": "..."},
    {"role": "user", "content": "{\"goal\":...}"}
  ],
  "stream": false
}
```

Loom accepts the first assistant message from the standard response shape and
uses the response model when present, falling back to the requested model.
Non-2xx responses, empty choices, empty assistant content, oversized bodies,
and malformed JSON become typed transport errors. The bearer token is never
included in those error strings.

## Planner contract

The Gateway assistant content must be exactly one JSON object with this shape:

```json
{
  "summary": "short plan summary",
  "steps": ["first executable step", "second executable step"]
}
```

Loom trims and validates the summary and steps. There must be one through
twelve non-empty step strings. Prose, Markdown fences, empty summaries, and
invalid JSON are rejected; Loom does not extract a plan from arbitrary prose.
The user message is a JSON serialization of `goal`, string `constraints`, and
optional `context`. Gateway configuration values are not included in that
user payload.

## Runtime behavior

When Gateway mode is disabled, the current local response remains stable:

```json
{
  "summary": "Plan prepared for <goal>",
  "steps": [
    "clarify objective",
    "identify constraints",
    "return minimal executable plan"
  ],
  "planner": {"source": "local_template", "model": null}
}
```

When Gateway mode is enabled, a successful response carries the same
`summary`, `steps`, `runId`, and `run` fields plus:

```json
{
  "planner": {
    "source": "gateway",
    "model": "resolved-model"
  }
}
```

For every valid `brain.plan` request, the daemon first stores a `running` run
and a `run_started` event. It releases the run-store lock before making the
blocking Gateway call. Success updates that same run to `succeeded` and adds
one `capability_completed` event. Gateway or model-output failure updates it
to `failed` and adds one `capability_failed` event. Each run transition and its
event commit atomically.

Gateway failure is explicit and does not silently fall back to the local
template. The invoke response is HTTP `502 Bad Gateway` with this stable
public error category:

```json
{
  "status": "failed",
  "error": {
    "code": "gateway_planner_failed",
    "message": "Gateway-backed planning failed",
    "capability": "brain.plan",
    "runId": "<failed run id>"
  }
}
```

The failed run retains only a truncated diagnostic and safe planner metadata.
Tokens, complete prompts, and raw request bodies are not written to runs,
events, responses, or status output.

The packaged daemon stores this evidence in bundled SQLite. Gateway-backed
success and failure runs remain queryable after daemon restart. A process
interruption leaves the call terminal: on the next startup, a stale `running`
run becomes `failed` with `daemon_restarted` and receives `run_interrupted`.
Loom does not replay the Gateway request.

`GET /status` adds an additive `brain_planner` object with `mode`,
`configured`, optional `model`, and timeout metadata. Status does not probe
Gateway and never exposes `LOOM_GATEWAY_TOKEN`. It also reports safe
`run_store.mode` and `run_store.persistent` metadata without exposing the
database path. For the packaged production daemon, it also reports the
request executor shape:

```json
{
  "requestExecutor": {
    "mode": "bounded_workers",
    "workers": 4,
    "queueCapacity": 32
  }
}
```

The packaged daemon runs the Gateway call on a bounded worker. A blocked
`brain.plan` request therefore does not block `/health` or `/status`, because
those reserved probes bypass the normal request queue. Another approved
concurrent capability can proceed when worker capacity remains. Queue saturation is
reported as HTTP 503 `daemon_busy` with `retryable: true`; the rejected request
has not entered route execution and creates no run or event. If shutdown arrives
while an already accepted request is still being read, it receives
`daemon_shutting_down` with the same retryable 503 contract. Windows console
control events request this graceful shutdown; accepted work drains and workers
join without forced cancellation.

## Verification

Targeted Rust gates:

```powershell
$env:CARGO_TARGET_DIR = "C:\t\loom-gateway-brain-plan"
cargo fmt --all -- --check
cargo test --locked -p loom_gateway
cargo test --locked -p loom-daemon
```

The packaged planner smoke requires only `loom.exe` and `loom-daemon.exe`:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\Invoke-LoomGatewayBrainPlanSmoke.ps1 `
  -PackageDir .\release\Loom\<versionId>
```

It starts an isolated loopback mock Gateway, verifies the chat-completions
request and bearer header, exercises the packaged daemon and CLI, checks the
stored run/events, and writes redacted UTF-8 evidence below
`target\runtime-smoke` by default.

The packaged restart smoke verifies that local planning evidence survives two
daemon instances and remains available through the desktop-started sibling
daemon:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\Invoke-LoomRunPersistenceSmoke.ps1 `
  -PackageDir .\release\Loom\<versionId>
```

The packaged concurrency smoke holds the first Gateway request open, then
proves that health/status and `tea.ticket.decompose.v1` complete before the
Gateway release:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\Invoke-LoomDaemonConcurrencySmoke.ps1 `
  -PackageDir .\release\Loom\<versionId>
```

## Explicit boundaries

Durable run/event evidence and bounded daemon request execution now exist, but
this phase does not claim automatic replay, forced cancellation, a Gateway
provider routing layer inside Loom, or a desktop-owned Gateway credential flow.
Provider routing, credential selection, relay behavior, and provider/runtime
details remain Gateway-owned. Loom must not copy implementation from
`Gateway/` to satisfy them.
