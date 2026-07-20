# Loom Gateway-Backed Brain Plan Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `brain.plan` produce validated model plans through the real Neuro Gateway when explicitly configured, while preserving deterministic offline behavior and queryable failed-run evidence.

**Architecture:** `loom_gateway` becomes a synchronous, bounded OpenAI-compatible transport client for `/v1/chat/completions`. A focused daemon `brain_plan` module owns the planner trait, local template, Gateway prompt/JSON validation, status metadata, and safe errors; the daemon route owns the existing HTTP envelope and running-to-terminal run/event lifecycle.

**Tech Stack:** Rust 2021, `reqwest::blocking`, Serde/JSON, Loom's existing synchronous daemon HTTP server, PowerShell release tooling.

---

## File map

- Modify `Loom/crates/loom_gateway/Cargo.toml`: add the existing workspace `reqwest` dependency.
- Rewrite `Loom/crates/loom_gateway/src/lib.rs`: real Gateway request/response transport and typed errors.
- Create `Loom/apps/daemon/src/brain_plan.rs`: planner config, trait, local/Gateway implementations, model-output validation, and unit tests.
- Modify `Loom/apps/daemon/src/lib.rs`: planner wiring, status reporting, invoke lifecycle, and daemon integration tests.
- Modify `Loom/apps/daemon/src/main.rs`: load explicit planner configuration from environment.
- Create `Loom/scripts/Invoke-LoomGatewayBrainPlanSmoke.ps1`: packaged mock-Gateway smoke and evidence writer.
- Modify `Loom/docs/GATEWAY_INTEGRATION.md`, `Loom/docs/ARCHITECTURE.md`, and `Loom/README.md`: canonical configuration and runtime behavior.
- Create `docs/loom/progress/phase-39-gateway-brain-plan.md` and modify `docs/loom/progress/MASTER.md`: phase evidence and next-step status.

### Task 1: Replace the placeholder Gateway transport with the real public API

**Files:**
- Modify: `Loom/crates/loom_gateway/Cargo.toml`
- Modify: `Loom/crates/loom_gateway/src/lib.rs`

- [ ] **Step 1: Replace the old mock-path test with failing OpenAI-compatible transport tests**

Add tests that bind a loopback `TcpListener`, capture one request, and require:

```rust
assert!(request.starts_with("POST /v1/chat/completions HTTP/1.1"));
assert!(request.contains("Authorization: Bearer test-token"));
assert!(request.contains("\"model\":\"gpt-test\""));
assert!(request.contains("\"role\":\"system\""));
assert!(request.contains("\"role\":\"user\""));
assert!(request.contains("\"stream\":false"));
```

Return this standard response and assert the flattened result:

```json
{
  "id": "chatcmpl-test",
  "model": "gpt-test-resolved",
  "choices": [{
    "index": 0,
    "message": {"role": "assistant", "content": "{\"summary\":\"ok\",\"steps\":[\"one\"]}"},
    "finish_reason": "stop"
  }]
}
```

Also add tests for:

```rust
assert!(matches!(
    error,
    GatewayError::HttpStatus { status: 503, .. }
));

assert!(matches!(
    error,
    GatewayError::MalformedResponse(_)
));
```

- [ ] **Step 2: Run the targeted crate test and verify RED**

Run:

```powershell
cargo test --manifest-path Loom/Cargo.toml -p loom_gateway
```

Expected: failure because the current client still sends `POST /v1/chat`, serializes `{model,content}`, and expects `{model,content}`.

- [ ] **Step 3: Add `reqwest` and implement the transport types**

Add to `Loom/crates/loom_gateway/Cargo.toml`:

```toml
reqwest.workspace = true
```

Replace the old request shape with these public contracts:

```rust
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GatewayChatMessage {
    pub role: String,
    pub content: String,
}

impl GatewayChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: "system".to_owned(), content: content.into() }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self { role: "user".to_owned(), content: content.into() }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GatewayChatRequest {
    pub model: String,
    pub messages: Vec<GatewayChatMessage>,
    pub stream: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GatewayChatResponse {
    pub model: String,
    pub content: String,
    pub request_id: Option<String>,
}
```

