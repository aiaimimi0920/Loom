# Loom Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `Neuro/Loom` as Neuro's headless-first AI brain and orchestration runtime.

**Architecture:** `Loom` is an independent Rust workspace with thin daemon/CLI apps and focused crates for core contracts, durable events, agents, workflows, memory, sandbox, Gateway client integration, and hooks. It adapts ArtLoom workflow concepts and NeuroLoom runtime concepts, while keeping Gateway provider relay, Platform account/quota policy, and Hook foreground capture outside Loom.

**Tech Stack:** Rust 2021 workspace, Tokio, Serde, Axum or JSON-RPC for local daemon API, in-memory test stores first, optional SQLite later, Markdown/YAML-frontmatter agent specs.

---

## Pre-flight

- [x] Read `docs/loom/progress/MASTER.md` and the active phase file.
- [x] Read `docs/loom/analysis/loom-source-migration-matrix.md` before copying any source.
- [x] Confirm `Loom/` still contains only `.gitkeep` or known previous implementation files.
- [x] Do not modify `NeuroPlatform`, `ArtNexus`, `ArtNexus-GitHub`, or `NeuroLoom`; they are references only.

## Task 1: Workspace skeleton

**Files:**

- Create: `Loom/Cargo.toml`
- Create: `Loom/apps/daemon/Cargo.toml`
- Create: `Loom/apps/daemon/src/main.rs`
- Create: `Loom/apps/cli/Cargo.toml`
- Create: `Loom/apps/cli/src/main.rs`
- Create: `Loom/crates/*/Cargo.toml`
- Create: `Loom/crates/*/src/lib.rs`
- Modify: `Loom/.gitkeep` remove once real files exist

- [x] Create the Rust workspace with members listed in `docs/loom/analysis/loom-project-overview.md`.
- [x] Add minimal compileable lib crates: `loom_core`, `loom_durable`, `loom_agent`, `loom_workflow`, `loom_memory`, `loom_sandbox`, `loom_gateway`, `loom_hooks`.
- [x] Add minimal compileable app crates: `loom-daemon`, `loom` CLI.
- [x] Run `cargo check --manifest-path Loom/Cargo.toml --workspace`.
- [x] Update `docs/loom/progress/phase-1-workspace-skeleton.md` and `docs/loom/progress/MASTER.md`.

## Task 2: Core and durable runtime

**Files:**

- Modify: `Loom/crates/loom_core/src/lib.rs`
- Modify: `Loom/crates/loom_durable/src/lib.rs`
- Test: crate-local unit tests in the same files or `tests/` directories

- [x] Add IDs, errors, event/message DTOs, run/session state, and serialization tests.
- [x] Add event store trait plus in-memory event store.
- [x] Add actor mesh registration, dispatch, state tracking.
- [x] Run `cargo test --manifest-path Loom/Cargo.toml -p loom_core -p loom_durable`.
- [x] Update Phase 2 progress.

## Task 3: Agent and workflow runtime

**Files:**

- Modify: `Loom/crates/loom_agent/src/lib.rs`
- Modify: `Loom/crates/loom_workflow/src/lib.rs`
- Modify: `Loom/crates/loom_core/src/lib.rs` only for shared DTOs

- [x] Implement markdown/YAML-frontmatter `AgentSpec` loading.
- [x] Implement deterministic project/user agent scope precedence.
- [x] Implement workflow graph model and validation.
- [x] Implement workflow executor with durable run events.
- [x] Add tests for 3-node success DAG and mixed success/failure DAG.
- [x] Run `cargo test --manifest-path Loom/Cargo.toml -p loom_agent -p loom_workflow`.
- [x] Update Phase 3 progress.

## Task 4: Gateway, sandbox, hooks

**Files:**

- Modify: `Loom/crates/loom_gateway/src/lib.rs`
- Modify: `Loom/crates/loom_sandbox/src/lib.rs`
- Modify: `Loom/crates/loom_hooks/src/lib.rs`

- [x] Add Gateway client with base URL and auth token config.
- [x] Test Gateway client against a mock local endpoint.
- [x] Add sandbox deny-by-default policy and explicit allow policy.
- [x] Add hook events and disabled-by-default dispatcher.
- [x] Run `cargo test --manifest-path Loom/Cargo.toml -p loom_gateway -p loom_sandbox -p loom_hooks`.
- [x] Update Phase 4 progress.

## Task 5: Daemon and CLI

**Files:**

- Modify: `Loom/apps/daemon/src/main.rs`
- Modify: `Loom/apps/cli/src/main.rs`
- Create: `Loom/examples/agents/default.md`
- Create: `Loom/examples/workflows/three-node-success.yaml`

- [x] Implement daemon startup with isolated configurable port.
- [x] Implement health/status endpoint.
- [x] Implement CLI commands: `status`, `agents list`, `workflows list`, `run <workflow-id>`.
- [x] Add sample agent and workflow fixtures.
- [x] Run daemon/CLI smoke on an isolated port.
- [x] Update Phase 5 progress.

## Task 6: ArtLoom adapters

**Files:**

- Modify or create: `Loom/crates/loom_workflow/src/artloom.rs`
- Create: `Loom/examples/artloom/`
- Test: `Loom/crates/loom_workflow/tests/artloom_conversion.rs`

- [x] Add converter for selected ArtLoom workflow YAML fixtures.
- [x] Add tests proving converted fixtures validate and run.
- [x] Port ArtLoom success and mixed-failure smoke scenarios without requiring ArtHook or desktop UI.
- [x] Update Phase 6 progress.

## Task 7: Final validation and docs

**Files:**

- Create: `Loom/README.md`
- Create: `Loom/docs/ARCHITECTURE.md`
- Create: `Loom/docs/MIGRATION_MAP.md`
- Create: `Loom/docs/WORKFLOW_CONTRACT.md`
- Create: `Loom/docs/AGENT_DEFINITIONS.md`
- Create: `Loom/docs/GATEWAY_INTEGRATION.md`

- [x] Run `cargo fmt --manifest-path Loom/Cargo.toml --all -- --check`.
- [x] Run `cargo check --manifest-path Loom/Cargo.toml --workspace --all-targets`.
- [x] Run `cargo test --manifest-path Loom/Cargo.toml --workspace`.
- [x] Run daemon/CLI smoke.
- [x] Complete Loom docs.
- [x] Update Phase 7 progress and mark `docs/loom/progress/MASTER.md` complete only when validation proves it.

## Completion gate

Do not mark the Loom objective complete until all progress phase files are checked, all validation commands pass with current output, and `Loom/` contains the implemented runtime rather than only planning documents.
