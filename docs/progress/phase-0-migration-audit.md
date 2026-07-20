# Phase 0: Migration Audit and Source Map

## Tasks

- [x] T0.1 Build source migration matrix.
  - Acceptance: `docs/loom/analysis/loom-source-migration-matrix.md` lists selected
    ArtLoom, NeuroLoom, Codex, and Claude Code source areas with copy/adapt/ignore
    decisions and target modules.
- [x] T0.2 Compare ArtLoom old/new deltas.
  - Acceptance: clean baseline vs local delta is documented; runtime/build
    outputs are excluded.
- [x] T0.3 Lock v1 scope.
  - Acceptance: v1 is explicitly limited to daemon + CLI + core crates; desktop
    UI is deferred; Gateway remains external.

## Notes

Completed. Continue with Phase 1 workspace skeleton.