Replace the old error variants with the transport-safe set:

```rust
#[derive(Debug, Error)]
pub enum GatewayError {
    #[error("invalid Gateway base URL: {0}")]
    InvalidBaseUrl(String),
    #[error("Gateway URL scheme must be http or https")]
    UnsupportedScheme,
    #[error("Gateway URL must not contain credentials")]
    CredentialsInUrl,
    #[error("Gateway HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Gateway I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Gateway JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Gateway returned HTTP status {status}: {message}")]
    HttpStatus { status: u16, code: Option<String>, message: String },
    #[error("Gateway response exceeded {0} bytes")]
    ResponseTooLarge(usize),
    #[error("Gateway response is malformed: {0}")]
    MalformedResponse(String),
}
```

Use a constructor instead of public config fields so timeout setup remains valid:

```rust
pub struct GatewayClientConfig {
    base_url: String,
    auth_token: Option<String>,
    timeout: Duration,
}

impl GatewayClientConfig {
    pub fn new(base_url: impl Into<String>) -> Self;
    pub fn with_auth_token(self, token: impl Into<String>) -> Self;
    pub fn with_timeout(self, timeout: Duration) -> Self;
}
```

- [ ] **Step 4: Implement bounded blocking request/response parsing**

Build one `reqwest::blocking::Client` in `GatewayClient::new`, validate that the base URL uses `http` or `https`, contains no username/password, and join `v1/chat/completions` from the origin root.

Implement:

```rust
pub fn chat(&self, request: GatewayChatRequest) -> GatewayResult<GatewayChatResponse> {
    let mut builder = self.http.post(self.chat_url.clone()).json(&request);
    if let Some(token) = self.auth_token.as_deref() {
        builder = builder.bearer_auth(token);
    }

    let mut response = builder.send()?;
    let status = response.status();
    let request_id = response
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let mut body = Vec::new();
    response
        .by_ref()
        .take((MAX_GATEWAY_RESPONSE_BYTES + 1) as u64)
        .read_to_end(&mut body)?;
    if body.len() > MAX_GATEWAY_RESPONSE_BYTES {
        return Err(GatewayError::ResponseTooLarge(body.len()));
    }
    if !status.is_success() {
        return Err(parse_gateway_http_error(status.as_u16(), &body));
    }

    parse_chat_response(&body, &request.model, request_id)
}
```

`parse_chat_response` must require `choices[0].message.content` as a non-empty string, use the response model when present and the requested model otherwise, and reject streamed/SSE or malformed JSON bodies. Limit error messages to 512 characters and never include the auth token.

- [ ] **Step 5: Run the Gateway crate tests and format check**

Run:

```powershell
cargo fmt --manifest-path Loom/Cargo.toml --all -- --check
cargo test --manifest-path Loom/Cargo.toml -p loom_gateway
```

Expected: all `loom_gateway` tests pass.

- [ ] **Step 6: Commit the real Gateway transport**

```powershell
git add -- Loom/crates/loom_gateway/Cargo.toml Loom/crates/loom_gateway/src/lib.rs Loom/Cargo.lock
git commit -m "feat(loom): call the real Gateway chat API"
```

### Task 2: Add the planner boundary and validated Gateway plan output

**Files:**
- Create: `Loom/apps/daemon/src/brain_plan.rs`

- [ ] **Step 1: Write failing planner tests before creating the implementation**

Create `brain_plan.rs` with a test module that specifies these behaviors:

```rust
#[test]
fn local_template_preserves_existing_plan_text() {
    let result = LocalTemplatePlanner.plan(BrainPlanRequest {
        goal: "release smoke".to_owned(),
        constraints: vec!["Hook Talk Loom".to_owned()],
        context: None,
    }).expect("local plan");

    assert_eq!(result.summary, "Plan prepared for release smoke");
    assert_eq!(result.steps, vec![
        "clarify objective",
        "identify constraints",
        "return minimal executable plan",
    ]);
    assert_eq!(result.source, BrainPlanSource::LocalTemplate);
}

#[test]
fn gateway_planner_parses_valid_json_plan() {
    // Mock /v1/chat/completions returns assistant content:
    // {"summary":"Gateway plan","steps":["inspect","execute"]}
    // Assert source=Gateway, resolved model, summary, and two steps.
}

#[test]
fn gateway_planner_rejects_prose_or_empty_steps() {
    // Test both non-JSON assistant content and {"summary":"x","steps":[]}.
}

#[test]
fn config_enables_gateway_only_when_model_is_non_empty() {
    // Use BrainPlannerConfig::from_lookup with a closure-backed map.
}
```

