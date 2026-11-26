# Loom Gateway Integration

Loom calls an external OpenAI-compatible AI Gateway (such as Neuro Gateway) through
the `loom_gateway` crate and exposes a centralized AI relay bus so managed apps,
Capability Plugins, Arts, and paired LAN devices can share one configured AI origin.

Gateway remains the owner of hosted-provider routing, credential selection and
refresh, model/provider implementation, browser-worker execution, request auditing,
and quota/health policy. Loom owns local configuration persistence, CredentialStore
token isolation, request validation, local run evidence, and bounded daemon request
scheduling.

## Configuration

AI Gateway relay can be configured either via environment variables on startup or
dynamically at runtime via `PUT /v1/ai/config` (persisted in
`<control-plane-root>/settings/ai-gateway.json` with the bearer token isolated in
Loom's `CredentialStore` under `loom.ai_gateway/bearer_token`).

| Environment variable | Meaning |
| --- | --- |
| `LOOM_GATEWAY_MODEL` | Default model or alias passed to Gateway when a caller omits `model`. Enables env-configured Gateway mode when paired with `LOOM_GATEWAY_BASE_URL`. |
| `LOOM_GATEWAY_BASE_URL` | Gateway origin (`http://` or `https://`). Defaults to `http://127.0.0.1:4200` when `LOOM_GATEWAY_MODEL` is set. |
| `LOOM_GATEWAY_TOKEN` | Optional bearer token sent only in the outbound HTTP `Authorization` header. |
| `LOOM_GATEWAY_TIMEOUT_SECS` | Blocking request timeout, default 60 seconds, bounded to 1 through 300. |

### Origin security rules

The base URL must be an origin without embedded credentials, a path, query, or
fragment:

- **Loopback (`GatewayOriginKind::Loopback`)**: Literal IP loopback hosts
  (`127.0.0.1`, `[::1]`, IPv4-mapped loopback IPv6) allow cleartext `http://` or
  `https://` and always bypass system proxies (`.no_proxy()`).
- **Private LAN (`GatewayOriginKind::PrivateLan`)**: Literal RFC1918 IPv4
  (`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`) and IPv6 ULA (`fc00::/7`)
  addresses allow cleartext `http://` or `https://` for home/intranet Gateway
  deployments and always bypass system proxies (`.no_proxy()`).
- **Remote TLS (`GatewayOriginKind::RemoteTls`)**: Public IP addresses and all DNS
  hostnames strictly require `https://` with standard certificate validation.

HTTP redirects are never followed, including same-origin redirects. Every 3xx
response is a typed HTTP status error so bearer credentials and chat payloads are
never replayed to a `Location` destination. Response bodies are bounded to 2 MiB.

## Relay and capability endpoints

`loom-daemon` exposes direct relay endpoints (available to admin callers and, for
`status`/`models`/`chat/completions`, to paired LAN device sessions):

```http
GET  /v1/ai/status
GET  /v1/ai/config
PUT  /v1/ai/config
GET  /v1/ai/models
POST /v1/ai/chat/completions
```

It also exposes unified capability invocation with durable run evidence:

```http
POST /v1/invoke
```

- `ai.chat.v1`: forwards `{ "messages": [...], "model": "...", "temperature": ... }`
  to the configured Gateway `/v1/chat/completions` endpoint and records durable
  `run_started` -> `capability_completed` / `capability_failed` evidence. When no
  Gateway is configured, returns HTTP `503` with `ai_gateway_not_configured` (never
  faking AI responses).
- `loom.runtime.probe.v1`: deterministic local hub capability used to verify
  `/v1/invoke`, concurrent worker scheduling, and `SqliteRunEvidenceStore`
  transitions without requiring an external Gateway.

`GET /status` reports an additive `ai_gateway` object (`enabled`, `configured`,
`source`, `origin_kind`, `default_model`, `timeout_secs`, `has_token`) without
probing Gateway or exposing the bearer token.

## Durable evidence storage

The packaged daemon uses `LOOM_RUN_STORE_PATH` when configured, otherwise
`<control-plane-root>/runs/loom-runs.sqlite3`. Library fixtures can use the
in-memory store. Selecting persistent storage never silently falls back to memory
after an open, schema, or integrity error.

Schema v1 is defined in `crates/loom_durable/src/run_store/sqlite.rs`: `runs`
stores the indexed ID/status, canonical JSON, and creation/update timestamps;
`run_events` stores an autoincrementing sequence, run ID, kind, and object fields.
Opening validates the schema, `PRAGMA quick_check`, indexed identity/status, and
bounded JSON records before serving. Interrupted-run recovery marks stale `running`
records as `failed` with `daemon_restarted` and appends `run_interrupted` without
replay.

## Verification

Targeted gates:

```powershell
cargo fmt --all -- --check
cargo test --locked -p loom_gateway
cargo test --locked -p loom-daemon
```

Packaged smokes:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\Invoke-LoomGatewayBrainPlanSmoke.ps1 `
  -PackageDir .\release\Loom\<versionId>

powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\Invoke-LoomRunPersistenceSmoke.ps1 `
  -PackageDir .\release\Loom\<versionId>

powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\Invoke-LoomDaemonConcurrencySmoke.ps1 `
  -PackageDir .\release\Loom\<versionId>
```
