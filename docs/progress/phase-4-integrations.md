# Phase 4: Gateway, Sandbox, and Hooks

## Tasks

- [x] T4.1 Implement Gateway client abstraction.
  - Acceptance: mock Gateway chat/model endpoint test passes; Loom does not
    duplicate Gateway routing or credential logic.
- [x] T4.2 Implement sandbox execution contract.
  - Acceptance: denied commands do not run; explicit allow policy works for a
    safe fixture command.
- [x] T4.3 Implement hooks.
  - Acceptance: run/tool/agent hook events exist; hooks are disabled by default;
    enabled hook receives serialized payload in tests.

## Notes

Safety defaults are part of the acceptance criteria.

## Validation

- `cargo test --manifest-path Loom/Cargo.toml -p loom_gateway -p loom_sandbox -p loom_hooks`