- [ ] **Step 2: Run the daemon library test and verify RED**

Run:

```powershell
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon brain_plan::tests
```

Expected: compile failure because the planner module and types do not exist.

- [ ] **Step 3: Implement configuration without global-env tests**

Define:

```rust
const DEFAULT_GATEWAY_BASE_URL: &str = "http://127.0.0.1:4200";
const DEFAULT_GATEWAY_TIMEOUT_SECS: u64 = 60;
const MIN_GATEWAY_TIMEOUT_SECS: u64 = 1;
const MAX_GATEWAY_TIMEOUT_SECS: u64 = 300;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum BrainPlannerConfig {
    LocalTemplate,
    Gateway(GatewayPlannerConfig),
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct GatewayPlannerConfig {
    pub base_url: String,
    pub auth_token: Option<String>,
    pub model: String,
    pub timeout: Duration,
}
```

Implement a manual `Debug` for `GatewayPlannerConfig` that prints `auth_token: "[REDACTED]"` when set. Implement:

```rust
impl BrainPlannerConfig {
    pub fn from_env() -> Result<Self, BrainPlannerConfigError> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    fn from_lookup<F>(lookup: F) -> Result<Self, BrainPlannerConfigError>
    where
        F: Fn(&str) -> Option<String>;
}
```

If `LOOM_GATEWAY_MODEL` is absent/blank, return `LocalTemplate` and ignore the other Gateway variables. If present, trim all values, parse timeout as an integer in `1..=300`, and preserve an optional non-empty token.

- [ ] **Step 4: Implement the planner trait and local template**

Use an internal shared boundary:

```rust
pub(crate) trait BrainPlanner: Send + Sync {
    fn plan(&self, request: BrainPlanRequest) -> Result<BrainPlanResult, BrainPlannerError>;
    fn status(&self) -> BrainPlannerStatus;
}

pub(crate) type SharedBrainPlanner = Arc<dyn BrainPlanner>;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BrainPlanRequest {
    pub goal: String,
    pub constraints: Vec<String>,
    pub context: Option<Value>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum BrainPlanSource {
    LocalTemplate,
    Gateway,
}

pub(crate) struct BrainPlanResult {
    pub summary: String,
    pub steps: Vec<String>,
    pub source: BrainPlanSource,
    pub model: Option<String>,
}
```

`LocalTemplatePlanner` must return the exact current summary and three steps.

- [ ] **Step 5: Implement the Gateway planner and strict model JSON validation**

Construct the request with separate system/user messages. The user message must be `serde_json::to_string` of:

```rust
json!({
    "goal": request.goal,
    "constraints": request.constraints,
    "context": request.context,
})
```

The system message must require JSON only with `summary` and `steps`, one to twelve executable step strings, and no Markdown fences.

Deserialize assistant content into:

```rust
#[derive(Deserialize)]
struct ModelBrainPlan {
    summary: String,
    steps: Vec<String>,
}
```

Trim summary/steps, reject an empty summary, reject zero or more than twelve steps, reject any empty step, and return `BrainPlannerError::InvalidModelOutput` rather than attempting prose extraction.

- [ ] **Step 6: Implement safe planner status and builder**

Return owned, serializable status data:

```rust
#[derive(Clone, Debug, Serialize)]
pub(crate) struct BrainPlannerStatus {
    pub mode: &'static str,
    pub configured: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u64>,
}

pub(crate) fn build_brain_planner(
    config: BrainPlannerConfig,
) -> Result<SharedBrainPlanner, BrainPlannerError>;
```

