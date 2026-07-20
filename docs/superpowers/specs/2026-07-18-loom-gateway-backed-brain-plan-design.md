# Loom Gateway-Backed Brain Plan Design

## Status

Approved for implementation on 2026-07-18. The existing local template remains
the offline path; explicitly enabling a Gateway model opts the caller into
transparent Gateway-backed behavior.

## Goal

Make `brain.plan` use the external Neuro Gateway for real model planning while
preserving Loom's local-first usability and the existing local capability
envelope. The phase must not move provider routing, credential selection,
quota policy, or model implementation details into Loom.

## Non-goals

- Do not modify the sibling `Gateway` implementation or its routing policy.
- Do not add persistent run storage in this phase.
- Do not redesign the `/v1/invoke` envelope or the existing capability IDs.
- Do not add streaming planning responses.
- Do not make the desktop UI own Gateway credentials in this phase.

## Approaches considered

### 1. Direct HTTP from the daemon

The daemon would construct an OpenAI-compatible request itself. This is the
smallest apparent code change, but it leaves `loom_gateway` unused, duplicates
transport/error handling, and makes future Gateway-backed capabilities diverge.
Rejected.

### 2. Shared Gateway transport plus a Loom planner module (recommended)

Upgrade `loom_gateway` to a real OpenAI-compatible chat-completions client and
put plan prompting, structured-output validation, local fallback, and run/event
semantics behind a focused daemon `BrainPlanner` trait and `brain_plan` module.
This keeps ownership clear, is testable with a local mock Gateway, and does not
require a daemon-wide async or persistence redesign. Selected.

### 3. Asynchronous planner queue with durable job state

Return a pending run, execute model calls in a worker, and persist transitions.
This is the long-term shape for slow model calls, but it expands the phase into
queue lifecycle, restart recovery, cancellation, and persistent storage. Defer
until the durable run backend is designed.

## Configuration

Gateway mode is enabled only when `LOOM_GATEWAY_MODEL` is a non-empty string.
This avoids making existing local installations fail merely because a default
Gateway port is unavailable.

| Environment variable | Required | Meaning |
| --- | --- | --- |
| `LOOM_GATEWAY_MODEL` | Yes for Gateway mode | Model/alias passed unchanged to Gateway. |
| `LOOM_GATEWAY_BASE_URL` | No | Gateway origin, default `http://127.0.0.1:4200`. |
| `LOOM_GATEWAY_TOKEN` | No | Bearer token forwarded to Gateway when present. |
| `LOOM_GATEWAY_TIMEOUT_SECS` | No | Request timeout, bounded to a safe range with a 60-second default. |

The token is never placed in Loom run records, events, response payloads, or
logs. A configured model with an unavailable or invalid Gateway is an explicit
failure, not a silent local fallback. When no model is configured, Loom uses the
current deterministic local planner.

## Transport contract

`loom_gateway::GatewayClient` will call:

```http
POST /v1/chat/completions
Authorization: Bearer <LOOM_GATEWAY_TOKEN>
Content-Type: application/json
```

The JSON request is OpenAI-compatible and contains `model`, `messages`, and
`stream: false`. Loom sends a system message requiring a JSON object and a user
message containing the goal, constraints, and optional caller-provided `context`
as serialized data. Unknown additive input fields remain accepted by the local
capability envelope and are not allowed to change model, endpoint, or auth
configuration. The client normalizes the first non-streaming assistant message
into a small Loom-owned response type. It accepts `http://` and `https://`
origins, applies a bounded request timeout, and returns typed errors for
transport, non-2xx, malformed, or oversized responses.

The client must not implement provider routing, retries that change provider
selection, credential refresh, quota probing, or response streaming.

## Planner output contract

The model is required to return one JSON object:

```json
{
  "summary": "short plan summary",
  "steps": ["first executable step", "second executable step"]
}
```

