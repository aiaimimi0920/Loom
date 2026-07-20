# Loom Module Inventory

## Target modules

### `apps/daemon`

Responsibility:

- Start and own the Loom runtime.
- Load config.
- Initialize event store, actor mesh, agent registry, workflow registry,
  Gateway client, sandbox executor, hook dispatcher, and memory service.
- Expose local HTTP or JSON-RPC control APIs.

Complexity: medium.

Dependencies:

- `loom_core`
- `loom_durable`
- `loom_agent`
- `loom_workflow`
- `loom_memory`
- `loom_sandbox`
- `loom_gateway`
- `loom_hooks`

### `apps/cli`

Responsibility:

- Provide local commands:
  - `loom status`
  - `loom agents list`
  - `loom workflows list`
  - `loom run <workflow-id>`
  - `loom chat --agent <agent-id>`

Complexity: medium.

Dependencies:

- `loom_core`
- daemon API client code from `loom_core` or a small app-local client.

### `crates/loom_core`

Responsibility:

- Shared IDs, errors, result type, message/event primitives, run/session types,
  tool identifiers, serialized status structures, and public API contracts.

Complexity: low.

Source references:

- `NeuroLoom/crates/nl_core`
- Codex protocol/core separation pattern.

### `crates/loom_durable`

Responsibility:

- Event store abstraction.
- In-memory event store for tests and first milestone.
- SQLite event store after contracts stabilize.
- Actor mesh and actor lifecycle.

Complexity: medium.

Source references:

- `NeuroLoom/crates/nl_durable/src/event_store.rs`
- `NeuroLoom/crates/nl_durable/src/actor_mesh.rs`

### `crates/loom_agent`

Responsibility:

- Agent definition schema.
- Agent teams.
- Markdown/YAML-frontmatter loader.
- Tool scope and permission metadata.
- Agent registry with project/user scope precedence.

Complexity: medium.

Source references:

- Claude Code subagent docs.
- `NeuroLoom/references/cherry-studio/.../agents`
- `NeuroLoom/references/claude-code-router/.../agents`

### `crates/loom_workflow`

Responsibility:

- Workflow graph schema.
- Workflow validation.
- Workflow execution engine.
- Run state and run summaries.
- Workflow codec/import path from ArtLoom YAML concepts.

Complexity: high.

Source references:

- `ArtNexus-GitHub/ArtLoom/src/features/workflow-editor`
- `ArtNexus-GitHub/ArtLoom/src-tauri/src/workflow_codec.rs`
- `ArtNexus-GitHub/ArtLoom/src-tauri/src/workflow_store.rs`
- `ArtNexus-GitHub/ArtLoom/src/services/WorkflowOrchestrator.ts`
- `NeuroLoom/crates/nl_cognitive/src/system1.rs`

### `crates/loom_memory`

Responsibility:

- Durable conversation/run memory.
- Archival store.
- Retrieval API.
- GraphRAG-compatible concept layer.

Complexity: medium.

Source references:

- `NeuroLoom/crates/nl_memory`

### `crates/loom_sandbox`

Responsibility:

- Safe tool and command execution contracts.
- Deny-by-default permission layer.
- Isolation abstraction.

Complexity: medium.

Source references:

- `NeuroLoom/crates/nl_sandbox`
- Codex exec/sandbox separation pattern.

### `crates/loom_gateway`

Responsibility:

- Client abstraction for Neuro Gateway.
- Gateway base URL/auth configuration.
- Mockable model-call interface.

Complexity: low.

Source references:

- `Neuro/Gateway`
- `NeuroLoom/crates/nl_llm` only as conceptual predecessor.

### `crates/loom_hooks`

Responsibility:

- Hook event model.
- Hook dispatcher.
- Disabled-by-default command hooks.
- Event names: run start, run stop, before tool call, after tool call, agent
  stop.

Complexity: medium.

Source references:

- Claude Code hooks/settings docs.
- Neuro Hook project theory.

## ArtLoom content classification

### Copy/adapt concepts

- Workflow graph representation.
- Workflow YAML codec and normalization.
- Run summaries and stable GUI-test semantics.
- IPC/backend event update model.
- Settings and MCP marketplace ideas.
- Smoke test strategy.

### Ignore for v1

- React workflow editor implementation.
- Tauri UI shell.
- ArtHook-specific parameter edit path.
- OCR-specific paths.
- Art Registry naming and UI.
- Build/runtime outputs, dist, node_modules, coverage, logs.

## NeuroLoom content classification

### Copy/adapt concepts

- Rust workspace structure.
- Event/actor durable runtime.
- Cognitive SOP/MCTS/Courtroom/MoA concepts.
- Memory/GraphRAG concepts.
- Sandbox contract ideas.
- Provider Hub lessons for local-first web UI only as a later UI reference.

### Do not copy directly without review

- Dirty in-progress UI changes.
- Provider Hub as Loom UI.
- `nl_llm` as Gateway substitute.
- Any unrestricted God Mode default.