The status must never include base URL credentials or auth token.

- [ ] **Step 7: Run planner tests and commit**

Run:

```powershell
cargo fmt --manifest-path Loom/Cargo.toml --all -- --check
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon brain_plan::tests
```

Expected: all planner tests pass.

Commit:

```powershell
git add -- Loom/apps/daemon/src/brain_plan.rs
git commit -m "feat(loom): add brain planner providers"
```

### Task 3: Wire planner configuration and operator-visible status into the daemon

**Files:**
- Modify: `Loom/apps/daemon/src/lib.rs`
- Modify: `Loom/apps/daemon/src/main.rs`

- [ ] **Step 1: Write failing daemon configuration/status tests**

Add tests requiring local mode by default and Gateway metadata when injected:

```rust
assert_eq!(status["brain_planner"]["mode"], "local_template");
assert_eq!(status["brain_planner"]["configured"], false);

assert_eq!(gateway_status["brain_planner"]["mode"], "gateway");
assert_eq!(gateway_status["brain_planner"]["configured"], true);
assert_eq!(gateway_status["brain_planner"]["model"], "test-model");
assert!(gateway_status["brain_planner"].get("auth_token").is_none());
```

Add a pure configuration test showing an invalid configured timeout returns an error instead of silently selecting local mode.

- [ ] **Step 2: Run targeted tests and verify RED**

```powershell
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon daemon_reports_brain_planner
```

Expected: failure because `/status` has no `brain_planner` field and `DaemonConfig` has no planner wiring.

- [ ] **Step 3: Add planner config to `DaemonConfig` and planner instance to `LoomDaemon`**

Add:

```rust
mod brain_plan;

use brain_plan::{
    build_brain_planner, BrainPlannerConfig, BrainPlannerStatus, SharedBrainPlanner,
};
```

`DaemonConfig::bind_host` must set `brain_planner: BrainPlannerConfig::LocalTemplate`. Add:

```rust
pub fn with_brain_planner_from_env(mut self) -> Result<Self> {
    self.brain_planner = BrainPlannerConfig::from_env()
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    Ok(self)
}
```

`LoomDaemon::bind` builds one planner and stores it as `brain_planner: SharedBrainPlanner`.

- [ ] **Step 4: Load env configuration only in the production entry point**

In `main.rs`, change initial config construction to:

```rust
let mut config = DaemonConfig::bind_host(host, port).with_brain_planner_from_env()?;
```

This keeps unit/integration tests deterministic even if the parent shell has Gateway variables.

- [ ] **Step 5: Add status metadata and pass planner through routing**

Extend `StatusResponse`:

```rust
struct StatusResponse {
    status: &'static str,
    modules: Vec<ModuleStatus>,
    hooks: HookSettingsSummary,
    brain_planner: BrainPlannerStatus,
}
```

Pass `&self.brain_planner` through `serve_until -> route -> invoke_capability`. `/status` calls `brain_planner.status()` and retains `status: "ready"` without probing Gateway.

- [ ] **Step 6: Run status, daemon CLI, and full daemon tests**

```powershell
cargo fmt --manifest-path Loom/Cargo.toml --all -- --check
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon daemon_reports_brain_planner
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon --test daemon_cli_contract
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon
```

Expected: all commands pass.

- [ ] **Step 7: Commit daemon configuration wiring**

```powershell
git add -- Loom/apps/daemon/src/lib.rs Loom/apps/daemon/src/main.rs
git commit -m "feat(loom): configure gateway brain planning"
```

### Task 4: Implement running-to-terminal `brain.plan` run lifecycle

**Files:**
- Modify: `Loom/apps/daemon/src/lib.rs`

- [ ] **Step 1: Add failing local, Gateway-success, and Gateway-failure integration tests**

Keep the existing local test and add assertions:

```rust
assert_eq!(invoke["output"]["planner"]["source"], "local_template");
assert_eq!(events["events"][0]["kind"], "run_started");
assert_eq!(events["events"][1]["kind"], "capability_completed");
```

Add a mock-Gateway success test that configures the daemon planner and asserts:

```rust
assert_eq!(invoke["status"], "succeeded");
assert_eq!(invoke["output"]["summary"], "Gateway plan");
assert_eq!(invoke["output"]["steps"], json!(["inspect", "execute"]));
assert_eq!(invoke["output"]["planner"]["source"], "gateway");
assert_eq!(invoke["output"]["planner"]["model"], "resolved-model");
assert_eq!(invoke["output"]["run"]["status"], "succeeded");
```

Capture the Gateway request and assert serialized `goal`, `constraints`, and optional `context` are present, while Loom configuration/token fields are absent from the prompt.

Add a 503 mock-Gateway test that asserts:

```rust
assert!(response.starts_with("HTTP/1.1 502 Bad Gateway"));
assert_eq!(body["status"], "failed");
assert_eq!(body["error"]["code"], "gateway_planner_failed");
let run_id = body["error"]["runId"].as_str().expect("failed run id");
assert_eq!(stored_run["status"], "failed");
assert_eq!(events["events"][0]["kind"], "run_started");
assert_eq!(events["events"][1]["kind"], "capability_failed");
```

- [ ] **Step 2: Run the new integration tests and verify RED**

```powershell
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon daemon_invokes_gateway_brain_plan -- --nocapture
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon daemon_records_failed_gateway_brain_plan -- --nocapture
```

Expected: failures because `invoke_brain_plan` still constructs the fixed output after planner wiring.

- [ ] **Step 3: Create and store the running run before calling the planner**

After validating `goal`, derive string constraints without rejecting additive/legacy input fields:

```rust
let constraints = request.input
    .get("constraints")
    .and_then(Value::as_array)
    .map(|values| {
        values.iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>()
    })
    .unwrap_or_default();
let context = request.input.get("context").cloned();
```

Create a run with `status: "running"`, original input, no final output, and one `run_started` event. Insert it under the run-store lock, then release the lock before `brain_planner.plan(...)`.

- [ ] **Step 4: Map successful planner output to the stable response shape**

Build additive metadata:

```rust
let planner = json!({
    "source": result.source.as_str(),
    "model": result.model,
});
let output = json!({
    "summary": result.summary,
    "steps": result.steps,
    "planner": planner,
});
```

Update the existing run to `succeeded`, attach `output`, append
`capability_completed` with planner metadata, and return the existing success
envelope plus `output.planner`. Preserve exact local summary/steps and exact two
local event kinds.

- [ ] **Step 5: Map planner errors to a failed run and HTTP 502**

On planner failure, update the same run:

```rust
let diagnostic = truncate_diagnostic(error.to_string(), 512);
let run_error = json!({
    "code": "gateway_planner_failed",
    "message": "Gateway-backed planning failed",
    "diagnostic": diagnostic,
});
```

Set `status: "failed"`, attach `error`, append `capability_failed`, and return:

```json
{
  "requestId": "loom-request-gateway-failure",
  "status": "failed",
  "error": {
    "code": "gateway_planner_failed",
    "message": "Gateway-backed planning failed",
    "capability": "brain.plan",
    "runId": "018f7c6e-9f8a-7b6c-a5d4-123456789abc"
  }
}
```

Add `502 => "Bad Gateway"` to `write_response`. Do not include token, full prompt, or raw request body in the diagnostic.

- [ ] **Step 6: Run targeted and full daemon regressions**

```powershell
cargo fmt --manifest-path Loom/Cargo.toml --all -- --check
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon daemon_invokes_brain_plan_and_serves_run_and_events
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon daemon_invokes_gateway_brain_plan
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon daemon_records_failed_gateway_brain_plan
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon
```

Expected: all commands pass; existing local contract remains unchanged except additive planner metadata.

- [ ] **Step 7: Commit run lifecycle integration**

```powershell
git add -- Loom/apps/daemon/src/lib.rs
git commit -m "feat(loom): plan through Gateway with run evidence"
```

### Task 5: Add a repeatable packaged Gateway planner smoke

**Files:**
- Create: `Loom/scripts/Invoke-LoomGatewayBrainPlanSmoke.ps1`

- [ ] **Step 1: Create the smoke script with explicit package and evidence boundaries**