Loom validates that `summary` is non-empty and that `steps` contains between one
and twelve non-empty strings. Invalid JSON or invalid shape is a Gateway planner
failure. Loom does not attempt to interpret arbitrary prose as a plan.

The existing response fields remain available:

- `summary`
- `steps`
- `runId`
- `run`

An additive `planner` object identifies the source without exposing secrets:

```json
{
  "source": "gateway" | "local_template",
  "model": "optional Gateway response model"
}
```

`GET /status` also gains an additive `brain_planner` object containing only
`mode`, `configured`, optional `model`, and timeout metadata. It does not perform
a provider probe and does not expose the Gateway token. Existing module status
and daemon readiness semantics remain unchanged.

## Run and event semantics

For every valid request, the daemon first inserts a `running` run plus its
`run_started` event, then releases the run-store lock before calling the planner.
After planning it updates the same run and appends exactly one terminal event.
The external model cannot choose run/session IDs, capability, status, or event
kind. This lifecycle is required even though the current HTTP server is
synchronous, because failed planning must still leave queryable evidence.

### Local mode

The current deterministic summary and three steps remain unchanged. The run
continues to be `succeeded` and emits exactly `run_started` followed by
`capability_completed`, preserving existing smoke and consumer expectations.

### Gateway success

The run is `succeeded`, stores the validated model plan, and emits the same two
event kinds. Gateway source/model metadata is placed in the completion payload;
request tokens and raw model prompts are not persisted.

### Gateway failure

The daemon creates a failed run and stores:

1. `run_started`
2. `capability_failed` with a safe error code and planner source

The invoke response is an HTTP `502` with `status: "failed"`, error code
`gateway_planner_failed`, the run ID, and a concise non-secret diagnostic. This
allows callers to retrieve the failed run and events without presenting a false
successful plan.

## Error mapping

The public error payload uses stable categories:

- `gateway_planner_failed` for Gateway I/O, timeout, non-2xx, or malformed
  planner output;
- `invalid_input` for the existing missing/invalid goal contract;
- `run_not_found` and all existing capability errors remain unchanged.

The underlying transport error may be retained in the failed run's diagnostic
field after truncation, but bearer tokens and request bodies must never appear.

The daemon currently handles HTTP requests synchronously, as it already does for
cloud tool execution. This phase bounds Gateway latency with the configured
timeout but does not claim concurrent health/status service during a running
model request. Moving all request handling to an async or worker-backed server is
a separate daemon-runtime phase because it affects every existing route, not
only `brain.plan`.

## Code boundaries

- `Loom/crates/loom_gateway/src/lib.rs`: OpenAI-compatible transport and typed
  response/error parsing.
- `Loom/apps/daemon/src/brain_plan.rs`: planner configuration, prompt assembly,
  `BrainPlanner` trait, model JSON validation, local fallback, Gateway-backed
  implementation, and planner result/error types.
- `Loom/apps/daemon/src/lib.rs`: daemon configuration wiring, route dependency
  passing through `Arc<dyn BrainPlanner + Send + Sync>`, and run/event response
  integration only.
- `Loom/docs/GATEWAY_INTEGRATION.md` and `Loom/README.md`: operator-facing
  configuration and behavior documentation.

No files under `Gateway/`, `Tea/`, `Hook/`, `Talk/`, `Platform/`, or root release
scripts are required for this phase.

## Verification

1. `loom_gateway` unit tests prove method/path, auth forwarding, request schema,
   standard OpenAI response parsing, non-2xx mapping, and malformed response
   rejection.
2. Planner unit tests prove local fallback, valid Gateway JSON, invalid model
   JSON, and bounded step validation.
3. Daemon integration tests prove Gateway success, transparent failure with a
   failed run/event chain, and unchanged local fallback behavior.
4. Existing Loom workspace tests and desktop/build contracts remain green.
5. A new packaged Loom candidate is built only after the Loom source scope is
   committed, then formal verification, existing unified smoke, and a focused
   mock-Gateway packaged smoke are run under `release\\Loom`.
