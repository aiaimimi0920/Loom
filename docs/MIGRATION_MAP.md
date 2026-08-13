# Loom Migration Map

This document records what was migrated into the Loom runtime, what was adapted,
and what intentionally remains outside this standalone repository.

## Sources

Primary reference sources:

- `Z:\project\AI\GameEditor\NeuroLoom` for Rust runtime architecture patterns.
- `Z:\project\project\ArtNexus-GitHub\ArtLoom` for clean ArtLoom workflow
  concepts and smoke scenarios.
- `Z:\project\project\ArtNexus\ArtLoom` only for reviewed local deltas.

Repository planning source:

- `docs/analysis/loom-source-migration-matrix.md`

## Implemented targets

| Source concept | Loom target | Migration type | Notes |
| --- | --- | --- | --- |
| Rust workspace shape | `Cargo.toml`, `apps/*`, `crates/*` | adapt | Independent workspace, no source vendoring. |
| Core runtime IDs/events | `crates/loom_core` | adapt | Session/run/message/event IDs and serializable runtime DTOs. |
| Durable event store | `crates/loom_durable` | adapt | In-memory `EventStore` plus actor mesh. |
| Agent specs | `crates/loom_agent`, `examples/agents` | adapt | Markdown with YAML frontmatter and deterministic project-over-user precedence. |
| Workflow graph/executor | `crates/loom_workflow` | adapt | Native Loom DAG validation and durable execution events. |
| Gateway boundary | `crates/loom_gateway` | adapt | HTTP client to external Gateway; no provider routing copy. |
| Memory/retrieval contract | `crates/loom_memory` | adapt | Session-scoped memory records, run/message links, metadata, retrieval query, and in-memory store. |
| Safe execution boundary | `crates/loom_sandbox` | adapt | Deny-by-default command policy. |
| Hooks | `crates/loom_hooks` | adapt | Disabled-by-default lifecycle event dispatch. |
| Headless host | `apps/daemon` | adapt | Local daemon with health/status endpoints. |
| CLI | `apps/cli` | adapt | Status, fixture listing, and sample workflow run commands. |
| ArtLoom desktop window shape | `apps/desktop` | adapt | Thin Loom Tauri + React shell connects to current `loom-daemon`; old ArtLoom backend is not copied. |
| Loom<->Hook Art invocation | `crates/loom_protocol/src/hook.rs`, `protocol/schemas/hook-message.v1.schema.json` | replace | `loom.hook.v1` is the only supported bridge; old ArtLoom/AHRP routes and converters were retired in Phase 70. |

## Completed later parity work

The original headless Loom baseline deferred several capabilities. Later work
restored them through Loom-owned boundaries:

- OCR and packaged ONNX runtime resources.
- Embedded Python and Python Art resources.
- Native image, image conversion, and shared image handling.
- MCP, registry, workflow store, desktop control plane, and Hook Bridge compatibility.

## Remaining architectural non-goals

- Gateway provider routing, credentials, browser workers, and relay APIs.
- Platform account, quota, and entitlement logic.

These concerns remain behind external Gateway and Platform boundaries. The
desktop remains a thin Tauri/React shell over the Loom daemon; it does not copy
the ArtLoom monolithic desktop-local backend.

## Current completion boundary

The baseline is complete when:

1. all phase progress files are checked;
2. `cargo fmt --all -- --check` passes;
3. `cargo check --locked --workspace --all-targets`
   passes;
4. `cargo test --locked --workspace` passes;
5. daemon/CLI smoke passes against `Loom/examples`.