Use parameters:

```powershell
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$PackageDir,
    [string]$EvidenceRoot = ""
)
```

The script must:

1. resolve `PackageDir` and require `loom.exe` and `loom-daemon.exe`;
2. allocate separate free ports for mock Gateway and Loom daemon;
3. start a loopback `TcpListener` background job that accepts exactly one
   `/v1/chat/completions` request, verifies `Authorization: Bearer smoke-token`,
   and returns an OpenAI response whose assistant content is a JSON plan;
4. start only the packaged daemon PID with isolated `LOOM_CONTROL_PLANE_ROOT`,
   `LOOM_CONFIGURATION_ROOT`, `LOOM_LOG_DIR`, and these planner variables:

```powershell
$env:LOOM_GATEWAY_MODEL = "smoke-planner"
$env:LOOM_GATEWAY_BASE_URL = "http://127.0.0.1:$gatewayPort"
$env:LOOM_GATEWAY_TOKEN = "smoke-token"
$env:LOOM_GATEWAY_TIMEOUT_SECS = "10"
```

5. invoke `/v1/invoke` with goal, constraints, and context;
6. assert Gateway summary/steps, `planner.source=gateway`, stored succeeded run,
   and `run_started,capability_completed` events;
7. stop only the daemon PID/job it created and verify no candidate PID leak;
8. write UTF-8 without BOM evidence below `Loom/target/runtime-smoke` by default.

- [ ] **Step 2: Run the script against debug binaries and verify it passes**

Build debug binaries if they are not already available:

```powershell
cargo build --manifest-path Loom/Cargo.toml -p loom-daemon -p loom-cli
```

Create a temporary package directory containing the debug daemon and CLI, then
run:

```powershell
$debugPackage = Join-Path $env:TEMP "loom-gateway-brain-plan-debug-$PID"
New-Item -ItemType Directory -Force -Path $debugPackage | Out-Null
Copy-Item .\Loom\target\debug\loom-daemon.exe, .\Loom\target\debug\loom.exe $debugPackage
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Loom\scripts\Invoke-LoomGatewayBrainPlanSmoke.ps1 -PackageDir $debugPackage
```

Expected: exit `0`, Gateway source/model evidence present, and cleanup passed.

- [ ] **Step 3: Commit the packaged smoke tool**

```powershell
git add -- Loom/scripts/Invoke-LoomGatewayBrainPlanSmoke.ps1
git commit -m "test(loom): smoke packaged gateway planning"
```

### Task 6: Update canonical docs and phase tracking

**Files:**
- Modify: `Loom/docs/GATEWAY_INTEGRATION.md`
- Modify: `Loom/docs/ARCHITECTURE.md`
- Modify: `Loom/README.md`
- Create: `docs/loom/progress/phase-39-gateway-brain-plan.md`
- Modify: `docs/loom/progress/MASTER.md`

- [ ] **Step 1: Document real Gateway configuration and strict failure behavior**

Document these exact variables and semantics:

```text
LOOM_GATEWAY_MODEL
LOOM_GATEWAY_BASE_URL
LOOM_GATEWAY_TOKEN
LOOM_GATEWAY_TIMEOUT_SECS
```

State that no model means deterministic offline template, while a configured
model means Gateway is required for that invocation and failures produce a
failed run with `gateway_planner_failed`.

- [ ] **Step 2: Correct the architecture and transport documentation**

Replace the obsolete `/v1/chat` and `{model,content}` examples with
`POST /v1/chat/completions`, system/user messages, non-streaming response
normalization, and the rule that provider routing remains Gateway-owned.

Record that run/event storage is still process-memory and that daemon-wide
concurrent request handling plus persistent recovery remain later phases.

- [ ] **Step 3: Record Phase 39 implementation evidence**

Create the phase document with sections:

```markdown
# Phase 39: Gateway-Backed Brain Planning

## Goal
## Implemented
## Compatibility
## Validation
## Release evidence
## Remaining work
```

Update `MASTER.md` so Phase 39 is the completed active/last phase only after all
source and release validations pass. Do not claim run persistence or concurrent
model serving.

