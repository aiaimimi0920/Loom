# Loom Task Breakdown

## Task name

Loom migration and runtime foundation.

## Goal

Create `Neuro/Loom` as a headless-first AI brain and orchestration runtime with
agent definitions, workflows, durable events, Gateway integration, safe tool
execution, hooks, daemon, and CLI.

## Phase 0: Migration audit and source map

### T0.1: Build source migration matrix

- Priority: P0
- Effort: M
- Dependencies: none
- Parallel lane: audit
- Acceptance criteria:
  - `docs/loom/analysis/loom-source-migration-matrix.md` lists every selected source
    area from ArtLoom, NeuroLoom, Codex, and Claude Code references.
  - Each row has `copy`, `adapt`, or `ignore`.
  - Every `copy` or `adapt` row has a target Loom module.

### T0.2: Compare ArtLoom old/new deltas

- Priority: P0
- Effort: M
- Dependencies: none
- Parallel lane: audit
- Acceptance criteria:
  - `ArtNexus-GitHub/ArtLoom` is recorded as baseline.
  - `ArtNexus/ArtLoom` local deltas are classified.
  - Runtime/build outputs are explicitly excluded.

### T0.3: Lock v1 scope

- Priority: P0
- Effort: S
- Dependencies: T0.1, T0.2
- Parallel lane: audit
- Acceptance criteria:
  - v1 scope is documented as daemon + CLI + core crates.
  - Desktop/Tauri UI is deferred.
  - Gateway remains external.

## Phase 1: Workspace skeleton

### T1.1: Create Loom Rust workspace

- Priority: P0
- Effort: M
- Dependencies: T0.3
- Parallel lane: foundation
- Acceptance criteria:
  - `Loom/Cargo.toml` exists.
  - `apps/daemon`, `apps/cli`, and all v1 crates exist.
  - `cargo check --manifest-path Loom/Cargo.toml --workspace` succeeds.

### T1.2: Add project docs and README

- Priority: P1
- Effort: S
- Dependencies: T1.1
- Parallel lane: docs
- Acceptance criteria:
  - `Loom/README.md` describes Loom as Neuro's AI brain.
  - `Loom/docs/ARCHITECTURE.md` documents module ownership.
  - `Neuro/README.md` points to `Loom/`.

## Phase 2: Core and durable runtime

### T2.1: Implement core primitives

- Priority: P0
- Effort: M
- Dependencies: T1.1
- Parallel lane: core
- Acceptance criteria:
  - `loom_core` exposes errors, result type, IDs, events, messages, run state,
    and serialized API DTOs.
  - Unit tests cover ID serialization and event/message serialization.

### T2.2: Implement in-memory event store

- Priority: P0
- Effort: M
- Dependencies: T2.1
- Parallel lane: durable
- Acceptance criteria:
  - `loom_durable` has an event store trait and in-memory implementation.
  - Tests cover append, query by run/session, and ordering.

### T2.3: Implement actor mesh

- Priority: P0
- Effort: M
- Dependencies: T2.1
- Parallel lane: durable
- Acceptance criteria:
  - `loom_durable` has actor registration, dispatch, state tracking.
  - Tests cover register, send, terminate.

## Phase 3: Agents and workflows

### T3.1: Implement agent definition loader

- Priority: P0
- Effort: M
- Dependencies: T2.1
- Parallel lane: agent
- Acceptance criteria:
  - Markdown with YAML frontmatter loads into `AgentSpec`.
  - Project/user scope precedence is deterministic.
  - Tool allowlist/denylist metadata is parsed.

### T3.2: Implement workflow graph model

- Priority: P0
- Effort: M
- Dependencies: T2.1
- Parallel lane: workflow
- Acceptance criteria:
  - Workflow graph supports node IDs, edges, entry node, action kind, failure
    state, and run summary.
  - Validation detects missing entry, missing nodes, cycles where disallowed, and
    orphaned edges.

### T3.3: Implement workflow executor

- Priority: P0
- Effort: L
- Dependencies: T2.2, T2.3, T3.2
- Parallel lane: workflow
- Acceptance criteria:
  - Successful 3-node DAG test passes.
  - Mixed success/failure DAG test passes.
  - Event store records run start, node start, node finish, run finish.

### T3.4: Implement cognitive orchestration facade

- Priority: P1
- Effort: M
- Dependencies: T3.1, T3.3
- Parallel lane: cognitive
- Acceptance criteria:
  - System 1 workflow execution is exposed.
  - System 2 planning is represented by a trait and simple deterministic test
    implementation.
  - Courtroom/MoA roles can be represented as agent specs.

