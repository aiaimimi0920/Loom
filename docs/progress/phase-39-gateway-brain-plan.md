# Phase 39: Gateway-backed Brain Planning

## Goal

Make `brain.plan` use the real Neuro Gateway when explicitly configured while
preserving deterministic offline behavior for existing Loom installations.
The phase also makes Gateway failures queryable through the existing run and
event endpoints without exposing credentials or prompts.

## Scope

- Upgrade `loom_gateway` to the OpenAI-compatible
  `POST /v1/chat/completions` contract.
- Add a focused `BrainPlanner` boundary with local-template and Gateway
  providers.
- Wire `LOOM_GATEWAY_*` configuration into the daemon and expose safe planner
  metadata through `/status`.
- Validate model output as strict JSON with one through twelve non-empty steps.
- Store `running`, `succeeded`, and `failed` state transitions in the existing
  process-local run store and preserve event ordering.
- Add a packaged smoke that requires only `loom.exe` and `loom-daemon.exe`.

## Configuration contract

Gateway mode is enabled only when `LOOM_GATEWAY_MODEL` is non-empty:

| Variable | Default / boundary |
| --- | --- |
| `LOOM_GATEWAY_MODEL` | Required to enable Gateway mode. |
| `LOOM_GATEWAY_BASE_URL` | `http://127.0.0.1:4200`; origin only. |
| `LOOM_GATEWAY_TOKEN` | Optional bearer token; never persisted. |
| `LOOM_GATEWAY_TIMEOUT_SECS` | 60 seconds, bounded to 1 through 300. |

Without a model, the existing local output remains:

```json
{
  "summary": "Plan prepared for <goal>",
  "steps": [
    "clarify objective",
    "identify constraints",
    "return minimal executable plan"
  ]
}
```

With a model, Gateway errors and invalid model output are explicit failures;
there is no silent local fallback. The public failure category is
`gateway_planner_failed` and the HTTP status is 502.

## Run and event contract

For a valid request the daemon first inserts a `running` run and
`run_started`. It releases the run-store mutex before the blocking Gateway
call. A successful planner result updates the same run to `succeeded`, stores
the validated output, and appends `capability_completed`. A planner failure
updates the same run to `failed`, stores a bounded diagnostic, and appends
`capability_failed`.

The additive planner metadata is:

```json
{
  "source": "local_template" | "gateway",
  "model": "optional resolved model"
}
```

Tokens, complete prompts, and raw request bodies are excluded from status,
runs, events, and public error responses.

## Implementation evidence

The implementation was delivered in these Loom commits:

- `666220e` real Gateway chat-completions transport.
- `803b020` planner providers and strict model output validation.
- `373df41` daemon planner configuration and status wiring.
- `36c4b3a` run/event lifecycle and Gateway failure mapping.
- `2d3ff38` synchronous daemon entrypoint regression fix and CLI coverage.
- `ac915e0` packaged Gateway brain-plan smoke.

The entrypoint fix is important for the configured path: Loom's daemon is a
synchronous HTTP server and uses `reqwest::blocking`; it must not construct
that client inside a Tokio async main context.

## Validation evidence

Fresh debug validation on 2026-07-19:

```powershell
$env:CARGO_TARGET_DIR = "C:\t\loom-gateway-brain-plan"
cargo fmt --manifest-path Loom/Cargo.toml --all -- --check
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon
cargo build --manifest-path Loom/Cargo.toml -p loom-daemon -p loom-cli
```

Results:

- 83 daemon library tests passed.
- 3 daemon CLI contract tests passed, including Gateway-configured startup.
- Packaged debug smoke passed with `plannerSource = gateway`.
- Packaged smoke observed `run_started,capability_completed`.
- Mock Gateway verified `POST /v1/chat/completions`, model, messages, and
  bearer authentication.
- Smoke cleanup reported `daemonStopped = true` and `gatewayJobStopped = true`.
- Evidence files were UTF-8 without BOM and contained no smoke token.

Debug smoke evidence:

```text
Loom/target/runtime-smoke/20260719-045842-e90abf5c/summary.json
```

## Formal release closure

Phase 39 closed on 2026-07-19 with the formal candidate:

```text
release/Loom/20260719-051119-dcdc94a8
```

Candidate provenance and package identity:

- Git head: `dcdc94a899506955d602f1b19ab2eb5a19884a1f`.
- Repository state: `gitDirty = true`, preserving the parallel monorepo state.
- Loom source state: `sourceGitDirty = false`.
- Approved source paths: `Loom`, `scripts/build-release-exes.ps1`.
- Packaged executables: `loom.exe`, `loom-daemon.exe`, and
  `loom-desktop.exe`.
- ZIP: `packages/Loom-20260719-051119-dcdc94a8-windows-x64.zip`.
- ZIP SHA-256:
  `23b9a0d7f907d39a1698a44c4438f4056a79e0ffbc53aac74004622d8fd71d07`.

Release-level evidence:

- Formal verifier: `passed`, with 31 checksum entries and
  `sourceGitDirty = false`.
  Evidence:
  `Loom/target/runtime-smoke/20260719-053219-formal-release-verification/summary.json`.
- Unified local release smoke:
  `output/smoke/runs/20260719-051738-Loom-36916-4a3cf592712546048aee2ac91cad8e4f/release-local-apps-20260719-051119-dcdc94a8-Loom-summary.json`.
  It retained the local-template `brain.plan` path and the existing Loom
  runtime, OCR, embedded Python, MCP, workflow, Hook, and compatibility
  coverage.
- Packaged Gateway planner smoke:
  `Loom/target/runtime-smoke/20260719-051754-e039122a/summary.json`.
  It proved `plannerSource = gateway`, resolved model propagation, a succeeded
  run, and `run_started,capability_completed` without persisting the smoke
  token.
- Desktop sibling-daemon auto-start smoke:
  `Loom/target/runtime-smoke/20260719-053035-8365b4a0/summary.json`.
  It proved the desktop remained alive, started exactly one sibling daemon,
  matched the daemon parent PID to the desktop PID, reported `health = ok`,
  `status = ready`, initialized all 8 modules, exposed 4 capabilities, returned
  CLI status with exit code 0, and left no candidate process behind.

## Explicit non-goals

This phase does not implement durable run/event persistence across restarts,
daemon-wide async or worker-backed request handling, provider routing inside
Loom, or desktop-owned Gateway credential management. Gateway remains a
separate sibling project and no files under `Gateway/` were changed.

## Release status

Phase 39 is complete. Its formal release candidate is
`release/Loom/20260719-051119-dcdc94a8`, with the verifier and all required
release-level smokes passing. The previous candidate
`release/Loom/20260718-213610-43f60196` remains the pre-Gateway baseline and
must not be described as containing this phase.
