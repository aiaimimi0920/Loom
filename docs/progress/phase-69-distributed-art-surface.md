# Phase 69: Distributed Art Surface phase-one closure

## Status

Implementation and dirty-source native runtime verification are complete.
Phase-one publication remains open only because Hook and Loom still need scoped
commits followed by clean-source release builds and provenance verification.

## Scope delivered

- `loom.surface.v1` protocol types and JSON Schemas for manifests, scenes,
  snapshots, ordered patches, lifecycle, actions, preview/formal commits,
  resources, device sessions, confirmations, cancellation, and failures.
- Persistent and temporary instance semantics, shared instance reuse, per-Hook
  attachments, attachment-scoped resource leases, and patch fanout.
- Process-framework action execution with bounded timeout, concurrency policy,
  progress, confirmation, cancellation, event deduplication, stale-generation
  rejection, and independent preview/result revisions.
- Device pairing, approval, Ed25519 challenge/session issuance, scoped short-term
  tokens, nonce replay rejection, device-bound actions, and authenticated remote
  resource service access.
- Hook declarative rendering with bounded style sanitization and stable node IDs.
- Hook JavaScript Surface isolation with CSP/no-network policy, timer/DOM/event/
  resource limits, capability-reported Chromium heap/long-task budgets, watchdog
  disposal, and declarative fallback.
- Hook patch-conflict recovery through an explicit remount request and daemon
  full-snapshot broadcast; Hook store recreation accepts the recovered snapshot.
- Persistent daemon restart recovery that retains authoritative scene/state and
  content-addressed resources, renews expired resource leases, drops temporary
  instances, replays snapshots, and executes a subsequent action.

## Prototype and security evidence

- Stock card: continuous input, commit, streaming patches, and formal quote.
- Shared dashboard: two attachments, revision fanout, formal result, two distinct
  leases, real 68-byte `image/png` resource retrieval, and SHA-256 verification.
- Project form: required-field validation, high-risk confirmation, successful
  submission, and cancellation of the real process action.
- Package integrity/path validation, unknown-node and undeclared-action rejection,
  malicious-style sanitization, event ID deduplication, unapproved-device denial,
  bearer authorization, signed device session, nonce replay rejection, and
  device-authenticated binary resource retrieval have direct tests.
- Headless Chromium 149 executes the production JavaScript Surface document and
  proves healthy heap/long-task telemetry plus timer, DOM, CPU, and memory budget
  failures. Evidence:
  `Hook/artifacts/runtime-performance/javascript-surface-browser.json`.

## Fresh source validation

- Hook Vitest: 253 files / 1048 tests passed.
- Hook Rust: 240 library tests passed; Loom connector 14, Talk connector 11,
  and all remaining non-ignored suites passed. The real Tea daemon smoke remains
  intentionally ignored unless a Tea daemon is supplied.
- Hook's production TypeScript typecheck passed. The separate test-only
  typecheck remains red on pre-existing fixture/mock typing debt outside Phase
  69; the executable Vitest suite itself is fully green.
- Loom daemon: 219 tests passed.
- Loom tool registry: 119 tests passed.
- Loom plugin CLI: 7 tests passed.
- `cargo check --locked --workspace --all-targets` and
  `cargo test --locked --workspace` passed. The first workspace check exposed
  two stale test-only `execute_workflow_node` calls missing the new deadline
  argument; both were corrected and `loom_workflow_runtime` 17/17 tests passed
  before the full workspace rerun.
- Full Hook lint is not a Phase 69 green gate: it stops on the pre-existing,
  unrelated `src/services/syncedImagePayload.ts` redundant Boolean cast; 16
  additional findings are warnings.

## Runtime candidates

### Hook R8

Path:
`release/Hook/20260811-distributed-art-surface-r8`

- `hook.exe`: 7,218,688 bytes, SHA-256
  `753b7ab09138b0df0079a4fcba77c75cf78dbc8baf618069ae58b811166a9cf9`.
