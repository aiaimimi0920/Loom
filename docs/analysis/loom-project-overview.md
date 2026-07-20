# Loom Project Overview

## Task definition

Build `Neuro/Loom` as Neuro's AI brain and orchestration runtime.

Loom is not an ArtLoom clone and is not a Gateway replacement. Loom owns agent
planning, workflows, memory, execution coordination, hooks, and local service
orchestration. Gateway remains the provider/API relay that Loom calls when model
access is required.

## Source projects and references

### Target

- `C:\Users\Public\nas_home\AI\GameEditor\Neuro\Loom`
- Current state: placeholder only (`.gitkeep`).

### Local source references

- `Z:\project\project\ArtNexus-GitHub\ArtLoom`
  - Cleaner public/baseline ArtLoom source.
  - Use as the primary reference for ArtLoom workflow registry, workflow codec,
    Tauri/backend command shape, smoke-test pattern, and release discipline.
- `Z:\project\project\ArtNexus\ArtLoom`
  - Newer local ArtLoom source with additional dirty/local deltas.
  - Use only after comparing against the GitHub baseline; do not bulk-copy.
- `Z:\project\AI\GameEditor\NeuroLoom`
  - Primary Neuro brain/runtime reference.
  - Use as the source for Rust workspace architecture, cognitive modules,
    durable runtime, memory, sandbox, HAP/networking, and Provider Hub lessons.

### External references

- `https://github.com/openai/codex`
  - Reference for separating reusable core logic from CLI/TUI/headless shells.
  - Relevant architecture pattern: Rust workspace with core, exec, protocol,
    login/provider, app-server, CLI/TUI surfaces.
- Claude Code official docs:
  - Subagents: declarative agent definitions with scoped tools and separate
    context.
  - Hooks/settings: event-driven automation points and configuration layering.
  - Use concepts only; do not assume Claude Code source is available to copy.

## Current local source shape

### ArtLoom

ArtLoom is a TypeScript/React/Tauri workflow control plane. Its useful concepts:

- Art/workflow registry and metadata management.
- Workflow graph YAML codec and normalization.
- Browser preview plus desktop runtime bridge.
- IPC backend on port `19820`.
- Workflow editor smoke tests for success and mixed failure cases.
- Settings, MCP engine, native/cloud/CLI/Python engine abstractions.
- Release and packaging scripts for a standalone app.

Do not blindly migrate:

- ArtHook-specific foreground integration.
- Art-specific naming and UI assumptions.
- Old Tauri shell as the first Loom deliverable.
- OCR/image-specific code unless later required by Hook integration.

### NeuroLoom

NeuroLoom is a Rust workspace that already sketches Loom's desired runtime:

- `apps/daemon`: headless runtime host.
- `apps/cli`: command-line surface.
- `apps/provider_hub`: local provider UI; useful as Gateway integration
  precedent but not Loom's brain.
- `crates/nl_core`: core primitives.
- `crates/nl_durable`: event store, snapshots, actor mesh.
- `crates/nl_llm`: single-line provider execution; in Neuro this should be
  split from Gateway and used only as a conceptual predecessor.
- `crates/nl_memory`: archival, HAMT, GraphRAG.
- `crates/nl_cognitive`: System 1 SOP, System 2 MCTS, courtroom/MoA.
- `crates/nl_sandbox`: executor, God Mode, Micro-VM concepts.
- `crates/nl_hap`: external protocol / networking concepts.

## Target architecture

`Neuro/Loom` should be an independent Rust workspace:

```text
Loom/
├── apps/
│   ├── daemon/
│   └── cli/
└── crates/
    ├── loom_core/
    ├── loom_durable/
    ├── loom_agent/
    ├── loom_workflow/
    ├── loom_memory/
    ├── loom_sandbox/
    ├── loom_gateway/
    └── loom_hooks/
```

## Core responsibilities

- Agent definitions and teams.
- Workflow graph execution.
- Cognitive orchestration.
- Durable evented runtime.
- Memory and retrieval.
- Tool and sandbox dispatch.
- Gateway-backed model access.
- Hook event contracts for foreground interaction.
- CLI and daemon entry points.

## Non-goals for the initial migration

- Full desktop/Tauri UI.
- Copying ArtLoom React workflow editor wholesale.
- Copying ArtHook foreground capture behavior into Loom.
- Moving Gateway provider routing or credential relay into Loom.
- Recreating Platform account/quota policy.

## Initial success criteria

- `Neuro/Loom` has a compilable Rust workspace.
- A daemon can start a Loom runtime.
- A CLI can query status and run a sample workflow.
- Agent definitions can be loaded from markdown/YAML frontmatter.
- Workflows can execute a small DAG and produce durable events.
- Gateway model calls are represented by a Gateway client interface, tested with
  a mock endpoint.
- Sandbox and hook contracts have safe defaults.