## Phase 4: Gateway, sandbox, and hooks

### T4.1: Implement Gateway client abstraction

- Priority: P0
- Effort: M
- Dependencies: T2.1
- Parallel lane: integration
- Acceptance criteria:
  - `loom_gateway` can call a mock Gateway chat/model endpoint.
  - Config supports base URL and auth token.
  - Loom does not duplicate Gateway routing or credential logic.

### T4.2: Implement sandbox execution contract

- Priority: P0
- Effort: M
- Dependencies: T2.1
- Parallel lane: safety
- Acceptance criteria:
  - Sandbox denies all process execution by default.
  - Explicit allow policy permits a safe fixture command in tests.
  - Denied command test proves no execution occurs.

### T4.3: Implement hooks

- Priority: P1
- Effort: M
- Dependencies: T2.1
- Parallel lane: automation
- Acceptance criteria:
  - Hook events exist for run start, run stop, before tool call, after tool call,
    and agent stop.
  - Hooks are disabled by default.
  - Enabled hook receives serialized event payload in tests.

## Phase 5: Daemon and CLI

### T5.1: Implement daemon runtime startup

- Priority: P0
- Effort: L
- Dependencies: T2.2, T2.3, T3.1, T3.3, T4.1, T4.2, T4.3
- Parallel lane: app
- Acceptance criteria:
  - Daemon starts on an isolated configurable port.
  - Health/status endpoint returns initialized module status.
  - Startup uses temp config in tests.

### T5.2: Implement CLI

- Priority: P0
- Effort: M
- Dependencies: T5.1
- Parallel lane: app
- Acceptance criteria:
  - `loom status` works against daemon.
  - `loom agents list` works.
  - `loom workflows list` works.
  - `loom run <workflow-id>` runs a sample workflow.

### T5.3: Add sample workflow and agent fixtures

- Priority: P1
- Effort: S
- Dependencies: T3.1, T3.3
- Parallel lane: fixtures
- Acceptance criteria:
  - Sample agent and workflow fixtures live under `Loom/examples/`.
  - CLI smoke can run the sample workflow.

## Phase 6: ArtLoom migration adapters

### T6.1: Add ArtLoom workflow converter

- Priority: P1
- Effort: L
- Dependencies: T3.2
- Parallel lane: migration
- Acceptance criteria:
  - Converter reads selected ArtLoom YAML fixtures.
  - Converter outputs Loom workflow fixtures.
  - Tests prove converted sample validates and runs.

### T6.2: Port ArtLoom smoke patterns

- Priority: P1
- Effort: M
- Dependencies: T5.2, T6.1
- Parallel lane: tests
- Acceptance criteria:
  - Success DAG smoke exists.
  - Mixed failure smoke exists.
  - Smoke does not require ArtHook or desktop UI.

## Phase 7: Final validation and baseline

### T7.1: Full validation

- Priority: P0
- Effort: M
- Dependencies: all prior P0 tasks
- Parallel lane: validation
- Acceptance criteria:
  - `cargo fmt --manifest-path Loom/Cargo.toml --all -- --check` passes.
  - `cargo check --manifest-path Loom/Cargo.toml --workspace --all-targets`
    passes.
  - `cargo test --manifest-path Loom/Cargo.toml --workspace` passes.
  - Daemon/CLI smoke passes.

### T7.2: Migration baseline documentation

- Priority: P1
- Effort: S
- Dependencies: T7.1
- Parallel lane: docs
- Acceptance criteria:
  - `Loom/docs/MIGRATION_MAP.md` is complete.
  - `Loom/docs/WORKFLOW_CONTRACT.md` is complete.
  - `Loom/docs/AGENT_DEFINITIONS.md` is complete.
  - `Loom/docs/GATEWAY_INTEGRATION.md` is complete.

## Parallel lane summary

- Audit lane: T0.1, T0.2.
- Foundation/docs lanes: T1.1, T1.2.
- Core/durable lanes: T2.1, then T2.2/T2.3.
- Agent/workflow/cognitive lanes: T3.1/T3.2, then T3.3/T3.4.
- Integration/safety/automation lanes: T4.1/T4.2/T4.3.
- App/fixtures lanes: T5.1/T5.2/T5.3.
- Migration/tests lanes: T6.1/T6.2.
- Validation/docs lanes: T7.1/T7.2.
