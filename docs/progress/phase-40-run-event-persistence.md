# Phase 40: Durable Run and Event Persistence

## Goal

Persist Loom capability run evidence across daemon restarts and recover stale
running records without replaying model or tool side effects.

## Tasks

- [x] P40.1 Define the run-evidence store contract and memory backend.
- [x] P40.2 Implement bundled SQLite schema, validation, and recovery.
- [x] P40.3 Wire persistent binary configuration and safe status metadata.
- [x] P40.4 Make run transitions transactional and canonical.
- [x] P40.5 Add packaged restart and desktop auto-start smoke tooling.
- [x] P40.6 Complete full workspace and release validation.
- [x] P40.7 Generate and verify the formal release candidate.

## Runtime contract

- Library-level `DaemonConfig::localhost` uses an in-memory run store.
- The real `loom-daemon.exe` uses bundled SQLite by default.
- The default path is
  `<LOOM_CONTROL_PLANE_ROOT>\runs\loom-runs.sqlite3`.
- `LOOM_RUN_STORE_PATH` overrides the file.
- `GET /status` reports only `run_store.mode` and
  `run_store.persistent`; it does not expose the path.
- Run creation and every status/event transition commit atomically.
- Startup validates schema version 1, `PRAGMA quick_check`, run JSON, event
  JSON, indexed IDs/statuses, and foreign keys before serving HTTP.
- Stale `running` records become `failed` with `daemon_restarted` and receive
  `run_interrupted` in one recovery transaction.
- Interrupted calls are never replayed automatically.

## Implementation evidence

- `17558a8` defines `RunEvidenceStore`, `RunEventDraft`, status/error types,
  and the in-memory backend.
- `6b8c433` adds bundled `rusqlite 0.40.1`, schema version 1, full validation,
  SQLite transactions, reopen behavior, and recovery.
- `c184d2e` wires memory/SQLite configuration, safe `/status` metadata, the
  default production path, and binary-level isolated contracts.
- `cd23af5` makes run transitions transactional, preserves canonical
  stop/retry history, adds restart/recovery tests, and contains request-time
  store failures without stopping the daemon.
- `2d91613` adds packaged restart persistence and desktop sibling-daemon smoke
  tooling with exact-path process cleanup and UTF-8 evidence.
- `23bd0af` keeps package preflight inside the smoke cleanup/evidence boundary
  and adds a regression contract for failed summaries.

## Current validation

- Rust formatting check passed with
  `cargo fmt --manifest-path Loom/Cargo.toml --all -- --check`.
- Workspace all-target compilation passed with
  `cargo check --manifest-path Loom/Cargo.toml --workspace --all-targets --locked`.
- `loom_durable`: 21 tests passed across two test harnesses.
- `loom-daemon`: 94 tests passed, including 89 library tests and 5 daemon CLI
  contract tests.
- `loom-cli`: 5 tests passed.
- Complete workspace test run: 245 tests passed, 0 failed across 44 test-result
  harnesses with `cargo test --manifest-path Loom/Cargo.toml --workspace --locked`.
- Desktop source validation passed: `npm run typecheck`, `npm run build`, and
  Tauri `cargo check --manifest-path Loom/apps/desktop/src-tauri/Cargo.toml --locked`.
- Release contracts passed: desktop shell, ArtLoom parity, and run-persistence
  smoke contracts.
- Debug packaged persistence smoke passed with `persistedStatus = succeeded`,
  event sequences `[1, 2]` before and after restart, CLI exit code `0`, and no
  remaining candidate process.
- Latest debug evidence:
  `Loom/target/runtime-smoke/20260719-082225-4344a83e/summary.json`.
- Desktop was intentionally skipped only for this debug target directory,
  where `loom-desktop.exe` is not produced by the Cargo workspace build.
- The smoke preflight regression now writes a failed summary before reporting
  invalid package input; the normal debug path remains green.

## Formal release closure

Phase 40 closed on 2026-07-19 with the formal candidate:

```text
release/Loom/20260719-082918-923fc5f8
```

Candidate provenance and package identity:

- Git head: `923fc5f840cbce279496b2b43612a38a9d6e1c91`.
- Repository state: `gitDirty = true`, preserving unrelated parallel monorepo
  work.
- Approved Loom source state: `sourceGitDirty = false`.
- Approved source paths: `Loom`, `scripts/build-release-exes.ps1`.
- Packaged executables: `loom.exe`, `loom-daemon.exe`, and
  `loom-desktop.exe`.
- ZIP:
  `packages/Loom-20260719-082918-923fc5f8-windows-x64.zip`.
- ZIP SHA-256:
  `07aa84be77c860144191c9f77a1c34c1a3d139005b1e7b0d79eafb7e631b542b`.

Release-level evidence:

- Formal verifier passed with 31 checksum entries and
  `sourceGitDirty = false`:
  `Loom/target/runtime-smoke/20260719-083825-formal-release-verification/summary.json`.
- Unified local release smoke passed while retaining the complete Loom runtime,
  OCR, embedded Python, MCP, workflow, Hook, compatibility, local
  `brain.plan`, and bearer-token matrix:
  `output/smoke/runs/20260719-083627-Loom-4884-c7dc780f02b94f979a440e46735683c5/release-local-apps-20260719-082918-923fc5f8-Loom-summary.json`.
- Packaged persistence smoke passed:
  `Loom/target/runtime-smoke/20260719-083651-a92f4f37/summary.json`.
  It proved the run and `[1, 2]` event sequence survived daemon restart,
  `persistedStatus = succeeded`, CLI exit code `0`, the desktop remained alive,
  the sibling daemon parent matched the desktop PID, and no candidate process
  remained after cleanup.
- Packaged Gateway planner smoke passed:
  `Loom/target/runtime-smoke/20260719-083740-3f536340/summary.json`.
  It proved `plannerSource = gateway`, resolved model propagation, a succeeded
  durable run, `run_started,capability_completed`, and complete daemon/Gateway
  cleanup.

## Non-goals

- Async workers or daemon-wide concurrent request scheduling.
- Automatic replay or retry of interrupted calls.
- Cancellation leases, idempotency keys, retention, export, or encryption.
- Provider routing or Gateway credential management inside Loom.
- Unifying typed workflow `EventStore` values with HTTP capability evidence.

## Release status

Phase 40 is complete at 7/7 tasks. Its formal release candidate is
`release/Loom/20260719-082918-923fc5f8`; source validation, formal verification,
unified local smoke, restart persistence, desktop sibling-daemon auto-start,
and packaged Gateway planning all passed.
