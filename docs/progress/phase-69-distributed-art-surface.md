# Phase 69: Distributed Art Surface phase-one closure

## Status

Phase one is complete. Hook and Loom were committed independently, rebuilt from
detached clean worktrees, verified with clean provenance, and exercised together
through the formal ten-minute native dual-end gate.

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

### Hook clean R9

Path:
`release/Hook/20260811-distributed-art-surface-clean-r9`

- `hook.exe`: 7,218,688 bytes, SHA-256
  `40fb48aa728d70be27846c9c073354fc25f07f2e2ba672e190cedea80a7fe5b9`.
- `hook-windows-x64-V0.1.7.zip`: 3,482,241 bytes, SHA-256
  `b3de3d387faf62eb5cc53d5f2461b83eddf6eb03d7c0ca023f6b8dd34fd0d3a1`.
- CLI version is `hook 0.1.7`; no-GUI self-check status is `ok`.
- Candidate root contains exactly the executable and ZIP. The ZIP contains
  `hook.exe`, project license, third-party notices, and three required bundled
  license files. Evidence:
  `Hook/artifacts/release-validation/clean-r9/summary.json`.
- Source commit is `0e76222e7add3225cc8c9906a6ad1422910079aa`;
  the detached build worktree remained clean before and after the build.
- Clean R9 includes the awaited `frontend-initialized` marker and required
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
  packaged clean R9 daemon on isolated ports/control-plane storage, installs the
  real process framework and dashboard prototype through a fixture Art Store,
  starts the Hook Bridge, then delegates to the native clean R9 runner. The CDP
  probe waits for the real declarative Surface DOM, clicks `refresh`, requires a higher
  revision and resolved PNG data URL, and cross-checks the daemon attachment,
  succeeded ACK, authoritative state, and formal result.
- The formal 600-second packaged clean R9 Hook -> packaged clean R9 Loom run
  passed. It approved the pending device, mounted the real dashboard, advanced Surface
  revision 1 -> 4, resolved the authenticated PNG resource, observed exactly one
  succeeded ACK, and ended 303,104 bytes below the private-memory baseline with
  no positive growth and no violations across 384 samples. Peak private memory
  was 167,219,200 bytes.
- The first Hook process exited normally with one cleanup record and no remaining
  Hook/watchdog/CDP process. Restart preserved settings and the same instance,
  attachment, and unit identities, then advanced revision 5 -> 8. Final cleanup
  stopped the daemon, fixture store, Bridge, and all three isolated listeners.
  Evidence:
  `Hook/artifacts/runtime-performance/hook-loom-surface-candidate/clean-r9-clean-r9-full-600s/summary.json`.

### Loom clean R9

Path:
`release/Loom/20260811-distributed-art-surface-clean-r9`

- 45 checksum entries verified.
- `verify-release.ps1 -RunSmoke` passed standalone release, Hook canvas, Hook
  error preview, Framework Art Store Hook, Plugin Boundary, Surface Prototype,
  and Authored Art Creation smokes.
- `Loom.exe` SHA-256 is
  `8495b17f7cadd9bd92c4afa2c2570fa7fd733b29b0945fce719f91ac602aff47`;
  `runtime/loom-daemon.exe` SHA-256 is
  `2b480d23d9aa750f3baadadf466a29350a913b2c1b154f481467c5a25c5fa920`.
- The desktop ZIP SHA-256 is
  `a41c175ab96204293f64965aafc078f6172e9fc263fefb691e65219b2e3dca25`.
- Manifest/provenance record Git head
  `2daf1f611fefa10a161ee67dcb7984dd66781a54` with `gitDirty=false` and
  `sourceGitDirty=false`.
- Release summary:
  `target/runtime-smoke/latest/loom-release-20260811-distributed-art-surface-clean-r9-summary.json`.
  Consolidated verifier evidence:
  `target/runtime-smoke/phase69-clean-r9/verify-summary.json`.

## Closure result

All ten phase-one acceptance gates are complete. Dirty-source R8/R8 remains
immutable historical runtime evidence; clean R9/R9 is the publication baseline.
The final teardown found no clean-candidate Hook, watchdog, Loom daemon, Art
Store, Bridge, CDP, or isolated listener remaining. The user's pre-existing Loom
R120 process pair retained its original PIDs and executable paths.

One compatibility risk remains outside the phase-one gate: a first verifier run
from a deeply nested worktree produced Windows OS error 267 when a framework
process path reached 301 characters. The same exact clean candidate passed the
complete verifier from a short detached worktree. Extended-length framework
package/control-plane paths therefore remain follow-up hardening; this result is
not relabeled as a product-functional failure or hidden as a passed long-path
case.
