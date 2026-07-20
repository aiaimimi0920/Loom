# Loom Milestones

## Milestone 0: Migration intent locked

Completion criteria:

- Source migration matrix exists.
- ArtLoom old/new deltas are classified.
- v1 scope is locked as headless runtime + CLI/API.

## Milestone 1: Loom workspace boots

Completion criteria:

- `Loom/Cargo.toml` workspace exists.
- Required apps and crates exist.
- `cargo check --manifest-path Loom/Cargo.toml --workspace` succeeds.
- README and architecture docs describe Loom's role.

## Milestone 2: Durable runtime works

Completion criteria:

- Core primitives serialize.
- In-memory event store works.
- Actor mesh can register and dispatch messages.
- Unit tests pass for all above.

## Milestone 3: Agent/workflow runtime works

Completion criteria:

- Agent specs load from markdown/YAML frontmatter.
- Workflow graph validates.
- Workflow executor handles success and mixed failure DAGs.
- Run events are durable.

## Milestone 4: External integration is safe

Completion criteria:

- Gateway client works against mock server.
- Sandbox denies by default.
- Hooks are disabled by default and testable when enabled.

## Milestone 5: Daemon and CLI are usable

Completion criteria:

- Daemon starts on isolated configurable port.
- CLI can query status.
- CLI can run a sample workflow.
- Sample agent/workflow fixtures exist.

## Milestone 6: ArtLoom-compatible migration path exists

Completion criteria:

- Selected ArtLoom workflow fixtures convert into Loom workflow fixtures.
- Converted fixtures validate and run.
- ArtLoom smoke patterns are represented without requiring ArtHook or desktop UI.

## Milestone 7: Loom baseline ready

Completion criteria:

- `cargo fmt --manifest-path Loom/Cargo.toml --all -- --check` passes.
- `cargo check --manifest-path Loom/Cargo.toml --workspace --all-targets`
  passes.
- `cargo test --manifest-path Loom/Cargo.toml --workspace` passes.
- Daemon/CLI smoke passes.
- Migration and contract docs are complete.