- `hook-windows-x64-V0.1.7.zip`: 3,482,191 bytes, SHA-256
  `3f2c39bf78671da3880e9c9a88d0bf49e2deee3c17f12608c9659f2b1bde4f37`.
- CLI version is `hook 0.1.7`; no-GUI self-check status is `ok`.
- Candidate root contains exactly the executable and ZIP. The ZIP contains
  `hook.exe`, project license, third-party notices, and three required bundled
  license files. Evidence:
  `Hook/artifacts/release-validation/r8/summary.json`.
- R8 retains the awaited `frontend-initialized` marker and required
  `remote_resources` host capability, then adds a bounded pending-device approval
  wait. Only Loom's structured `device_not_authorized` response is retryable;
  revoked, malformed, and server-failure responses remain terminal.
- Hook now uses Tauri `run_return` and one atomic exactly-once process-exit guard.
  Native acceptance exits through the normal Tauri request path, restores system
  cursor/input state, tears down watchdog/CDP children, and returns exit code 0.
- `Hook/scripts/Invoke-HookNativeCandidateAcceptance.ps1` validates candidate
  SHA-256, real WebView2 CDP plus Tauri IPC, second-instance refusal, a full
  process-tree memory soak, normal exit cleanup, watchdog/CDP teardown, restart,
  isolated app-settings persistence, and the dashboard Surface before and after
  Hook restart.
- `Hook/scripts/Invoke-HookLoomSurfaceCandidateAcceptance.ps1` starts the exact
  packaged R8 daemon on isolated ports/control-plane storage, installs the real
  process framework and dashboard prototype through a fixture Art Store, starts
  the Hook Bridge, then delegates to the native R8 runner. The CDP probe waits
  for the real declarative Surface DOM, clicks `refresh`, requires a higher
  revision and resolved PNG data URL, and cross-checks the daemon attachment,
  succeeded ACK, authoritative state, and formal result.
- The formal 600-second packaged R8 Hook -> packaged R8 Loom run passed. It
  approved the pending device, mounted the real dashboard, advanced Surface
  revision 1 -> 4, resolved the authenticated PNG resource, observed exactly one
  succeeded ACK, and held process-tree private-memory growth to 7,450,624 bytes
  (4.945%) with no violations across 373 samples.
- The first Hook process exited normally with one cleanup record and no remaining
  Hook/watchdog/CDP process. Restart preserved settings and the same instance,
  attachment, and unit identities, then advanced revision 5 -> 8. Final cleanup
  stopped the daemon, fixture store, Bridge, and all three isolated listeners.
  Evidence:
  `Hook/artifacts/runtime-performance/hook-loom-surface-candidate/r8-r8-full-600s/summary.json`.

### Loom R8

Path:
`release/Loom/20260811-distributed-art-surface-r8`

- 45 checksum entries verified.
- `verify-release.ps1 -RunSmoke` passed standalone release, Hook canvas, Hook
  error preview, Framework Art Store Hook, Plugin Boundary, Surface Prototype,
  and Authored Art Creation smokes.
- Release Surface evidence:
  `target/runtime-smoke/surface-prototypes/20260811-182410-surface-prototypes-59448-4a5be59062134766810044915df88fef/summary.json`.
- Release summary:
  `target/runtime-smoke/latest/loom-release-20260811-distributed-art-surface-r8-summary.json`.
- Manifest/provenance record Git head
  `92fb6e11a0dd9577d1d48c41ee3a2e9d595b1ff8` with `gitDirty=true` and
  `sourceGitDirty=true`. R8 is runtime evidence, not a clean-source publication.

## Remaining closure gates

1. Commit Hook and Loom independently while excluding unrelated dirty-worktree
   changes.
2. Build both final candidates from detached clean worktrees at those exact
   commits, verify release contents, and require clean provenance
   (`gitDirty=false` / `sourceGitDirty=false` where supported).
3. Repeat the packaged dual-end gate against the clean-source candidates and
   retain process/listener cleanup evidence.
4. Only after those gates may the architecture document state that phase one is
   fully complete.