- [ ] **Step 4: Validate docs and commit**

```powershell
rg -n "POST /v1/chat$|Current v1 path" Loom/docs/GATEWAY_INTEGRATION.md Loom/README.md
git diff --check -- Loom/docs docs/loom/progress
git add -- Loom/docs/GATEWAY_INTEGRATION.md Loom/docs/ARCHITECTURE.md Loom/README.md docs/loom/progress/phase-39-gateway-brain-plan.md docs/loom/progress/MASTER.md
git commit -m "docs(loom): document gateway brain planning"
```

Expected: obsolete `/v1/chat` documentation is absent and the commit contains only Loom documentation.

### Task 7: Full validation, clean-scope release, and runtime evidence

**Files:**
- Verify only; release output under `release/Loom/$versionId`.

- [ ] **Step 1: Run focused contracts and workspace validation**

```powershell
cargo fmt --manifest-path Loom/Cargo.toml --all -- --check
cargo check --manifest-path Loom/Cargo.toml --workspace --all-targets --locked
cargo test --manifest-path Loom/Cargo.toml -p loom_gateway --locked
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon --locked
cargo test --manifest-path Loom/Cargo.toml --workspace --locked
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-desktop-shell-contract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1
```

Expected: every command exits `0`; no existing local-template or desktop contract regresses.

- [ ] **Step 2: Validate the desktop toolchain**

```powershell
Push-Location .\Loom\apps\desktop
npm run typecheck
npm run build
Pop-Location
cargo check --manifest-path .\Loom\apps\desktop\src-tauri\Cargo.toml --locked
```

Expected: TypeScript, Rsbuild, and Tauri checks pass.

- [ ] **Step 3: Inspect and commit any final Loom-only corrections**

```powershell
git diff --check -- Loom docs/loom
git status --short -- Loom docs/loom scripts/build-release-exes.ps1
git log -12 --oneline
```

Do not include Gateway, Hook, Talk, Tea, or Platform work. Commit only required
Loom corrections, then require:

```powershell
git status --porcelain --untracked-files=all -- Loom scripts/build-release-exes.ps1
```

Expected: no output.

- [ ] **Step 4: Build a new scoped-provenance candidate**

```powershell
$versionId = "$(Get-Date -Format 'yyyyMMdd-HHmmss')-$(git rev-parse --short=8 HEAD)"
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release-exes.ps1 -Apps Loom -VersionId $versionId
```

Expected: candidate is written only to `release\Loom\$versionId` and manifest contains:

```json
{
  "gitDirty": true,
  "sourceGitDirty": false,
  "sourcePaths": ["Loom", "scripts/build-release-exes.ps1"]
}
```

- [ ] **Step 5: Run formal and unified release verification**

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-release.ps1 -VersionId $versionId -Apps Loom
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\smoke-release-local-apps.ps1 -VersionId $versionId -Apps Loom
```

Expected: formal status `passed`; local fallback still reports `health=ok`,
`status=ready`, four capabilities, and exact `run_started,capability_completed`.

- [ ] **Step 6: Run the focused packaged Gateway planner smoke**

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Loom\scripts\Invoke-LoomGatewayBrainPlanSmoke.ps1 -PackageDir ".\release\Loom\$versionId"
```

Expected: Gateway planner source, model, validated summary/steps, stored run/events, and cleanup all pass.

- [ ] **Step 7: Re-run desktop sibling-daemon auto-start smoke**

Use isolated `APPDATA`, `LOCALAPPDATA`, and a dynamic `LOOM_DAEMON_URL`. Assert:

```text
loom-desktop.exe remains alive
exactly one sibling loom-daemon.exe starts
health = ok
status = ready
modules = 8/8 initialized
capabilities = 4
loom.exe status exit code = 0
no candidate PID leaks
```

- [ ] **Step 8: Final boundary and evidence check**

```powershell
git status --porcelain --untracked-files=all -- Loom scripts/build-release-exes.ps1
git diff --check -- Loom docs/loom
```

Expected: Loom source scope remains clean; release and smoke evidence paths are recorded in Phase 39 and the completion report.
