# Loom Source Migration Matrix

## Decision legend

- **copy**: migrate source content directly after renaming/namespace cleanup.
- **adapt**: use the source as a design/reference and implement in Loom's Rust
  architecture.
- **ignore**: do not migrate into Loom v1.

## Source priority

1. `Z:\project\AI\GameEditor\NeuroLoom` is the primary Rust brain/runtime
   architecture reference.
2. `Z:\project\project\ArtNexus-GitHub\ArtLoom` is the clean ArtLoom baseline.
3. `Z:\project\project\ArtNexus\ArtLoom` is the newer local ArtLoom delta source
   and must be reviewed against the GitHub baseline before use.
4. OpenAI Codex and Claude Code are concept references, not bulk-copy sources.

## Matrix

| Source | Decision | Target | Reason |
| --- | --- | --- | --- |
| `NeuroLoom/Cargo.toml` | adapt | `Loom/Cargo.toml` | Provides Rust workspace shape and dependency pattern. Loom should be independent and renamed, not copied verbatim. |
| `NeuroLoom/apps/daemon` | adapt | `Loom/apps/daemon` | Useful headless runtime startup pattern; current daemon is skeletal and must be made behavior-driven. |
| `NeuroLoom/apps/cli` | adapt | `Loom/apps/cli` | CLI surface pattern; commands must be redefined around Loom status, agents, workflows, chat. |
| `NeuroLoom/apps/provider_hub` | ignore for v1 | none | Provider Hub is Gateway/provider management UI precedent, not Loom brain runtime. |
| `NeuroLoom/crates/nl_core` | adapt | `Loom/crates/loom_core` | Provides core primitive direction; names and DTOs must align to Loom sessions/runs/agents/workflows. |
| `NeuroLoom/crates/nl_durable` | adapt | `Loom/crates/loom_durable` | Event store and actor mesh concepts are central to Loom; current skeleton needs real tested behavior. |
| `NeuroLoom/crates/nl_cognitive` | adapt | `Loom/crates/loom_agent`, `Loom/crates/loom_workflow` | SOP/MCTS/Courtroom/MoA concepts form Loom's cognitive layer; implement as facades and testable runtime contracts. |
| `NeuroLoom/crates/nl_memory` | adapt | `Loom/crates/loom_memory` | Memory/GraphRAG concepts belong in Loom but should start with small durable retrieval contracts. |
| `NeuroLoom/crates/nl_sandbox` | adapt | `Loom/crates/loom_sandbox` | Execution concepts are useful, but Loom v1 must deny unsafe execution by default. |
| `NeuroLoom/crates/nl_hap` | adapt later | `Loom/crates/loom_hooks` or future networking crate | External protocol ideas are useful after daemon/CLI are stable. |
| `NeuroLoom/crates/nl_llm` | ignore as implementation | none | Gateway owns provider relay. Loom should only call Gateway via `loom_gateway`. |
| `ArtNexus-GitHub/ArtLoom/src-tauri/src/workflow_codec.rs` | adapt | `Loom/crates/loom_workflow` | Workflow YAML codec and normalization concepts are useful; implementation should target Loom workflow schema. |
| `ArtNexus-GitHub/ArtLoom/src-tauri/src/workflow_store.rs` | adapt | `Loom/crates/loom_workflow`, `loom_durable` | Store concepts are useful; persistence should go through Loom durable contracts. |
| `ArtNexus-GitHub/ArtLoom/src/services/WorkflowOrchestrator.ts` | adapt | `Loom/crates/loom_workflow` | Successful and mixed-outcome DAG semantics are directly relevant. |
| `ArtNexus-GitHub/ArtLoom/src/features/workflow-editor/types/graph.ts` | adapt | `Loom/crates/loom_workflow` | Graph schema concepts are useful but should become Rust types. |
| `ArtNexus-GitHub/ArtLoom/src-tauri/src/ipc_service.rs` | adapt | `Loom/crates/loom_hook_bridge`, `Loom/apps/daemon` | IPC/update model is required for control-plane parity. Implement as a Loom bridge with compatibility method names at the protocol edge. |
| `ArtNexus-GitHub/ArtLoom/src-tauri/src/settings.rs` and `system_settings.rs` | adapt | `Loom/crates/loom_core`, `apps/daemon` | Configuration layering is useful; names and settings must fit Loom. |
| `ArtNexus-GitHub/ArtLoom/src-tauri/src/mcp_engine.rs` | adapt | `Loom/crates/loom_mcp`, `Loom/apps/daemon`, `Loom/apps/desktop` | MCP server config, registry lookup, tools/list, and tools/call are required control-plane parity features. |
| `ArtNexus-GitHub/ArtLoom/src-tauri/src/python_engine.rs` | ignore for v1 | none | Python runtime packaging is ArtLoom/Art-specific; Loom v1 should not bundle embedded Python. |
| `ArtNexus-GitHub/ArtLoom/src-tauri/src/ocr_service.rs` | ignore for v1 | none | OCR belongs closer to Hook/foreground context, not Loom v1. |
| `ArtNexus-GitHub/ArtLoom/src/features/mcp/**` React UI | adapt | `Loom/apps/desktop` | MCP marketplace/settings UI is required control-plane parity. Rebuild in Loom desktop style rather than copying Ant Design UI wholesale. |
| `ArtNexus-GitHub/ArtLoom/src/features/art-registry/**` React UI | adapt | `Loom/apps/desktop`, `Loom/crates/loom_tool_registry` | Tool registry UI and persistence are required control-plane parity. |
| `ArtNexus-GitHub/ArtLoom/src/features/workflow-editor/**` React UI | adapt | `Loom/apps/desktop`, `Loom/crates/loom_workflow_store` | Workflow editing and graph roundtrip are required control-plane parity, staged after store/codec contracts. |
| `ArtNexus-GitHub/ArtLoom/src/features/workflow-manager/**` React UI | adapt | `Loom/apps/desktop`, `Loom/crates/loom_workflow_store` | Workflow list/create/delete surfaces are required control-plane parity. |
| `ArtNexus-GitHub/ArtLoom/__tests__/unit/*Workflow*` | adapt | `Loom/crates/loom_workflow/tests` | Test scenarios are valuable: graph validation, success runs, failure runs, summaries. |
| `ArtNexus-GitHub/ArtLoom/__tests__/unit/*mcp*` | adapt later | `Loom/crates/loom_sandbox`, `loom_hooks` | MCP/tool tests become useful once tool contracts exist. |
| `ArtNexus-GitHub/ArtLoom/release/package scripts` | ignore for v1 | none | Loom v1 baseline is source/runtime correctness, not Windows release package. |
| `ArtNexus/ArtLoom/bin/python-embed/**` | ignore | none | Embedded Python runtime and site-packages are release artifacts, not source migration input. |
| `ArtNexus/ArtLoom/dist`, `coverage`, `node_modules` | ignore | none | Build/test/dependency outputs. |
| `ArtNexus/ArtLoom` shared source diffs | review before adapt | matching Loom modules | Local source deltas may contain newer workflow/MCP/runtime behavior, but must be compared file-by-file against GitHub baseline. |
| OpenAI Codex `codex-rs/core` style architecture | adapt | all Loom crates | Reference for separating core logic from app shells. |
| OpenAI Codex `exec`/sandbox concepts | adapt | `loom_sandbox` | Reference for execution isolation boundaries. |
| OpenAI Codex CLI/TUI/app-server split | adapt | `apps/cli`, `apps/daemon` | Reference for thin user-facing shells over reusable core. |
| Claude Code subagent docs | adapt | `loom_agent` | Agent specs should use markdown/YAML-frontmatter, scoped tools, and separate context. |
| Claude Code hooks/settings docs | adapt | `loom_hooks`, `apps/daemon` | Hook/settings concepts guide event-driven automation and config layering. |

