# Loom Risk Assessment

## Major risks

### 1. Bulk-copying ArtLoom instead of migrating responsibilities

Risk:

- Loom becomes an ArtLoom UI clone and inherits ArtHook-specific assumptions.

Mitigation:

- Start with headless Rust runtime.
- Migrate workflow concepts, not React/Tauri UI.
- Keep a migration matrix for every copied source area.

### 2. Collapsing Gateway into Loom

Risk:

- Loom duplicates provider routing, credentials, and API relay logic already
  owned by Gateway.

Mitigation:

- Implement `loom_gateway` as a client only.
- Keep provider credential routing in Gateway.
- Test Loom Gateway calls with mocked Gateway endpoints.

### 3. Copying NeuroLoom skeletons without filling behavior

Risk:

- Event store, actor mesh, cognitive engine, and sandbox compile but do not
  provide verifiable runtime behavior.

Mitigation:

- Every migrated module must have behavior tests.
- In-memory implementations are acceptable; fake success APIs are not.
- Unimplemented durable backends must return explicit errors.

### 4. Unsafe tool execution

Risk:

- Loom becomes an automation engine with unrestricted command/file/network
  execution.

Mitigation:

- Sandbox is deny-by-default.
- Hooks are disabled by default.
- Tool permissions are explicit in agent definitions.

### 5. Dirty source trees and old/new ArtLoom divergence

Risk:

- `ArtNexus` and `ArtNexus-GitHub` differ; copying from the wrong one may pull
  local build artifacts or stale logic.

Mitigation:

- Use `ArtNexus-GitHub` as baseline.
- Diff against `ArtNexus` before porting local deltas.
- Exclude `node_modules`, `dist`, `coverage`, logs, and runtime outputs.

### 6. Overbuilding desktop UI before runtime correctness

Risk:

- UI work hides missing runtime contracts.

Mitigation:

- v1 milestone is daemon + CLI + tests.
- Desktop/Tauri UI is deferred until headless runtime is proven.

## Compatibility risks

- Existing ArtLoom workflow YAML may not map 1:1 to Loom workflow types.
- Existing NeuroLoom crate names and types may change during migration.
- Windows path and port behavior must be tested explicitly.
- Long-running builds on network paths may cause cargo lock contention.

## Quality gates

- `cargo fmt --manifest-path Loom/Cargo.toml --all -- --check`
- `cargo check --manifest-path Loom/Cargo.toml --workspace --all-targets`
- `cargo test --manifest-path Loom/Cargo.toml --workspace`
- Daemon/CLI smoke using an isolated port and temp config.
- Migration matrix reviewed before any source-copy phase.
