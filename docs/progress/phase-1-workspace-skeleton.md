# Phase 1: Workspace Skeleton

## Tasks

- [x] T1.1 Create Loom Rust workspace.
  - Acceptance: `Loom/Cargo.toml`, `apps/daemon`, `apps/cli`, and v1 crates
    exist; `cargo check --manifest-path Loom/Cargo.toml --workspace` succeeds.
- [x] T1.2 Add project docs and README.
  - Acceptance: `Loom/README.md`, `Loom/docs/ARCHITECTURE.md`, and root README
    references are updated.

## Notes

Completed after Phase 0 scope was locked. `Loom/.gitkeep` was removed once real
workspace files existed.

## Validation

- `cargo check --manifest-path Loom/Cargo.toml --workspace`