## ArtLoom old/new delta observations

Observed with a source/config/test-only file comparison excluding `node_modules`,
`dist`, `coverage`, `src-tauri/target`, and image assets:

- GitHub baseline has about 148 source/config/test files in scope.
- Local ArtLoom has thousands of additional files dominated by
  `bin/python-embed/**`; these are excluded from Loom migration.
- Shared source differences exist in workflow, MCP, runtime bridge, Tauri backend
  services, package metadata, and tests.
- GitHub baseline contains useful unit tests and helper modules that are missing
  from the local tree, including workflow editor contracts, live-session graph
  helpers, diagnostics, and process utility contracts.

Implication:

- Do not treat the local ArtLoom tree as strictly newer or better.
- Use GitHub baseline for structure and tests.
- Review local shared diffs only when implementing the corresponding Loom module.

## Locked v1 scope

In scope for Loom v1:

- Rust workspace.
- Headless daemon.
- CLI.
- Core primitives.
- Durable event store and actor mesh.
- Agent definitions.
- Workflow graph and executor.
- Gateway client.
- Safe sandbox contract.
- Hook event contract.
- Sample fixtures and smoke tests.

Out of scope for the original Loom v1 baseline:

- Desktop/Tauri UI.
- React workflow editor.
- ArtHook foreground behavior.
- OCR/image capture.
- Embedded Python runtime.
- Gateway provider routing internals.
- Platform account/quota policy.

## Phase 8 scope correction

The original v1 baseline is complete, but it is not product-complete. Phase 8
restores ArtLoom control-plane parity:

- MCP server management and stdio tool invocation move from `adapt later` to
  active scope.
- Art/tool registry moves from implicit deferred UI behavior to active scope.
- Workflow store and graph codec move from narrow conversion adapter to
  interactive persistence scope.
- The ArtLoom/ArtHook IPC model moves to a Loom Hook bridge with compatibility
  method names.
- Desktop/Tauri UI is now an active Loom control-plane surface, not out of
  scope.
