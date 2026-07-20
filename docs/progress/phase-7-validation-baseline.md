# Phase 7: Final Validation and Baseline

## Tasks

- [x] T7.1 Full validation.
  - Acceptance: `cargo fmt --manifest-path Loom/Cargo.toml --all -- --check`,
    `cargo check --manifest-path Loom/Cargo.toml --workspace --all-targets`,
    `cargo test --manifest-path Loom/Cargo.toml --workspace`, and daemon/CLI
    smoke all pass.
- [x] T7.2 Migration baseline documentation.
  - Acceptance: `Loom/docs/MIGRATION_MAP.md`,
    `Loom/docs/WORKFLOW_CONTRACT.md`, `Loom/docs/AGENT_DEFINITIONS.md`, and
    `Loom/docs/GATEWAY_INTEGRATION.md` are complete.

## Notes

Current validation output refreshed on 2026-06-04:

- `cargo fmt --manifest-path Loom/Cargo.toml --all -- --check` passed.
- `cargo check --manifest-path Loom/Cargo.toml --workspace --all-targets`
  passed.
- `cargo test --manifest-path Loom/Cargo.toml --workspace` passed.
- Targeted validation passed:
  - `cargo test --manifest-path Loom/Cargo.toml -p loom_core -p loom_durable`
  - `cargo test --manifest-path Loom/Cargo.toml -p loom_agent -p loom_workflow`
  - `cargo test --manifest-path Loom/Cargo.toml -p loom_memory`
  - `cargo test --manifest-path Loom/Cargo.toml -p loom_gateway -p loom_sandbox -p loom_hooks`
  - `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon -p loom-cli`
  - `cargo test --manifest-path Loom/Cargo.toml -p loom_workflow --test artloom_conversion`
- Daemon/CLI smoke passed on isolated local port `48269`:
  - `loom status --daemon-url http://127.0.0.1:<port>` returned
    `{"status":"ready", ...}`.
  - `loom agents list --examples-dir Loom/examples` returned
    `planner`, `reviewer`, and `writer`.
  - `loom workflows list --examples-dir Loom/examples` returned
    `sample.three_node`.
  - `loom run sample.three_node --examples-dir Loom/examples` returned
    `succeeded start,draft,review`.
