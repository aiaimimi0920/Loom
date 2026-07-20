# Phase 5: Daemon and CLI

## Tasks

- [x] T5.1 Implement daemon runtime startup.
  - Acceptance: daemon starts on isolated configurable port and health/status
    endpoint reports initialized modules.
  - Additional current API: `GET /v1/capabilities`, `POST /v1/invoke` for
    `brain.plan`, `GET /v1/runs/{run_id}`, and
    `GET /v1/runs/{run_id}/events`.
  - Non-loopback bind hosts require `LOOM_DAEMON_TOKEN`.
- [x] T5.2 Implement CLI.
  - Acceptance: `loom status`, `loom agents list`, `loom workflows list`, and
    `loom run <workflow-id>` work.
- [x] T5.3 Add sample workflow and agent fixtures.
  - Acceptance: fixtures live under `Loom/examples/`; CLI smoke runs a sample
    workflow.

## Notes

Follow Codex-style separation: core behavior in crates; CLI/daemon bind I/O.

## Validation

- `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon -p loom-cli`
- `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon daemon_invokes_brain_plan_and_serves_run_and_events`
- `cargo run --manifest-path Loom/Cargo.toml --bin loom -- agents list --examples-dir Loom/examples`
- `cargo run --manifest-path Loom/Cargo.toml --bin loom -- workflows list --examples-dir Loom/examples`
- `cargo run --manifest-path Loom/Cargo.toml --bin loom -- run sample.three_node --examples-dir Loom/examples`
