# Phase 79 - Loom/Hook large-file modularization and hardening plan

Date: 2026-08-23

Status: **in progress; Phase 79-A is locally verified and Loom batches 1-24 are locally recorded**

Canonical scope:

- `C:\Users\Public\nas_home\AI\GameEditor\Neuro\Loom`
- `C:\Users\Public\nas_home\AI\GameEditor\Neuro\Hook`
- final release roots:
  - `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom`
  - `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Hook`

Baseline source state:

- Loom commit `03fbe2149f234fbe4e945d53d22b2857562c0dd3`, tagged `发布前的v2`.
- Hook commit `6fca3eae224a6856c829900beb94917826392d07`, tagged `发布前的v2`.
- Both repositories were clean at planning start. This document itself will make the Loom worktree dirty
  until it is reviewed and committed.

This is a structural program, not a mechanical line-cutting exercise. A split is complete only when the
new files have coherent ownership, the old public behavior remains covered, and the resulting modules
have been reviewed for security, resource lifetime, and measured performance.

## 1. Objective

1. Replace oversized handwritten source files with modules organized by responsibility, dependency
   direction, state ownership, and lifecycle.
2. Keep resulting files near 150 effective code lines where that produces a natural module.
3. Enforce 500 effective lines as the normal acceptable ceiling.
4. Permit 501-700 effective lines only when cohesion would be harmed by another split and the reason is
   recorded.
5. Treat 701-1500 effective lines as mandatory refactoring work.
6. Treat more than 1500 effective lines as an unconditional hard-cap violation. A completed refactor
   batch must not leave its target or any newly created file above this cap.
7. Add useful module and invariant comments that were previously missing, without padding files with
   comments that merely restate the code.
8. After each structural split is behaviorally green, audit the resulting files for security defects,
   vulnerabilities, resource/memory leaks, and measurable performance problems.
9. Produce clean-source Loom and Hook release candidates after the entire program passes its final gates.

## 2. Non-goals

- Do not split by arbitrary names such as `part1`, `part2`, `helpers2`, or by equal line ranges.
- Do not change public HTTP, JSON, Tauri IPC, event, package, manifest, or `loom.hook.v1` contracts merely
  to make extraction easier.
- Do not combine unrelated product features with the structural work.
- Do not manually edit generated files, dependency output, lockfiles, compiled artifacts, snapshots, or
  immutable vendored code. Fix their generator or configuration when generated output is the problem.
- Do not merge the two independent Git repositories or add a runtime dependency from either repository
  to the other's source tree.
- Do not call a refactor successful from compilation alone. Contract tests and real release/runtime gates
  remain required.

## 3. Effective-line standard and enforcement

### 3.1 What is counted

The authoritative metric will count lines that contain code after removing:

- blank or whitespace-only lines;
- comment-only lines;
- multiline comment regions that contain no code.

A line containing both code and an inline comment counts as one code line. String and template literal
content remains code; a URL containing `//` is not a comment. The counter must understand the comment and
string forms used by Rust, TypeScript/TSX, JavaScript, PowerShell, Python, CSS, HTML, CMD, and BAT files.

The policy applies to handwritten production code, tests, build/release scripts, and styles. Locally
maintained fork code under Hook's `src-tauri/crates` also counts. Generated output and immutable third-party
source are excluded only through explicit path policy, not because a directory happens to be inconvenient.

### 3.2 Threshold policy

| Effective lines | Meaning | Required action |
| ---: | --- | --- |
| around 150 | preferred module size | Use this as the design target, normally 100-250 lines. |
| 251-500 | acceptable | Keep when the file has one clear responsibility. |
| 501-700 | soft-limit exception | Record why another split would reduce cohesion or obscure an invariant. |
| 701-1500 | mandatory split | No final strict-gate exception. Split on the next owning batch. |
| >1500 | hard-cap violation | No waiver. Continue splitting until every result is at most 700 lines. |

The target is not a demand to turn every cohesive 300-line algorithm into two files. Conversely, a
140-line file with unrelated responsibilities can still be split.

### 3.3 Tooling deliverables

Phase 79-A will add an independent effective-line checker to each repository. The implementations may be
copied initially but must not require a sibling checkout at runtime. Each checker will provide:

- human-readable and JSON reports sorted by effective lines;
- language-aware fixtures proving comment, string, raw-string, template, here-string, and mixed
  code/comment handling;
- explicit include/exclude policy;
- `--ratchet` mode for the migration period;
- `--strict` mode for final and future CI;
- a machine-readable 501-700 exception file containing path, effective lines, responsibility, reason,
  owner, and review date;
- failure when an exception is stale, missing, or no longer matches the file.

Migration ratchet rules:

1. Existing oversized files may appear only in the initial checked-in baseline.
2. No baseline file may grow in effective lines.
3. No new file may exceed 700 effective lines.
4. Once an oversized file is selected for a batch, the batch is not complete while any resulting file is
   above 700; a result above 1500 is never acceptable.
5. The final change removes the migration baseline and enables strict mode.

## 4. Current code-derived baseline

Phase 79-A replaced the planning estimate with repository-owned checker version 1. The baselines are read
from the exact `发布前的v2` commit trees, normalize checkout line endings before hashing, bind the checker,
lexer and policy hashes, and are revalidated against Git blobs during every ratchet run. Generated paths
are excluded only by the reviewed policy files. The following counts are now the authoritative migration
baseline for checker version 1.

| Repository | >1500 hard-cap files | 701-1500 mandatory files | 501-700 review files |
| --- | ---: | ---: | ---: |
| Loom | 16 | 28 | 9 |
| Hook | 7 | 15 | 20 |

### 4.1 Loom hard-cap queue

| Planning effective lines | File | Primary ownership seen in current code |
| ---: | --- | --- |
| ~29,072 | `apps/daemon/src/lib.rs` | daemon configuration, request parsing/routing, all HTTP domains, run lifecycle, executor, startup/shutdown |
| ~7,426 | `apps/desktop/src/App.tsx` | desktop orchestration plus many feature panels and event flows |
| ~6,906 | `apps/desktop/src/styles.css` | global tokens, shell, panels, canvas, Hook, MCP, responsive and state styles |
| ~6,138 | `crates/loom_tool_registry/src/lib.rs` | registry facade, execution, cloud response handling, lifecycle and persistence concerns |
| ~4,284 | `crates/loom_tool_registry/src/install.rs` | archive validation, staging, install, activation, recovery and pruning |
| ~3,779 | `apps/desktop/src-tauri/src/lib.rs` | Tauri bootstrap, daemon lifecycle, commands and packaged catalog behavior |
| ~2,994 | `crates/loom_mcp/src/lib.rs` | MCP configuration, validation, transports, negotiation, calls and cancellation |
| ~2,934 | `apps/daemon/src/surface_actions.rs` | action resolution, queueing, execution and result publication |
| ~2,927 | `crates/loom_tool_registry/src/framework.rs` | framework resolution, readiness, policy and lifecycle |
| ~2,567 | `framework-packages/runtime-host/src/mcp.rs` | runtime-host MCP configuration, bindings, validation and execution |
| ~2,563 | `apps/daemon/src/hook_canvas.rs` | Hook canvas parsing, state, node/edge/crop and transport behavior |
| ~2,344 | `apps/daemon/src/surface_store.rs` | surface document persistence, revisions, operations and JSON-pointer handling |
| ~2,358 | `crates/loom_tool_registry/src/framework_process.rs` | process framework configuration, launch, limits and response handling |
| ~1,678 | `crates/loom_workflow_runtime/src/lib.rs` | workflow orchestration, node execution, evidence and outputs |
| ~1,618 | `apps/desktop/src/services/loomApi.ts` | daemon client types plus interleaved API domains |
| ~1,572 | `crates/loom_mcp/src/package.rs` | package manifest, runtime resolution, integrity and launch preparation |

Known 701-1500 work also includes MCP package handling, Surface protocol/resources, durable run storage,
process/plugin security, workflow storage, Art settings, desktop MCP/Hook components and services, stock
sample runtimes, and large build/verify/smoke scripts. The generated Phase 79-A report, rather than this
prose summary, will be the exhaustive queue.

### 4.2 Hook hard-cap queue

| Planning effective lines | File | Primary ownership seen in current code |
| ---: | --- | --- |
| ~11,882 | `src-tauri/src/lib.rs` | CLI helpers, Tauri bootstrap, state, IPC commands, overlay/input/window/tray lifecycle and integrations |
| ~5,611 | `src-tauri/src/long_capture.rs` | target selection, overlap analysis, stitching, session state and output |
| ~3,442 | `src-tauri/src/loom_hook.rs` | protocol types, handshake, state, listener/transport, actions and resources |
| ~2,538 | `src/components/StickerAnnotationLayer.tsx` | tools, hit testing, edit state, SVG rendering and pointer interaction |
| ~2,121 | `src/app.tsx` | application/event/session/capture/canvas/Talk/Tea/Loom orchestration |
| ~2,044 | `src-tauri/src/screenshot.rs` | capture backends, display selection, HDR/scRGB decisions, conversion and fallback |
| ~1,518 | `src/components/UnitView.tsx` | unit rendering, Art/sticker states, menus and DOM interaction |

Known 701-1500 work also includes `src/services/api.ts`, sticker geometry, selection, large parameter/sticker
components, JavaScript Surface hosting, `app.css`, the locally maintained Windows capture target code, large
contract tests, and real-runtime/acceptance scripts. Phase 79-A will enumerate all remaining files.

Generated Rust files currently found under paths such as `src-tauri/target-test-sync` are build output and
must not enter the queue. Their presence is evidence that exclusion rules need exact tests; it is not a
reason to edit them.

## 5. Refactor rules for every batch

Each batch must follow this order:

1. **Characterize behavior.** Identify public symbols, serialization fields, event/command names, feature
   and `cfg` branches, state ownership, side effects, and resource lifetimes. Add or select focused tests
   before moving behavior.
2. **Design the module graph.** Give every destination one sentence of responsibility and define allowed
   dependency direction. Shared code moves only when at least two real owners need it.
3. **Extract without intentional behavior change.** Preserve the old facade when it is a public boundary;
   use the narrowest visibility (`private`, `pub(super)`, or repository equivalent).
4. **Add meaningful comments.** Document module purpose, cross-module invariants, protocol/security
   assumptions, platform constraints, ownership, and non-obvious performance choices.
5. **Run focused behavior gates.** Compilation alone is insufficient.
6. **Run the effective-line ratchet.** A selected source must finish at 700 or fewer effective lines, with
   a recorded reason if it remains above 500.
7. **Audit the new files one by one.** Perform the security, lifetime, and performance review in Section 9.
8. **Make hardening changes separately.** Keep the structural extraction reviewable independently from
   intentional behavior or performance changes.
9. **Re-run focused and owning-subsystem tests.** Record commands and results in the progress section.

A batch should represent one ownership boundary, not a target number of changed files. Do not continue to
an unrelated large file merely because the current test process is already warm.

## 6. Planned implementation order

### Phase 79-A - Measurement, policy, and ratchet gates

Implementation status: **locally complete; hosted CI execution awaits a future push**.

- Add and test the effective-line checker in Loom and Hook.
- Generate complete baseline JSON reports from the tagged pre-refactor commits.
- Classify every file as production, test, script, style, locally maintained fork, generated, or immutable
  third-party source.
- Add CI ratchet jobs and explicit 501-700 exception schemas.
- Add a per-batch checklist/template for module ownership and post-split hardening evidence.
- Confirm no existing release or source artifact is overwritten or moved.

Exit gate: both repositories reproduce the same counts locally and in CI; generated output is excluded by
tested policy; all current >700 files are present in the queue.

### Phase 79-B - Loom leaf and reusable runtime modules

Start below the daemon integration hub so later daemon extraction depends on stable, smaller libraries.

1. `loom_tool_registry`
   - reduce `lib.rs` to a facade and stable exports;
   - separate registry/state, Art execution, cloud request/response, response limits/redaction, lifecycle,
     and diagnostics;
   - split install into archive intake, containment/validation, staging, activation, recovery/rollback, and
     pruning;
   - split framework resolution/readiness from framework lifecycle and process execution;
   - preserve canonical publisher/id, immutable versions, signature/trust/revocation, source immutability,
     and pre-launch revalidation.
2. `loom_mcp`
   - separate public configuration/types, validators, package resolution, stdio transport, streamable HTTP,
     initialization/version negotiation, session/cancellation, tool listing/call normalization, and tests;
   - keep Windows and Unix spawn/termination `cfg` pairs together with their guards.
3. `loom_workflow_runtime`
   - separate orchestration, node adapters, evidence transitions, output mapping, and error policy.
4. Loom desktop Tauri host
   - separate bootstrap, daemon process ownership, command modules, catalog bootstrap, diagnostics and path
     resolution while preserving the packaged executable layout.

Minimum gates per owning Rust crate: `cargo fmt --all -- --check`, focused `cargo test --locked -p <crate>`,
and compile/test of direct dependents. Run workspace tests at the end of the phase.

### Phase 79-C - Loom daemon and Surface integration hub

1. Convert `apps/daemon/src/lib.rs` into a small composition facade.
2. Extract config/status DTOs, authentication, HTTP parsing/limits, route matching, response encoding,
   executor/queue policy, listener ownership, run lifecycle, shutdown, and domain handler modules.
3. Keep route handlers grouped by capability rather than by HTTP verb alone: runs/brain, packages/Arts,
   MCP, workflows, Hook/Surface, image/OCR, settings/credentials, and health/status.
4. Split `surface_actions.rs` into declaration/binding resolution, validation, scheduling/cancellation,
   execution, evidence, and publication.
5. Split `hook_canvas.rs` into protocol decoding, session/document projection, nodes/edges, crop/image
   handling, and transport.
6. Split `surface_store.rs` into document/revision model, operation application, persistence/transactions,
   JSON-pointer utilities, and stale/CAS policy.
7. Split `surface_resources.rs` by reference validation, fetch/proxy policy, bounded storage/cache, and
   response conversion.

Load-bearing Loom invariants:

- `LoomDaemon` retains sole `TcpListener` ownership; workers receive parsed jobs.
- Health/status probes keep their reserved behavior under queue pressure.
- File-backed control-plane routes retain deterministic serialization.
- HTTP shapes, bearer authentication, capability IDs, run/event ordering, SQLite transactions/recovery,
  and no-replay behavior remain unchanged.
- `loom.hook.v1` and `loom.surface.v1` remain the only current Art/Surface boundaries.

Minimum gates include crate/workspace tests, `apps/daemon/tests/daemon_cli_contract.rs`, focused route and
Surface tests, bounded-concurrency smokes, and packaged daemon lifecycle tests.

### Phase 79-D - Loom desktop frontend, styles, and sample/runtime code

1. Split `loomApi.ts` into a shared transport/error layer plus domain clients. Preserve exported names or
   add a facade so callers do not learn transport details.
2. Reduce `App.tsx` to cross-subsystem composition. Move domain controllers, event subscriptions, page
   state, and major panels into focused hooks/components.
3. Split `styles.css` by explicit cascade layers: tokens/reset, shell/layout, navigation, panels/forms,
   workflow canvas, Hook canvas, MCP/Art surfaces, feedback states, and responsive rules. Preserve import
   order and selector specificity with visual/browser checks.
4. Split large MCP, Hook canvas, workflow studio, marketplace, and sample stock Surface/runtime files by
   their feature boundaries.
5. Split oversized tests by behavior contract, not by arbitrary test count.
6. Extract reusable PowerShell functions only when their inputs/outputs and PowerShell 5.1 behavior can be
   tested independently.

Desktop gates from `apps/desktop`: `npm test`, `npm run typecheck`, `npm run build`, and
`cargo check --locked --manifest-path src-tauri/Cargo.toml`, plus relevant browser/runtime smokes.

### Phase 79-E - Hook pure frontend services and components

Start with pure logic and stable facades before touching event-order-sensitive integration code.

1. Split `src/services/api.ts` into a shared `safeInvoke`/browser fallback transport and domain clients for
   boot/settings, capture, session/history, overlay/window, Loom/Surface, Talk/voice, Tea, image/clipboard,
   and resources. Preserve command names, arguments, return types, and fallback semantics.
2. Split sticker geometry/editing/export modules into pure geometry primitives, per-shape transforms/hit
   tests, bounds/indexing, serialization, raster/export, and history/propagation boundaries.
3. Split `StickerAnnotationLayer.tsx` into controller/state, tool handlers, selection/gizmo overlays, and
   focused renderers for freehand, line/arrow, shape, mosaic/blur, and text where applicable.
4. Split `UnitView.tsx`, parameter panels, top strips/property bars, shader preview, and JavaScript Surface
   hosting into state controllers and focused views. Protocol validation and resource budgets stay outside
   presentational components.
5. Reduce `src/app.tsx` to application composition and explicit subsystem hooks for Tauri listeners,
   session restore, capture lifecycle, overlay input, shortcuts, Loom/Talk/Tea, and canvas coordination.
6. Split `app.css` along the same ownership boundaries as the resulting components while preserving
   cascade order and runtime visual behavior.

Required frontend invariants:

- persistent graph edits pass through graph-store actions;
- transient drag/edit previews do not write the graph per input sample;
- native overlay drag samples remain authoritative during active drag;
- all Tauri/event/browser listeners, timers, animation frames, object URLs, sockets, and abort controllers
  have explicit cleanup paths;
- Surface event validation and resource budgets remain fail-closed.

Minimum gates: focused Vitest files, `npm run lint`, `npm run typecheck`, `npm run typecheck:test`, full
`npm test`, Surface browser smoke, and production frontend build.

### Phase 79-F - Hook native capture and integration modules

1. Split `long_capture.rs` into target/focus discovery, axis/direction model, overlap analysis, stitch
   planning, pixel composition, session state, and output encoding.
2. Split `screenshot.rs` into backend selection, display/target selection, Windows Graphics Capture, SDR
   fallback, HDR/scRGB analysis, pixel conversion, and output metadata.
3. Split `loom_hook.rs` into protocol DTOs, handshake/capability negotiation, state, listener/transport,
   action routing, resource handling, and error redaction.
4. Reduce `src-tauri/src/lib.rs` to Tauri composition and an auditable command registry. Extract CLI/self
   check, managed state, app bootstrap, overlay/window lifecycle, native input queue, tray/shortcuts,
   capture commands, session/settings/history commands, and local-service bridge commands.
5. Refactor large locally maintained capture/drag platform files and Rust contract tests after their owning
   native modules are stable.

Load-bearing Hook invariants:

- Tauri command names, signatures, argument casing, serde fields, managed state, and event names/payloads
  remain compatible.
- Replaceable move samples may be coalesced; Down/Up/key/Escape edges remain ordered.
- physical global, logical monitor, WebView client, scale factor, and negative-origin coordinate spaces are
  never mixed implicitly.
- drag release commits graph position once and all cancel/blur/watchdog paths clear transient/GPU state.
- HDR/scRGB and SDR output contracts remain distinct; long capture remains SDR by design.
- Windows-specific symbols do not escape their `cfg(windows)` guards, and paired Unix behavior remains
  compilable where present.

Minimum native gates: `cargo fmt --check --manifest-path src-tauri/Cargo.toml`, focused Rust tests,
`cargo test --manifest-path src-tauri/Cargo.toml`, capture/overlay/DPI/drag contract suites, and built EXE
`--version`/`--self-check` checks.

### Phase 79-G - Remaining 701-1500 files and 501-700 decisions

- Process every remaining generated queue item, including tests, scripts, styles, sample runtimes, and
  locally maintained fork files.
- Split 701-1500 files unconditionally.
- For every 501-700 file, either split it or add a reviewed exception explaining its single responsibility,
  why another boundary is harmful, and which tests protect it.
- Review 150-500 files for responsibility mixing, but do not split them solely to approach 150 lines.
- Replace repeated script scaffolding with tested helper modules only when PowerShell 5.1, error propagation,
  encoding, process exit, and cleanup semantics remain explicit.
- Split tests by contract/scenario and keep fixture builders separate from assertions when this improves
  ownership. Do not weaken assertions to reduce file size.

Exit gate: strict effective-line mode passes in both repositories; there are no files above 700 and every
501-700 file has a valid exception.

## 7. Comment and documentation policy

Every newly created source module should begin with a short purpose comment when the language supports it.
Additional comments are expected only for information not obvious from names and types:

- public/protocol ownership and compatibility promises;
- security trust boundaries and validation order;
- state ownership, transaction boundaries, and lock ordering;
- resource acquisition/release and cancellation behavior;
- platform-specific `cfg`, DPI, COM, process-tree, or PowerShell 5.1 constraints;
- non-obvious performance choices, budgets, queue bounds, and cache invalidation;
- why two similar-looking paths intentionally remain separate.

Do not add narration such as "increment counter" or comments whose only purpose is lowering the apparent
effective-line ratio. Comments are excluded from the metric but remain reviewable content.

## 8. Behavior-preservation proof

Before extraction, record for each target:

- public exports and visibility;
- HTTP routes, Tauri commands, events, JSON/serde fields, schema and protocol versions;
- environment variables, CLI flags, file layouts and path resolution;
- initialization/shutdown order and resource owners;
- error codes/messages that are asserted or user-facing;
- current focused tests and missing characterization tests;
- hot paths and existing benchmark/performance gates.

After extraction, compare these inventories. A facade may re-export moved symbols, but compatibility layers
for dead or obsolete behavior must not be introduced.

## 9. Post-split per-file hardening

The following review happens only after the structural extraction is green. Findings must cite the exact
new file and symbol and end in a fix, a regression test, or an explicitly justified non-issue.

### 9.1 Security and vulnerability review

- Validate all external input at the owning boundary: HTTP/body/header, IPC/event, JSON/serde, package ZIP,
  path, URL, image/resource, environment, CLI and plugin/MCP data.
- Check path traversal, symlink/reparse-point escape, archive bombs, unsafe extraction, TOCTOU, permission
  checks, canonical identity/digest/signature verification, and atomic activation.
- Check command/process arguments and environment construction for injection, secret inheritance, unsafe
  shell use, and incomplete process-tree termination.
- Check credentials and tokens for exact, encoded, nested, log, error, response, and persistence leakage.
- Check Surface/browser code for script/HTML/URL injection, origin/source validation, CSP assumptions,
  message-port validation, resource budgets and stale-revision handling.
- Audit Rust `unsafe`, FFI/COM handles, integer conversions, byte lengths, image dimensions and allocation
  arithmetic.
- Confirm fail-closed behavior for signing/UIAccess, trust, revocation, capability and permission gates.

### 9.2 Memory and resource lifetime review

- Rust: process/thread joins, channel shutdown, socket/response bodies, file/temp handles, SQLite
  transactions, locks, COM/D3D resources, image buffers, audio sessions and cancellation.
- Frontend: event listener unsubscription, WebSocket/MessagePort closure, timers/RAF cancellation,
  AbortController use, object/Blob URL revocation, DOM/GPU follower cleanup and stale async work.
- Bound queues, caches, response bodies, decoded strings, image dimensions, concurrent work and retry state.
- Measure peak representations where bytes are decoded and parsed; avoid simultaneous full-size byte,
  UTF-16 string and parsed-object copies when a bounded/streaming representation is practical.
- Add teardown and repeated-use tests for suspected leaks; do not claim a leak fixed from code inspection
  alone when runtime measurement is feasible.

### 9.3 Performance review

- Establish a before/after measurement for hot paths; do not make speculative micro-optimizations.
- Inspect lock scope, serialization copies, repeated parsing/validation, filesystem scans, process/MCP
  reconnection, SQLite transaction width and synchronous work on UI/request threads.
- Preserve bounded daemon backpressure and probe responsiveness.
- Preserve Hook's no-per-move graph-write rule, move-event coalescing, single release commit, incremental
  indices and GPU cleanup.
- Check component subscriptions and derived state so extraction does not broaden rerenders.
- Run existing performance gates and add focused thresholds only when stable and meaningful.

## 10. Validation matrix

### 10.1 Per-batch minimum

- effective-line ratchet;
- official formatter/checker for touched languages;
- focused unit/contract tests for the extracted responsibility;
- direct dependent compile/typecheck;
- security/lifetime/performance regression tests added by the hardening pass;
- `git diff --check` and a scoped review of changed files.

### 10.2 Loom phase/final gates

```powershell
rtk cargo fmt --all -- --check
rtk cargo test --workspace --locked

Push-Location apps/desktop
rtk npm test
rtk npm run typecheck
rtk npm run build
rtk cargo check --locked --manifest-path src-tauri/Cargo.toml
Pop-Location
```

Run the relevant repo-owned smoke scripts for daemon concurrency, MCP, framework/Art, Hook Canvas,
Surface browser/runtime and release behavior. A smoke is successful only when its semantic assertions pass,
not merely when a process exits zero without evidence.

### 10.3 Hook phase/final gates

```powershell
rtk npm run lint
rtk npm run typecheck
rtk npm run typecheck:test
rtk npm test
rtk npm run test:surface-browser
rtk cargo fmt --check --manifest-path src-tauri/Cargo.toml
rtk cargo test --manifest-path src-tauri/Cargo.toml
rtk npm run build
```

Also run targeted overlay ordering, DPI/coordinate, real-drag preference, capture targeting/focus, long
capture, sticker edit/annotation, Surface protocol/store/resource, release provenance/signing, and built EXE
contract tests. `npm run verify:local` remains the final aggregate local gate.

## 11. Clean-source release closure

After all source, tests, comments, tooling and documentation are committed coherently in each independent
repository:

1. Require empty `git status --porcelain --untracked-files=all` in Loom and Hook.
2. Use new, unused version IDs. Never overwrite existing release directories.
3. Build Loom into `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom` with
   `scripts/build-release.ps1 -RequireCleanSource`.
4. Run `scripts/verify-release.ps1 -RunSmoke -RequireCleanSource` against that exact new package directory.
5. Build Hook into a new directory below
   `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Hook` with
   `scripts/build-local-hook-exe.ps1 -RequireCleanSource`.
6. Package the matching Hook executable and provenance with `scripts/package-release-zip.ps1`; run the
   built EXE `--version` and `--self-check` contracts.
7. Verify both provenance records contain the final reviewed commit, `gitDirty=false`, correct artifact
   digests and expected payload containment.
8. Preserve all prior release candidates unchanged.

The final clean-source releases may also supply the still-needed formal release evidence described by the
old Phase 77-78 R13/F10 record, but R13 must not be declared complete until these new artifacts actually
exist and pass their fresh gates.

## 12. Completion criteria

Phase 79 is complete only when all of the following are true:

- [ ] Both repositories have tested effective-line tooling and strict CI enforcement.
- [ ] No handwritten source, test, script, style or locally maintained fork file exceeds 700 effective
      lines.
- [ ] No file exceeds the 1500-line hard cap under any exception mechanism.
- [ ] Every 501-700 file has a current, specific cohesion justification.
- [ ] Major integration files are composition facades rather than hidden algorithm stores.
- [ ] Newly created modules have clear purpose/invariant comments without comment padding.
- [ ] Public protocol, HTTP, IPC, event, serialization, package and release contracts remain covered.
- [ ] Each resulting module has completed security, vulnerability, memory/resource and performance review.
- [ ] All confirmed high/critical security defects and resource leaks are fixed with regression evidence.
- [ ] Performance-sensitive paths have no unexplained regression against existing or added measurements.
- [ ] Loom's full Rust/desktop/smoke/release gates pass from clean source.
- [ ] Hook's full frontend/browser/Rust/runtime/release gates pass from clean source.
- [ ] New verified artifacts exist under the required Loom and Hook release roots with final commit
      provenance and `gitDirty=false`.
- [ ] Actual commands, results, residual 501-700 exceptions and release paths are appended to this document.

## 13. Progress log

### 2026-08-23 - Phase 79-A local implementation

- Added independent `scripts/effective-code-lines.mjs` and
  `scripts/effective-code-lines-lexer.mjs` implementations to Loom and Hook. The scanner and lexer are
  separate responsibilities and remain below 500 effective lines each.
- Added 15 Node regression tests in each repository for Rust nested comments/raw strings/lifetimes,
  JavaScript regex/template literals, PowerShell comments/here-strings, Python docstrings/triple strings,
  CSS/HTML/CMD comments, UTF-8/BOM/CRLF, path containment, migration ratchet behavior, strict thresholds,
  exception staleness/expiry, generated-path exclusion and deterministic source hashes.
- Added policy, empty exception registry and Git-tree-derived baseline JSON files to both repositories.
  Baselines are bound to the exact pre-refactor commits and trees plus checker, lexer and policy hashes.
- Loom baseline: 53 files above 500 effective lines: 16 above 1500, 28 from 701-1500 and 9 from 501-700.
- Hook baseline: 42 files above 500 effective lines: 7 above 1500, 15 from 701-1500 and 20 from 501-700.
- Added ratchet/report execution to Loom Windows CI and Hook build/release workflows. Added workflow contract
  assertions and Hook npm commands; Hook `verify:local` now includes checker tests and ratchet enforcement.
- Fresh local evidence:
  - Loom checker tests: 15 passed; ratchet passed; GitHub Actions contract passed.
  - Hook checker tests: 15 passed; ratchet passed; build/release workflow contracts: 5 passed.
- Phase 79-B subsequently started with `framework_process.rs`; see the next entry. No Phase 79 release has
  been built. Hosted CI remains unobserved until these changes are committed and pushed.

### 2026-08-23 - Phase 79-B batch 1: framework process boundary

- Split `crates/loom_tool_registry/src/framework_process.rs` from 2,358 effective lines into a 48-line
  facade plus responsibility modules for execution, persistent hosts, image output validation, package/MCP
  resolution, candidate projection and credential redaction. Tests are independently split into fixture,
  request/credential, runtime/error, host, candidate and redaction modules.
- Every resulting file is below 500 effective lines. The largest production modules are `host.rs` at 443
  and `execute.rs` at 429; the largest test module is `execution_runtime.rs` at 255. No 501-700 exception
  was added.
- Preserved the three public `framework_process` execution entry points through facade re-exports and kept
  direct process arguments out of a shell.
- Fixed four confirmed boundary/resource defects with regression evidence:
  - canonicalize the manifest command and reject symlink/junction resolution outside its canonical package;
  - enforce the four-host limit process-wide even though non-`Send` Windows job ownership requires each
    reusable host to remain in its originating thread-local pool;
  - atomically create each request temp leaf so a pre-existing directory is never adopted and later removed;
  - redact granted credential values from framework-controlled process/protocol error text.
- The normal installer rewrites Art `dir`, `stateDir`, `cacheDir` and `outputDir` to control-plane-owned
  locations. Execution still accepts legacy/local-authoring external package metadata; tightening that
  compatibility boundary requires an explicit trusted-authoring distinction during the later install and
  registry split rather than an unreviewed path-policy change in this batch.
- Fresh local evidence:
  - `cargo test -p loom_tool_registry framework_process`: 25 passed;
  - `cargo test -p loom_tool_registry`: 196 passed across two suites;
  - `cargo check -p loom-daemon`: passed;
  - `cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml`: passed;
  - effective-line ratchet: passed over 188 files; remaining Loom queue is 15 above 1,500, 28 from
    701-1,500 and 9 from 501-700;
  - `cargo fmt --all` and `git diff --check`: passed.
- No release was built for this individual batch; Phase 79 remains in progress.

### 2026-08-23 - Phase 79-B batch 2: Art installer boundary

- Split `crates/loom_tool_registry/src/install.rs` from 4,284 effective lines into a 46-line facade and
  responsibility modules for shared types, manifest metadata, filesystem safety, installation, resolution,
  integrity, activation/rollback, MCP dependencies, uninstall, lockfiles, binaries, recursive dependencies
  and package authoring. The test body is split into fixtures, manifest/authoring, core install, dependency,
  package, activation and recovery modules.
- Every resulting installer file is below 500 effective lines. The largest production modules are `mcp.rs`
  at 348, `core.rs` at 324 and `integrity.rs` at 315; the largest test module is `install_core.rs` at 388.
  The current `framework_process` maximum is `host.rs` at 459 after adding the lightweight slot-limit test
  helper. No 501-700 exception was added.
- Preserved the installer facade's public entry points through explicit re-exports rather than retaining a
  compatibility implementation beside the new modules.
- Fixed two confirmed high-risk filesystem/integrity defects with regression evidence:
  - reinstall now recomputes an existing immutable version directory's canonical digest and rejects modified
    content instead of reusing it solely because its manifest remains readable;
  - permission and recursive cleanup reject symbolic links and Windows reparse points, and lifecycle,
    tombstone and version-retention scans no longer traverse linked directories.
- Reworked the process-wide host-limit regression to reserve the real atomic slots without launching four
  simultaneous PowerShell processes. This keeps the production limit covered while avoiding test-created
  CPU/process pressure that caused unrelated process timeout and wall-time tests to interfere in parallel.
- Residual boundaries recorded for later batches: filesystem path checks still have ordinary path-based TOCTOU
  limits without handle-relative platform APIs; Art lifecycle operations do not yet have a per-Art interprocess
  lock; post-commit version-prune errors remain non-fatal to an otherwise successful install.
- Fresh local evidence:
  - the two new security regressions passed independently;
  - `cargo test -p loom_tool_registry install::tests`: 38 passed;
  - `cargo test -p loom_tool_registry framework_process::tests`: 25 passed;
  - `cargo test -p loom_tool_registry`: 198 passed across two suites;
  - `cargo check -p loom-daemon`: passed;
  - `cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml`: passed;
  - effective-line ratchet: passed over 209 files; remaining Loom queue is 14 above 1,500, 28 from
    701-1,500 and 9 from 501-700;
  - `cargo fmt --all -- --check` and `git diff --check`: passed.
- No release was built for this individual batch; Phase 79 remains in progress.

### 2026-08-23 - Phase 79-B batch 3: framework registry boundary

- Split `crates/loom_tool_registry/src/framework.rs` from 2,927 effective lines into a 70-line facade and
  responsibility modules for models, permissions, readiness, registry queries/state, lifecycle mutations,
  storage safety, dependency locks, catalog acquisition and package runtime validation. Tests are split into
  fixture, catalog, lifecycle, recovery and policy suites.
- Every resulting framework file is below 500 effective lines. The largest production modules are
  `registry_core.rs` at 464, `registry_mutation.rs` at 422 and `storage.rs` at 255; the largest test module is
  `lifecycle.rs` at 353. No 501-700 exception was added.
- Preserved the original `framework` public API with explicit facade re-exports; private implementation
  modules are not exposed as compatibility surfaces.
- Fixed confirmed integrity, path and memory-safety defects with regression evidence:
  - reinstall recomputes and compares an existing immutable framework version's canonical digest before reuse;
  - staging extraction atomically creates a fresh directory, never deletes a pre-existing collision and only
    cleans a staging tree owned by the current operation;
  - permission changes, cleanup, lifecycle/tombstone scans, active-version resolution, readiness and rollback
    reject symbolic links and Windows reparse points instead of traversing package storage boundaries;
  - readiness validates the canonical executable containment boundary rather than accepting `is_file()` alone;
  - persisted framework state rejects invalid package keys, and unresolved invalid references use a fixed
    in-root fallback rather than incorporating attacker-controlled path components;
  - manifests, activation/lifecycle state, framework state, lockfiles and catalog sidecars use a 4 MiB bounded
    non-link reader; local framework ZIP reads remain bounded at 32 MiB even if the file grows during the read.
- Resource/performance review found no framework self-test process or pipe-handle leak: the shared process runner
  kills and waits on isolation, cancellation, timeout and output-limit failures. Canonical package hashing is
  deliberately retained at trust/activation boundaries and is bounded by the package-security limits, although
  status queries and install/rollback can still repeat full-tree hashing and should only be optimized with
  measurement plus a safe immutable-content cache.
- Residual boundaries recorded for later batches: path-based validation retains ordinary TOCTOU windows without
  handle-relative platform APIs; framework lifecycle writes do not yet have a per-framework interprocess lock;
  fixed `.json.tmp` names serialize poorly across multiple processes; status construction repeats state,
  manifest, trust and digest reads.
- Fresh local evidence:
  - `cargo test -p loom_tool_registry framework::tests`: 40 passed;
  - `cargo test -p loom_tool_registry`: 206 passed across two suites;
  - `cargo check -p loom-daemon`: passed without warnings;
  - `cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml`: passed;
  - effective-line ratchet: passed over 224 files; remaining Loom queue is 13 above 1,500, 28 from
    701-1,500 and 9 from 501-700;
  - `cargo fmt --all -- --check` and `git diff --check`: passed.
- No release was built for this individual batch; Phase 79 remains in progress.

### 2026-08-23 - Phase 79-B batch 4: tool registry facade and execution boundaries

- Split `crates/loom_tool_registry/src/lib.rs` from 6,138 effective lines into a small public facade and
  responsibility modules for public models/errors, validation, persistent registry I/O, execution dispatch,
  MCP session/schema handling, cloud transport/request templates, response normalization, image candidate
  traversal/download and focused test fixtures/suites. The public module names, re-exports, error enum,
  serde models and execution function signatures remain available from the crate root.
- Removed the comparison-only copy of the original giant file after the unchanged structural test boundary
  passed. The ratchet report contains no new `tool_registry` implementation or test file above 500 effective
  lines, so this batch needs no 501-700 cohesion exception.
- Post-split security and resource hardening:
  - direct cloud image responses accept only the supported raster MIME allowlist and must also have a
    recognized raster byte signature; SVG and a response that merely claims `image/png` are rejected, and
    the emitted MIME type comes from the bytes rather than the header;
  - cloud JSON data URLs and raw base64 are bounded, decoded and identified from bytes before becoming image
    content. Unsupported/malformed/SVG payloads fall through as text instead of reaching the canvas as an
    active or broken image; AVIF signature recognition was added to keep the allowlist internally coherent;
  - direct MCP call errors now bound every process/server-controlled string even when tool listing succeeded;
  - `tools.json` reads stop at 4 MiB plus one detection byte before UTF-8 conversion or JSON parsing, removing
    the previous unbounded file allocation.
- Per-file lifetime/performance audit:
  - binary cloud images retain streaming base64 encoding and add only a 12-byte signature buffer; no complete
    raw-image copy was introduced;
  - cloud response and MCP image payload ceilings remain 64 MiB and 32 MiB respectively; JSON base64
    validation necessarily decodes then re-encodes a bounded payload;
  - the thread-local MCP session pool remains capped at eight sessions. Its 60-second idle eviction runs on
    subsequent pool access and all sessions drop at thread exit; a timer-owned eager eviction mechanism would
    change runtime lifecycle ownership and is recorded as residual design work rather than mixed into this
    behavior-preserving split.
- Fresh local evidence:
  - structural baseline before hardening: `cargo test -p loom_tool_registry` passed 206 tests;
  - final `cargo test -p loom_tool_registry`: 210 passed across two suites;
  - `cargo check -p loom-daemon`: passed;
  - `cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml`: passed;
  - effective-line ratchet: passed over 252 files; remaining Loom queue is 12 above 1,500, 28 from
    701-1,500 and 9 from 501-700;
  - `cargo fmt --all -- --check` and `git diff --check`: passed.
- No release was built for this individual batch; Phase 79 remains in progress.

### 2026-08-23 - Phase 79-B batch 5: MCP configuration and transport boundaries

- Split `crates/loom_mcp/src/lib.rs` from 2,994 effective lines into a three-line crate facade and
  responsibility modules for public configuration/errors, validation, Windows spawn construction, protocol
  negotiation, common client dispatch, stdio transport, runtime configuration, streamable HTTP transport,
  bounded HTTP response parsing and secret-safe diagnostics. Tests are split by configuration, protocol,
  stdio, HTTP and platform fixture ownership.
- Preserved the crate-root public API through facade re-exports and updated the subprocess fixture's exact test
  path. The behavior-preservation boundary passed the same 55 tests before and after the structural extraction.
- Every resulting MCP facade, implementation and test file is below 500 effective lines. The largest test file
  is `http_fixtures.rs` at 416; the largest production files are `stdio.rs` at 344 and `validation.rs` at 343.
  No 501-700 cohesion exception was added.
- Post-split security, resource and performance hardening:
  - stdio response delivery now uses a bounded four-message synchronous channel, rejects more than 32 malformed
    JSON lines per request and clamps a caller-provided zero timeout to one millisecond;
  - both transports validate tool identifiers and serialized argument size before sending a call;
  - HTTP response allocation checks `Content-Length` before reserving memory, retains the streaming 8 MiB body
    ceiling and limits JSON/SSE response collections to 256 messages;
  - outbound request errors strip reqwest URL context, remote-policy diagnostics retain only the endpoint
    origin, and configured header/environment values are redacted from bounded HTTP bodies and stdio stderr.
- Per-file lifetime/performance audit confirmed that `ManagedChild` termination kills and waits for the child;
  the pipe reader threads then terminate at EOF or receiver disconnect. Residual boundaries are explicit:
  outbound DNS validation still has a rebinding window until the HTTP connector can pin the approved address;
  pipe readers have no join handles and rely on child teardown closing the pipes; JSON arrays can allocate
  within the existing 8 MiB byte ceiling before the 256-message semantic check; credential-reference binding
  integrity remains owned by the later daemon/configuration boundary.
- Fresh local evidence:
  - structural baseline before hardening: `cargo test -p loom_mcp` passed 55 tests across two suites;
  - final `cargo test -p loom_mcp`: 62 passed across two suites;
  - direct dependent `cargo test -p loom_tool_registry`: 210 passed across two suites;
  - `cargo check -p loom-daemon`: passed;
  - `cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml`: passed;
  - effective-line ratchet: passed over 273 files; remaining Loom queue is 11 above 1,500, 28 from
    701-1,500 and 9 from 501-700;
  - `cargo fmt --all -- --check` and `git diff --check`: passed.
- No release was built for this individual batch; Phase 79 remains in progress.

### 2026-08-23 - Phase 79-B batch 6: MCP package lifecycle boundary

- Split `crates/loom_mcp/src/package.rs` from 1,572 effective lines into responsibility modules for public
  manifest/state models, errors, staged installation, manifest validation, trust enforcement, bounded archive
  intake, runtime configuration projection, active-state persistence, tree digests, pre-spawn verification,
  contained uninstall and path/lifecycle primitives. Tests are grouped by fixtures, validation, install,
  trust, integrity, archive and post-split hardening ownership.
- Preserved the public `loom_mcp::package` constants, serde models, error type and install/read/verify/uninstall
  functions. The behavior-preservation boundary passed the same 20 focused package tests before and after the
  structural extraction.
- Every resulting package implementation and test file is near the preferred target and below 500 effective
  lines. The largest files are `tests/integrity.rs` at 165, `tests/install.rs` at 147,
  `tests/trust.rs` at 146 and production `validation.rs` at 142. No 501-700 cohesion exception was added.
- Post-split security, resource and performance hardening:
  - package install/uninstall mutations are serialized process-wide, while staging directory leaves and
    `active.json` temporary files are claimed atomically with bounded `create`/`create_new` retries;
  - existing control-plane directory components, installed version roots and uninstall targets reject symbolic
    links and Windows reparse points before writes, tree traversal or recursive removal;
  - manifest and active-state reads are limited to 1 MiB and 8 MiB respectively, reject linked files and use a
    detection byte after the metadata precheck rather than trusting a potentially stale file size;
  - per-file SHA-256 now streams through a 64 KiB buffer instead of allocating every extracted file in full;
    temporary names add a process-local sequence to the existing process/time components.
- Per-file audit residuals remain explicit: the lifecycle mutex does not coordinate a second Loom process;
  path checks, digest verification, cleanup/uninstall and verify-then-spawn still have ordinary path-based
  TOCTOU windows until handle-relative/no-follow platform APIs and handle-bound execution are designed;
  `active.json` and the installed tree share the same local-writer trust boundary; POSIX state replacement does
  not fsync the parent directory; archive declaration checks and the shared secure extractor each parse the ZIP;
  state serialization clones a bounded map of at most 4,096 files.
- Fresh local evidence:
  - structural package boundary: `cargo test -p loom_mcp package::tests` passed the same 20 tests before and
    after extraction;
  - final package boundary: 23 tests passed, including new streaming-digest, oversized-state and concurrent
    install regressions;
  - final `cargo test -p loom_mcp`: 65 passed across two suites;
  - direct dependent `cargo test -p loom_tool_registry`: 210 passed across two suites;
  - `cargo check -p loom-daemon`: passed;
  - `cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml`: passed;
  - effective-line ratchet: passed over 293 files; remaining Loom queue is 10 above 1,500, 28 from
    701-1,500 and 9 from 501-700;
  - `cargo fmt --all -- --check` and `git diff --check`: passed.
- No release was built for this individual batch; Phase 79 remains in progress.

### 2026-08-23 - Phase 79-B batch 7: workflow execution and resource boundaries

- Split `crates/loom_workflow_runtime/src/lib.rs` from 1,678 effective lines into a two-line public facade and
  responsibility modules for API entry points, errors, dispatch context, stored models, graph validation,
  orchestration, node execution, child resolution, input bindings, preview policy, output selection, bounded
  image intake and retained-result accounting. Tests are grouped by fixtures, execution, child resolution,
  bindings, preview/formal output, local paths and post-split hardening concerns.
- Preserved the crate-root execution functions, `workflow_node_tool_ids`, error/result types and preview/formal
  output behavior. The structural boundary passed the same 16 tests before and after extraction, before any
  security behavior was changed.
- Every resulting workflow runtime implementation and test file is below 500 effective lines. The largest
  production files are `validation.rs` at 223 and `image.rs` at 156; the largest test files are
  `preview.rs` at 211, `bindings.rs` at 194 and `hardening.rs` at 176. The related
  `loom_image_io/src/lib.rs` remains below the normal ceiling at 290. No 501-700 exception was added.
- Post-split security and resource hardening:
  - public entry points carry a per-execution workflow stack, reject recursive workflow IDs and cap nested
    workflow depth at 32;
  - stored workflow YAML is limited to 4 MiB and validates node count, identifiers, duplicate IDs, dependency
    references, duplicate/self dependencies, parameter count, parameter value depth and total value elements
    before scheduling;
  - workflow bindings are bounded and reject unsupported kinds, missing nodes, invalid fields, duplicate node
    targets and invalid primary/preview output policies instead of silently ignoring malformed configuration;
  - stored workflow `meta.src`/`previewSrc` accepts only bounded inline image data, so persisted metadata cannot
    turn an ambient local path into a filesystem-read capability;
  - explicit caller-provided local images are opened once with no-follow semantics (`O_NOFOLLOW` on Unix and
    `FILE_FLAG_OPEN_REPARSE_POINT` on Windows), validated from the opened handle, read through a 32 MiB bounded
    stream and encoded only when their bytes identify a browser-renderable raster container. This removes the
    prior check/reopen race and avoids unbounded compressed-image decode surfaces;
  - node results are measured without serializing another copy, limited to 64 MiB and JSON depth 128 per node,
    and retained workflow results are capped at 256 MiB.
- Per-file lifetime/performance audit: the scheduler remains deterministic and sequential. Its repeated ready
  scan is O(V squared plus E), but graph validation now caps V at 256 and bindings at 512. Remaining deliberate
  costs are bounded but material: image strings can be cloned into the three compatibility argument aliases;
  output selection clones the selected bounded result; locked-child fallback can deserialize each bounded
  registry entry; native image processing cannot observe deadline/cancellation while inside its synchronous
  call; preview callbacks run inline on the execution thread.
- Residual boundaries remain explicit: an explicit caller argument still has local-path read authority because
  local image-path input is a retained public contract and no caller-owned allowlist/root is available here;
  `WorkflowStore::load_workflow` allocates its returned string before this crate applies the 4 MiB check; a child
  tool can allocate a large response before the runtime measures and rejects it; total transient input/base64
  cloning is not yet budgeted independently from retained results. Those ownership changes require API/runtime
  design rather than being mixed into this split.
- Fresh local evidence:
  - structural workflow boundary: `cargo test -p loom_workflow_runtime` passed the same 16 tests before and
    after extraction;
  - final `cargo test -p loom_workflow_runtime`: 22 passed across two suites;
  - `cargo test -p loom_image_io`: 8 passed across two suites;
  - `cargo check -p loom-daemon`: passed;
  - `cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml`: passed;
  - effective-line ratchet: passed over 315 files; remaining Loom queue is 9 above 1,500, 28 from
    701-1,500 and 9 from 501-700;
  - `cargo fmt --all -- --check` and `git diff --check`: passed.
- No release was built for this individual batch; Phase 79 remains in progress.

### 2026-08-23 - Phase 79-B batch 8: desktop Tauri host boundary

- Split `apps/desktop/src-tauri/src/lib.rs` from 3,779 effective lines into a two-line crate facade and
  responsibility modules for application bootstrap/tray ownership, daemon process lifecycle, command adapters,
  diagnostics, runtime configuration, Hook and Loom caches, JSON/binary loopback transport, bounded HTTP
  response parsing, package verification, framework fallback and packaged MCP/Art bootstrap. The original inline
  tests are grouped by fixtures, lifecycle, cache/commands, package checks, framework/Art bootstrap and
  transport/path contracts.
- Preserved the exact Tauri command registry, command names, camel-case argument shapes, public runtime models,
  packaged icon and daemon paths, and crate-root `run` entry point. The structural boundary passed the same 38
  Rust tests before and after extraction, before the hardening behavior changed.
- All 25 resulting Rust files are below 500 effective lines. The largest production file is
  `package_bootstrap.rs` at 413, followed by `daemon.rs` at 319; the largest test file is
  `tests/commands_cache.rs` at 379. The crate facade is two effective lines. No 501-700 exception was added.
- Post-split security, resource and responsiveness hardening:
  - settings, session, package catalogs, migration markers, checksums, auth tokens and package ZIPs now use
    bounded regular-file reads opened once with Unix `O_NOFOLLOW` or Windows reparse-point refusal. Package
    growth after metadata inspection is detected by a one-byte-over-limit read, and migration marker writes
    refuse linked/non-regular targets;
  - packaged framework upgrade IDs reject traversal and non-portable path characters before local package
    lookup; framework, Art and MCP ZIP verification retains exact filename and SHA-256 binding;
  - destructive cache roots must be absolute leaf paths without `.`/`..`, links, reparse points, protected-root
    containment or non-directory targets. Loom pruning validates every root, and Hook/Loom recursive snapshots
    and clear operations run through blocking workers instead of occupying a Tauri command thread;
  - JSON requests accept only the four used HTTP methods and normalized absolute daemon paths. Serialized
    requests are capped at 96 MiB to retain the 64 MiB MCP package-plus-base64 contract; JSON responses are
    capped at 16 MiB, binary previews at 32 MiB and response headers at 64 KiB;
  - the shared response parser strictly accepts HTTP/1.0 or HTTP/1.1 three-digit statuses, validates a single
    `Content-Length`, rejects unsupported transfer encoding and enforces body/header bounds. Successful JSON
    responses reject declared non-JSON media types;
  - loopback base URLs reject credentials, paths, queries, fragments and port zero. Preview MIME is derived from
    PNG/JPEG/GIF/WebP/BMP/AVIF bytes rather than a daemon-controlled header; SVG/HTML and unsupported payloads
    are rejected, including AVIF compatible-brand handling.
- Per-file lifetime/performance audit: binary response ownership uses `Vec::split_off`, avoiding a second body
  copy after the bounded socket read. JSON still retains the bounded raw response while `serde_json` builds its
  value, and packaged requests necessarily retain ZIP/base64/serialized representations under the 96 MiB
  ceiling. Cache inventories remain synchronous filesystem scans inside blocking workers. Residual filesystem
  risk is explicit: recursive cache traversal/deletion and marker replacement still use path-based APIs with
  ordinary cross-process TOCTOU windows; marker writes are no-follow and synced but not atomic replacements.
- Fresh local evidence:
  - structural desktop boundary: `cargo test --locked --manifest-path apps/desktop/src-tauri/Cargo.toml --lib`
    passed the same 38 tests before and after extraction;
  - final desktop Rust suite: 49 passed, including cache-root/settings, package/checksum/traversal, bounded HTTP,
    response parser, URL/path injection and raster-signature regressions;
  - `cargo check --locked --manifest-path apps/desktop/src-tauri/Cargo.toml`: passed;
  - desktop frontend `npm test`: 142 passed; `npm run typecheck`: passed; `npm run build`: passed;
  - effective-line ratchet: passed over 338 files; remaining Loom queue is 8 above 1,500, 28 from 701-1,500
    and 9 from 501-700;
  - desktop Rust formatter check and `git diff --check`: passed.
- No release was built for this individual batch; Phase 79 remains in progress.

### 2026-08-23 - Phase 79-C batch 9: daemon crate root and HTTP intake

- Split `apps/daemon/src/lib.rs` from the 29,072-effective-line hard-cap queue into a 190-effective-line
  crate facade, 52 responsibility files under `src/runtime`, a dedicated bounded HTTP intake module and
  31 shared-module test fragments. The responsibility files cover daemon configuration/lifecycle,
  connection admission, routing, secure persistence, MCP, devices, Surface lifecycle and resources, Art,
  frameworks, Python tools, settings/workflows, shared images, Hook bridge/protocol execution, capabilities,
  runs and response serialization.
- The implementation slices deliberately share the crate root through named `include!` boundaries. This
  preserves the daemon's existing crate-root public API and private symbol graph without turning hundreds of
  internal helpers into artificial `pub(super)` contracts. The 847-physical-line central `route` function was
  not retained as an exception: authentication remains in the entry function and the ordered match table is
  split into four ordinary route-group functions whose fallback calls the next group. Overlapping dynamic
  routes and the final 404 therefore retain their original first-match order.
- All 85 files resulting from this crate-root split are below 500 effective lines. The largest is
  `runtime/publisher_framework_tools.rs` at 488 effective / 510 physical lines; the largest test fragment is
  407 / 421; `http_request.rs` is 393 / 469 and the facade is 190 / 196. No 501-700 exception was added. The
  daemon's already-separate `surface_actions.rs`, `hook_canvas.rs`, `surface_store.rs` and
  `surface_resources.rs` remain in the Phase 79-C queue and are not represented as completed here.
- Preserved the exact external daemon facade and CLI contracts: `DAEMON_AUTH_TOKEN_FILE`, help/version text,
  `DaemonConfig` and its builders, run-store defaults, `LoomDaemon` bind/address/serve methods and runtime log
  reporting remain crate-root-visible as before. The structural crate split passed the same 258 library tests
  before and after the full responsibility extraction; the earlier HTTP-only boundary passed the same 254
  tests before behavior hardening.
- The initial mechanical test extraction exposed an invalid approach rather than being accepted as green:
  removing four source-indent spaces also changed multiline raw YAML fixture bytes and caused three focused
  failures. The final fragments restore the original fixture bytes, add only the five HTTP regression
  helpers/tests introduced here and are formatted as complete Rust item sequences. The three affected YAML
  tests and the final full suite pass.
- Post-split HTTP security and correctness hardening:
  - request lines and headers require strict UTF-8 and valid HTTP/1.0 or HTTP/1.1 syntax; malformed header
    names/values, duplicate or non-decimal `Content-Length`, any unsupported `Transfer-Encoding`, truncated or
    excess bodies and non-UTF-8 bodies are rejected as JSON 400 responses;
  - route-specific body limits remain 1 MiB for ordinary requests, 32 MiB for framework/Art/Surface package
    routes and 96 MiB for MCP packages. Declared limits are checked before capacity reservation, including
    hostile `usize`-scale lengths; a bounded valid length reserves once and allocation failure returns 413;
  - request reads use 64 KiB chunks, scan each header prefix once, retry `Interrupted`, enforce the 30-second
    wall deadline and retain shutdown grace. Production sockets still provide the required two-second
    per-read timeout for the generic `Read` loop;
  - duplicate credentials for the same Authorization scheme never authenticate. Distinct Bearer and Device
    credentials retain the existing compatibility rule: a valid administrator Bearer is not masked by a stale
    Device credential;
  - parsing copies only the bounded header and drains it from the owned request buffer, avoiding a second
    package-sized allocation before String ownership transfer. The remaining body shift is one O(body-size)
    `memmove`, and UTF-8 is intentionally validated before dispatch and again by owned String conversion.
- Lifetime/performance audit: request capacity is bounded by the selected route before reservation; a 96 MiB
  MCP upload now takes roughly 1,536 reads rather than about 196,000 512-byte reads. Route-group chaining adds
  at most three fallthrough calls while retaining the old ordered comparisons. The generic reader cannot
  interrupt a single indefinitely blocking arbitrary `Read`; its production TCP caller sets a read timeout,
  which remains an explicit caller precondition. HTTP chunked transfer remains intentionally unsupported and
  is rejected instead of partially parsed.
- Fresh local evidence:
  - final `cargo test --locked -p loom-daemon --lib`: 258 passed;
  - `cargo test --locked -p loom-daemon --test daemon_cli_contract`: 8 passed;
  - `cargo check --locked -p loom-daemon --all-targets`: passed with no warnings;
  - `cargo check --locked --manifest-path apps/desktop/src-tauri/Cargo.toml`: passed;
  - effective-line checker tests: 15 passed; ratchet passed over 422 files with 7 above 1,500, 28 from
    701-1,500 and 9 from 501-700;
  - direct `rustfmt --check` passed for 83 extracted runtime/test files; daemon Cargo formatter check and
    `git diff --check` passed.
- Two earlier whole-suite hardening runs exposed an unrelated Hook Canvas concurrency flake that passed when
  rerun in isolation; the final post-split 258-test run passed. This remains a signal for the later dedicated
  Hook Canvas file batch, not evidence of an HTTP parser failure.
- No release was built for this individual batch; Phase 79 remains in progress.

### 2026-08-23 - Phase 79-C batch 10: Surface resource store

- Split `apps/daemon/src/surface_resources.rs` from 1,170 effective / 1,356 physical lines into an
  80-effective-line type/facade module and ordinary Rust modules for content operations, lease lifecycle,
  persistence/recovery, garbage collection and responsibility-grouped tests. The runtime Surface resource
  entry remains a separate 257-effective-line route slice. The existing crate-private type names, method
  signatures, error/status mapping, content-addressed paths, lease persistence semantics and instance-lock to
  resource-lock ordering remain unchanged.
- All resulting files are below 500 effective lines. The largest production module is `content.rs` at
  226 effective / 244 physical lines; `leases.rs` is 171 / 186, garbage collection is 151 / 171 and
  persistence is 144 / 159. The largest test module is 187 / 197. No 501-700 exception was added.
- Post-split security and correctness hardening:
  - Surface payload Base64 now applies the 16 MiB decoded-payload bound to encoded length before invoking the
    decoder, then checks the exact decoded length as a second boundary. This prevents an oversized request
    from making the Base64 engine allocate its full output first;
  - a full 512-entry lease table is rejected before a new content-addressed payload or metadata record is
    written, so repeated rejected registrations cannot leave durable orphan pairs for the GC grace window;
  - `duplicate_loom_resource_lease` performs lazy expiry cleanup before lookup, preventing an expired grant
    that remains in memory from being duplicated into a fresh lease;
  - startup filters invalid/expired leases and deterministically trims an oversized persisted table to 512,
    removing the earliest expirations first and atomically persisting the normalized table;
  - an unchanged valid `leases.json` is no longer serialized, synced and atomically replaced on every daemon
    start. The in-memory persistence timestamp is still initialized so the existing addition debounce is
    preserved.
- Lifetime/performance audit: payload reads, hashes, atomic writes and lease serialization remain under the
  store's single `Mutex`; moving those operations outside it requires a transactional ownership redesign and
  was not mixed into this structural batch. Startup intentionally validates payload presence and declared
  length without hashing the entire store; every fetch still re-hashes before returning bytes. Shared-memory
  allocation and release remain owned by `SharedImageStore`, with route integration coverage proving release
  after lease deletion; `SurfaceResourceStore` continues to treat the transport handle as an opaque contract.
- Regression coverage now includes 14 responsibility-grouped store tests: the original 11 behaviors plus
  unchanged-startup persistence, expired-lease duplication rejection and startup lease-cap normalization.
  The route suite separately proves pre-decode and post-decode Base64 bounds, and the existing content-addressed
  binary/shared-memory route and installed JavaScript Surface recovery tests remain green.
- Fresh local evidence:
  - final `cargo test --locked -p loom-daemon --lib`: 262 passed;
  - `cargo test --locked -p loom-daemon --test daemon_cli_contract`: 8 passed;
  - `cargo check --locked -p loom-daemon --all-targets`: passed with no warnings;
  - `cargo check --locked --manifest-path apps/desktop/src-tauri/Cargo.toml`: passed;
  - effective-line checker tests: 15 passed; ratchet passed over 431 files with 7 above 1,500, 27 from
    701-1,500 and 9 from 501-700;
  - daemon Cargo formatter check and `git diff --check` passed.
- One earlier whole-suite run observed the unrelated Hook Art evidence test returning `cancelled` instead of
  `failed`; that exact test passed in isolation and the final post-review 262-test suite passed. No product
  change was made without a reproducible mechanism.
- No release was built for this individual batch; Phase 79 remains in progress.

### 2026-08-23 - Phase 79-C batch 11: Surface action executor

- Split `apps/daemon/src/surface_actions.rs` from 2,934 effective / 3,158 physical lines into a
  45-effective-line lexical facade plus responsibility fragments for the data model, executor lifecycle,
  confirmation helpers, concurrency coordination, worker/runner lifetime, response commits, terminal outcomes
  and seven test areas. `include!` intentionally keeps the implementation in the original private module, so the
  crate-private executor aliases, constructors, submit/confirm/cancel/recovery API and downstream route wiring
  did not acquire broader visibility.
- Every resulting file is below 500 effective lines. The largest test file is `commit_fanout.rs` at 460
  effective / 468 physical lines; the largest production files are `executor.rs` at 427 / 468 and
  `response.rs` at 387 / 396. The facade is 45 / 51, coordination is 134 / 161, the worker runtime is
  205 / 250 and the dedicated recovery tests are 88 / 96. No 501-700 exception was added.
- Post-split security, correctness and lifetime hardening:
  - patch, preview and formal-result commits now share the same current-generation check while holding the
    Surface store lock. Preview and formal output can no longer be committed after their action's generation
    has been superseded;
  - the serial-lock table stores weak mutex references and prunes inactive keys on the next serial dispatch,
    bounding key churn by active workers instead of retaining every historical `instance:action` pair;
  - poisoned coordinator and serial mutexes recover their guards instead of silently running a declared
    `Serial` action without a lock or leaking cancellation/`RejectWhileRunning` reservations;
  - Surface action response nesting is rejected above the shared 32-level host JSON budget before typed
    deserialization and recursive resource-alias replacement. Removing the owned `surfaceAction` member from
    the runner envelope also avoids cloning the entire response before validation;
  - daemon recovery now requeues persisted `Running` acknowledgements as well as queued, interrupted and
    cancel-requested actions. The same status predicate marks confirmation-bound pending events as already
    approved, preventing a restarted approved action from requesting a nonexistent second confirmation.
- The focused Surface action suite grew from 13 to 18 tests. New regressions prove stale preview/formal-result
  rejection, serial lock reuse/pruning and poison recovery, excessive nesting rejection, `Running` restart
  recovery, and restart recovery of an already approved confirmation-bound action.
- Remaining ownership/performance limits are explicit rather than hidden: shared-instance patches still commit
  one attachment at a time and can therefore partially fan out if generation changes between attachment
  transactions; preview, result and broadcasts are separate commits/messages. A runner that ignores
  cancellation beyond the five-second reap grace is detached, after which a later Serial action can overlap
  that legacy thread. Surface persistence and resource registration still hold their store mutexes across
  persistence/Base64 work, and the action layer adds a depth limit but still relies on the existing execution
  transports for total output-byte bounds. Those changes require transactional or runner-ownership redesigns
  and were not mixed into this structural batch.
- Fresh local evidence:
  - final `cargo test --locked -p loom-daemon --lib`: 267 passed;
  - `cargo test --locked -p loom-daemon --test daemon_cli_contract`: 8 passed;
  - `cargo check --locked -p loom-daemon --all-targets`: passed with no warnings;
  - `cargo check --locked --manifest-path apps/desktop/src-tauri/Cargo.toml`: passed;
  - effective-line checker tests: 15 passed; ratchet passed over 445 files with 6 above 1,500, 27 from
    701-1,500 and 9 from 501-700;
  - Cargo formatter check, direct `rustfmt --check` for the eight production Surface action facade/fragments,
    and `git diff --check` passed.
- The first whole-suite run observed the previously seen Hook Canvas concurrent-revision test returning a
  different error variant. The exact test passed in isolation and the fresh 267-test rerun passed; no unrelated
  Hook Canvas product change was made without a reproducible mechanism.
- No release was built for this individual batch; Phase 79 remains in progress.

### 2026-08-23 - Phase 79-C batch 12: Hook canvas document and preview resolution

- Split `apps/daemon/src/hook_canvas.rs` from 2,563 effective / 2,880 physical lines into a
  14-effective-line lexical facade plus responsibility fragments for the data model, document projection,
  bounded session input, geometry, preview candidate traversal, preview-source validation, graph/workflow
  export and four test areas. `include!` keeps the original private module boundary and therefore preserves the
  existing crate-private snapshot, preview, export and component API without visibility expansion.
- Every resulting file is below 500 effective lines. The largest production file is `document.rs` at 444
  effective / 475 physical lines; `preview_sources.rs` is 270 / 322, `preview_candidates.rs` is 264 / 291 and
  `session.rs` is 214 / 235. The largest test file is `preview_semantics.rs` at 414 / 444. No 501-700
  cohesion exception was added, and the facade/fragments are UTF-8 without BOM or trailing whitespace.
- Post-split security, memory and performance hardening:
  - Hook session reads now use a `File::take` boundary and retain at most 64 MiB plus one detection byte, so a
    hostile or corrupted local session cannot make `fs::read` allocate the entire file. Parsed JSON is rejected
    above the shared 32-level nesting budget before document projection and recursive preview traversal;
  - image Data URLs now require a bounded 128-byte image/Base64 header and calculate the exact decoded upper
    bound from Base64 groups, remainder and padding before the preview route invokes the decoder. The same
    20 MiB precondition covers session candidates and runtime preview overrides, while the route retains its
    exact post-decode length check;
  - incoming image links are indexed once while preserving the original first-image-edge precedence, reducing
    preview lookup from repeated `O(nodes * edges)` scans to `O(edges)` construction plus constant-time node
    lookups;
  - recursive upstream preview chains stop after 64 edges. Depth-limited results propagate without entering the
    shared resolution cache, so one excessive chain cannot poison later shallower node resolution. The bounded
    reader and preview walk own no background threads, handles or persistent allocations beyond the document.
- The focused Hook canvas suite first proved the pure structural split with the unchanged 38 tests, then grew
  to 43 tests. New regressions cover byte and nesting rejection without retries, encoded Data URL bounds and
  padding, first-edge index precedence, and depth-limit cache isolation.
- Remaining ownership and security limits are explicit: preview file authorization still canonicalizes a path
  before a later open/read, so closing the symlink/reparse-point TOCTOU window requires handle-based safe-open
  support; the shared Hook `clipboard_cache` root has no per-session ownership metadata. Session projection
  still clones raw node/parameter values and performs synchronous filesystem metadata/canonicalization work;
  removing those costs requires an owned streaming model or async snapshot boundary. Safety-limit failures
  intentionally retain the existing generic `hook_canvas_error` HTTP response, and an oversized runtime
  preview override is rejected without a new public error contract.
- Fresh local evidence:
  - final `cargo test --locked -p loom-daemon --lib`: 272 passed;
  - final focused `cargo test --locked -p loom-daemon hook_canvas`: 43 passed;
  - `cargo test --locked -p loom-daemon --test daemon_cli_contract`: 8 passed;
  - `cargo check --locked -p loom-daemon --all-targets`: passed with no warnings;
  - `cargo check --locked --manifest-path apps/desktop/src-tauri/Cargo.toml`: passed;
  - effective-line checker tests: 15 passed; ratchet passed over 456 files with 5 above 1,500, 27 from
    701-1,500 and 9 from 501-700;
  - Cargo formatter check, direct `rustfmt --check` for the eight production Hook canvas facade/fragments,
    `git diff --check`, UTF-8/BOM and trailing-whitespace checks passed.
- No release was built for this individual batch; Phase 79 remains in progress.

### 2026-08-23 - Phase 79-C batch 13: Surface instance store and persistence boundary

- Split `apps/daemon/src/surface_store.rs` from 2,344 effective lines into an 18-effective-line lexical
  facade plus responsibility fragments for the persisted model, load/query/create/migration operations,
  attachments and patches, lifecycle/result commits, events and confirmations, cancellation and durable
  transactions, bounded recovery validation, resource/scene JSON operations and strict JSON-pointer mutation.
  Six test fragments own fixtures, pointer behavior, persistence/recovery, generation/failure, events/lifecycle
  and persistent-projection/expiry contracts. `include!` preserves the original crate-private lexical boundary
  without widening the store API or changing the Surface serialization schema.
- Every resulting production and test file is below 500 effective lines and begins with a concise ownership
  comment. The largest production file is `validation.rs` at 401 effective / 438 physical lines, followed by
  `events_confirmations.rs` at 267 / 280 and `read_create.rs` at 252 / 273. The largest test file is
  `tests/persistence.rs` at 317 / 339. The facade is 18 / 22. No 501-700 cohesion exception was added.
- Post-split security, memory and performance hardening:
  - persisted `instances.json` reads retain at most 64 MiB plus one detection byte, reject excessive structural
    nesting before `serde_json` allocates a value tree, and then apply the exact shared 32-level value-depth
    check before typed deserialization;
  - recovery validates schema-bound identities, semver and canonical package digests; attachment, snapshot,
    event, acknowledgement and confirmation cross-identities; protocol values; preview/result/failure ownership;
    migration history and pending queue quotas. A file containing a runtime-only `Temporary` instance is now
    rejected as corrupt instead of being silently discarded before validation;
  - serialization writes the persistent projection through a 64 MiB bounded writer and a borrowed record map,
    so it cannot build an over-limit output and no longer deep-clones every persistent record merely to encode
    the document. The trailing newline remains inside the same exact byte budget;
  - removed the private `latest_continuous_events` map, which was written but never read, serialized or exposed.
    Continuous events retain their existing accepted/queued response but no longer keep distinct payloads alive
    for the daemon lifetime. A regression submits more distinct events than the discrete queue limit and proves
    they do not enter pending events, acknowledgement state or the persistent file.
- The focused Surface store suite first passed the unchanged 15 tests after the pure structural split, then grew
  to 22 tests. New regressions cover bounded read/write helpers, exact and pre-parse nesting rejection, persisted
  map-key and persistence-class integrity, recovery queue quotas and transient continuous-event retention.
- Remaining ownership/performance limits are explicit: terminal `event_acks` have no retention quota or GC, but
  automatic eviction requires a replay/idempotency contract; every mutation still clones the complete instance
  map for rollback and serializes the complete persistent projection; durable `write_all`/`sync_all`/replacement
  runs while callers hold the single store mutex. Moving these costs outside the lock requires a two-phase
  transaction or journal rather than a local refactor. Mutex-poison recovery also remains inconsistent across
  callers, and path-based atomic replacement retains ordinary cross-process filesystem race limits.
- Fresh local evidence:
  - final focused `cargo test --locked -p loom-daemon --lib surface_store::tests`: 22 passed;
  - final `cargo test --locked -p loom-daemon --lib`: 279 passed;
  - `cargo test --locked -p loom-daemon --test daemon_cli_contract`: 8 passed;
  - `cargo check --locked -p loom-daemon --all-targets`: passed with no warnings;
  - `cargo check --locked --manifest-path apps/desktop/src-tauri/Cargo.toml`: passed;
  - effective-line checker tests: 15 passed; ratchet passed over 471 files with 4 above 1,500, 27 from
    701-1,500 and 9 from 501-700;
  - Cargo formatter check, direct `rustfmt --check` for all 16 Surface store facade/implementation/test files,
    `git diff --check`, UTF-8/BOM and trailing-whitespace checks passed.
- No release was built for this individual batch; Phase 79 remains in progress.

### 2026-08-23 - Phase 79-D batch 14: desktop Loom API facade and domain clients

- Split `apps/desktop/src/services/loomApi.ts` from 1,618 effective / 1,827 physical lines into a stable
  16-effective-line facade and 16 production modules for shared/core types, defaults, settings/snapshot types,
  transport, snapshot orchestration, Hook bridge calls, framework lifecycle, plugin trust/credentials, Art
  management, workflows, MCP operations and runtime/cache settings. Internal clients depend on the transport
  module rather than importing the facade, while the facade preserves all 154 previous public exports and does
  not expose its eight new transport helpers.
- Split the former 21-test, 1,025-physical-line `loomApi.test.ts` by behavior contract into MCP, snapshot, Art,
  Hook, framework and plugin suites, then added a dedicated transport hardening suite. All 21 old test titles
  are present exactly once and seven new regressions bring the focused boundary to 28 tests.
- All 17 production files and seven test files are below 500 effective lines and start with an ownership
  comment. The largest production file is `snapshot.ts` at 278 effective / 303 physical lines; the largest test
  file is `snapshot.test.ts` at 283 / 320. The facade is 16 / 18. No 501-700 cohesion exception was added.
- Post-split security, lifetime and performance hardening:
  - browser JSON responses reject a declared or streamed body above 16 MiB, cancel an oversized stream on a
    best-effort basis, release its reader lock and bound untrusted daemon error details to 2,048 characters;
  - browser Hook preview URLs must remain on the configured origin and inside the normalized
    `/v1/hook-bridge/canvas/nodes/.../preview` route. Absolute external URLs and dot-segment traversal are
    rejected. A Tauri preview read failure is propagated instead of silently falling back to a direct URL;
  - `readLoomSnapshot` now accepts an optional `AbortSignal` and forwards it through the parallel browser core,
    optional-module and degraded-health probes. A throwing timeout observer can no longer prevent the timeout
    promise from resolving, and the new timeout regressions clear their watchdogs and settle abort-aware fixture
    operations rather than leaving delayed timers behind.
- Remaining boundaries are explicit: Tauri 2.11 `invoke` exposes headers but no `AbortSignal`, so
  `read_loom_snapshot` and `resolve_loom_daemon_url` cannot be cancelled from TypeScript after dispatch;
  single-flight callers consequently share the first in-flight native operation. The 16 MiB browser response
  bound still permits bounded transient duplication across stream chunks, the contiguous byte array, decoded
  string and parsed JSON. The configured daemon base URL remains a trusted desktop/runtime boundary rather than
  an allowlisted or authenticated browser transport, and generic mutation helpers do not yet accept caller
  cancellation. The current `App.tsx` caller does not yet forward its readiness signal, and the erased
  `mcpMarketplace.ts`/facade type dependency remains; both oversized files were deliberately left unchanged so
  this batch would not evade the rule that any touched mandatory file must be completed to 700 lines or fewer.
  They remain in the generated mandatory split queue.
- Fresh local evidence:
  - pure structural boundary before hardening: the same 21 `loomApi` tests passed and desktop typecheck passed;
  - final focused `loomApi` suites: 28 passed;
  - final desktop `npm test`: 149 passed; `npm run typecheck`: passed; `npm run build`: passed;
  - `cargo check --locked --manifest-path apps/desktop/src-tauri/Cargo.toml`: passed;
  - effective-line checker tests: 15 passed; ratchet passed over 493 files with 3 above 1,500, 26 from
    701-1,500 and 9 from 501-700;
  - export/test migration parity scripts, `git diff --check`, and UTF-8/BOM checks for all 24 touched/new
    desktop TypeScript files passed. The desktop package defines no separate formatter command.
- No release was built for this individual batch; Phase 79 remains in progress.

### 2026-08-23 - Phase 79-D batch 15: runtime-host MCP execution boundary

- Split `framework-packages/runtime-host/src/mcp.rs` from 2,567 effective / 2,819 physical lines into a
  61-effective / 66-physical-line lexical facade, 15 production responsibility fragments and 12 test
  fragments. The facade retains the original private `mcp` module, shared imports and lexical visibility, so
  `mcp::execute`, the serialized camelCase result contract, skip rules and the exact
  `mcp::tests::runtime_mcp_session_pool_fixture_server` child-test path remain unchanged.
- All 28 facade/production/test files are below 500 effective lines and start with an ownership comment. The
  largest production fragment is `transport_config.rs` at 208 effective / 220 physical lines; the largest test
  fragment is `tests/dependencies.rs` at 141 / 160. No 501-700 cohesion exception was added.
- Post-split security, resource and failure-path hardening:
  - `manifest.json` is read through a 1 MiB plus one detection-byte cap instead of an unbounded `fs::read`;
    the exact 1 MiB boundary is accepted by the reader and larger input is rejected before JSON parsing;
  - cached `tools/list` values and returned `tools/call` values must encode to at most 8 MiB and nest at most
    64 levels. The size counter writes into no backing allocation, and an over-limit response closes rather
    than recaches the affected session;
  - the response-depth bound also limits the later recursive credential-redaction walk. Focused regressions
    prove exact byte/depth acceptance and one-over rejection, and the temporary manifest fixture now cleans up
    through `Drop` even when an assertion panics;
  - the session cache remains deliberately limited to one entry with a 60-second idle lifetime. Its
    `thread_local` ownership is valid because the persistent `--serve` loop handles requests serially on one OS
    thread; this dependency is now documented next to the pool rather than being implicit.
- Independent post-split review found no public API, serde wire-shape, include-scope, credential-redaction or
  resource-release regression. The apparent double-close path is intentional best effort: stdio termination is
  idempotent, successful HTTP close clears the session id, and a failed HTTP close can be retried by `Drop`.
  Stdio argument path placeholders also remain an intentional compatibility contract rather than a sandbox
  boundary; the installed MCP process already executes with the granted filesystem permissions.
- Remaining boundaries are explicit: `FrameworkExecuteRequest` carries no cancellation token, so runtime-host
  calls rely on the lower MCP request timeout and caller cancellation needs a protocol migration. The cached
  client may retain one child process and credential-bearing transport configuration for up to 60 seconds;
  close remains synchronous. Response validation adds one bounded linear encoded-size traversal, while argument
  normalization, result redaction and final serialization can still create bounded transient duplication. DNS
  rebinding/JSON-RPC transport validation remain owned by `loom_mcp`; `art.runtime.json` is still read without a
  local byte cap in `main.rs`, outside this MCP module split; and path-based manifest opening retains ordinary
  symlink/handle race limits under the trusted installed-Art directory boundary.
- Fresh local evidence:
  - pure structural boundary before hardening: all original 34 runtime-host tests passed and the crate compiled;
  - final `cargo test --locked --manifest-path framework-packages/runtime-host/Cargo.toml`: 36 passed;
  - `cargo check --locked --manifest-path framework-packages/runtime-host/Cargo.toml`: passed;
  - dependent `cargo test --locked -p loom_tool_registry`: 210 passed across two suites;
  - `cargo check --locked -p loom-daemon --all-targets` and
    `cargo check --locked --manifest-path apps/desktop/src-tauri/Cargo.toml`: passed;
  - effective-line checker tests: 15 passed; ratchet passed over 520 files with 2 above 1,500, 26 from
    701-1,500 and 9 from 501-700;
  - workspace Cargo formatter check, direct `rustfmt --check` for all 28 MCP facade/implementation/test files,
    `git diff --check`, and UTF-8/BOM/trailing-whitespace checks passed.
- No release was built for this individual batch; Phase 79 remains in progress.

### 2026-08-23 - Phase 79-D batch 16: desktop MCP marketplace boundary

- Split `apps/desktop/src/services/mcpMarketplace.ts` from 735 effective / 808 physical lines into a
  26-effective-line stable facade and focused modules for public/Registry types, curated catalog data,
  deterministic helpers, installed-server configuration/health projection and external Registry normalization.
  The largest production module is `mcpMarketplace/registry.ts` at 269 effective / 298 physical lines;
  `types.ts` is 158 / 180, `catalog.ts` is 153 / 158, `configuration.ts` is 98 / 109 and `helpers.ts` is
  92 / 103. The focused test file is 308 / 335. No 501-700 cohesion exception was added.
- Preserved every previous facade export, including the category/catalog constants, parser and pagination
  helpers, Registry mapping, merge/configuration/health functions and all eight public types. Internal Registry
  transport shapes remain private to the implementation. The initial Node structural run identified that the
  extracted ESM imports needed explicit `.ts` extensions; those paths were corrected before the structural
  boundary was accepted. The same 11 focused tests and desktop typecheck then passed.
- Post-split security hardening:
  - Registry-provided Streamable HTTP options are rejected before projection unless the effective URL uses
    HTTP(S) and contains no username/password. The validation runs against a safe projection of unresolved
    templates and again against every fully resolved URL, so `javascript:`, `file:` and credential-bearing
    endpoints cannot enter an install configuration;
  - Registry variables marked secret are never substituted into the URL. Missing, secret or invalid-choice
    variables leave the template unresolved and require explicit manual configuration instead of persisting a
    secret in an endpoint string;
  - regressions cover unsafe protocols, URL credentials, retained safe templates, required secret headers and
    prevention of secret-variable URL embedding. The focused suite now has 13 tests.
- Per-file lifetime/performance review found no owned asynchronous resources in this pure transformation layer.
  The live MCP Store currently filters and paginates the eight-item curated catalog; those scans are bounded and
  negligible. Registry normalization is linear in server/package/remote count and is currently an exported
  import path rather than the active Store feed. Response-size ownership therefore remains with the browser/API
  transport rather than being duplicated in these pure modules.
- Residual trust boundaries remain explicit: the existing editable URL contract permits credential-free local
  HTTP endpoints, so private/metadata-address SSRF enforcement remains the native connection owner's
  responsibility and was not silently redefined here. Registry package identifiers and argv values remain
  user-reviewed install choices passed as argument arrays rather than shell text; package trust and execution
  policy remain daemon/package-layer responsibilities.
- Fresh local evidence:
  - pure structural boundary after ESM path repair: 11 focused tests passed and desktop typecheck passed;
  - final focused MCP marketplace suite: 13 passed;
  - final desktop `npm test`: 151 passed; `npm run typecheck`: passed; `npm run build`: passed;
  - `cargo check --locked --manifest-path apps/desktop/src-tauri/Cargo.toml`: passed;
  - effective-line checker tests: 15 passed; ratchet passed over 525 files with 2 above 1,500, 25 from
    701-1,500 and 9 from 501-700;
  - `git diff --check` passed.
- No release was built for this individual batch; Phase 79 remains in progress.

### 2026-08-23 - Phase 79-D batch 17: desktop MCP workspace boundary

- Split `apps/desktop/src/components/mcp/McpHub.tsx` from 1,041 effective / 1,086 physical lines into a
  349-effective / 372-physical-line orchestration facade and focused modules for workspace tabs and inputs,
  installed-service cards, Store cards and pagination, server editing, credential editing, icons, shared private
  types and package-file intake. The largest extracted file is `McpServerDialog.tsx` at 270 / 284;
  `McpStorePanel.tsx` is 168 / 172, `McpHubToolbar.tsx` is 159 / 170, `McpCredentialDialog.tsx` is 124 / 130,
  `McpServicesPanel.tsx` is 112 / 116, and all remaining production modules are below 30 effective lines. No
  file exceeds 500 effective lines and no cohesion exception was added.
- Preserved the established `McpHub.tsx` public import surface: `McpHub` remains the workspace component and
  `McpCredentialDialog` remains re-exported from the same path for its independent `App.tsx` consumer. Daemon
  mutation ownership, single-operation serialization, save-then-refresh/test ordering, marketplace health
  projection, tab semantics, package-service controls and dialog portal/cleanup behavior remain in their prior
  layers. The existing source-contract test now reads the exact responsibility files instead of weakening its
  assertions after extraction.
- Post-split safety, lifetime and performance hardening:
  - browser-selected MCP archives are rejected before `File.arrayBuffer()` when their declared size exceeds the
    native package layer's 64 MiB compressed-archive ceiling, and the resulting byte length is checked again by
    the encoder before transport;
  - Base64 conversion now encodes 24 KiB chunks whose size is divisible by three. This preserves the exact wire
    value while removing the previous archive-sized binary-string intermediate; regressions cover empty and
    binary input, the chunk boundary, the exact size limit, one-over rejection and invalid numeric sizes;
  - the daemon now applies the same decoded-size ceiling before Base64 allocation as well as after decoding.
    The preflight uses the encoded-length upper bound, preserves the existing invalid-Base64 response and maps
    oversize input to the existing `invalid_mcp_server_package` contract. Focused Rust tests cover below-limit,
    exact-limit, same-encoded-length one-over, invalid encoding and pre-decode rejection paths;
  - the credential dialog now recaptures Tab focus when focus has escaped the modal, matching the server editor's
    focus trap while retaining timer/listener/body-overflow cleanup and previous-focus restoration.
- Independent post-split review found no demonstrated XSS, unsafe external scheme, credential echo, shell-string
  construction, mutation-order or resource-cleanup regression. Marketplace source URLs are normalized and
  validated before the Tauri command validates them again; React continues to escape registry text; package
  execution and signature/trust enforcement remain native responsibilities. Credential-free local HTTP MCP
  endpoints remain an intentional product contract rather than being silently reclassified as an SSRF defect.
- Remaining boundaries are explicit: package transport still necessarily retains the selected bytes and final
  Base64/JSON payload concurrently, but both are bounded by the 64 MiB archive contract. Component async work has
  no cancellation token or generation id, so a completed server test can still publish its bounded snapshot after
  an external list refresh or unmount; operations remain deliberately serialized, and no speculative lifecycle
  abstraction was introduced without a component harness.
- Fresh local evidence:
  - pre-split structural baseline: 13 focused MCP marketplace tests and desktop typecheck passed;
  - pure split boundary: the same 13 focused tests, desktop typecheck and production build passed;
  - final focused frontend MCP suites: 15 passed; final desktop `npm test`: 153 passed;
  - desktop `npm run typecheck` and `npm run build`: passed;
  - focused `loom-daemon` decoded-size regression and
    `cargo check --locked --manifest-path apps/desktop/src-tauri/Cargo.toml`: passed;
  - workspace `cargo fmt --all -- --check`: passed;
  - effective-line checker tests: 15 passed; ratchet passed over 534 files with 2 above 1,500, 24 from
    701-1,500 and 9 from 501-700;
  - `git diff --check` passed.
- No release was built for this individual batch; Phase 79 remains in progress.

### 2026-08-23 - Phase 79-D batch 18: Hook canvas service boundary

- Split `apps/desktop/src/services/hookCanvas.ts` from 834 effective / 975 physical lines into an
  8-effective / 10-physical-line stable facade and focused modules for API transport, snapshot presentation,
  instantiation/interface inference, workflow bindings, workflow Art export, sub-workflow serialization,
  selection/geometry, viewport layout and shared types. The largest production module is `workflowArt.ts` at
  183 / 201; `types.ts` is 158 / 186 and `layout.ts` is 153 / 166. Every new production file is below 500
  effective lines, so no cohesion exception was added.
- Preserved every existing public export and both established consumer import paths through the facade. A
  structural baseline of 32 focused tests plus desktop typecheck passed before extraction and again immediately
  after extraction. Independent post-split review confirmed the public surface is complete, module dependencies
  are acyclic and the previously covered ordering, fallback geometry, component selection and serialization
  semantics remain intact.
- Post-split security and correctness hardening:
  - secret parameters are no longer copied into workflow YAML or ordinary tool defaults. Exported secret fields
    retain only the credential-binding marker expected by the daemon package layer;
  - workflow expression node and port tokens are restricted to the daemon-compatible ASCII slug alphabet before
    interpolation, preventing YAML/expression delimiter injection;
  - daemon metadata with duplicate workflow node IDs now fails closed in both Art-bundle and sub-workflow export
    paths instead of emitting ambiguous duplicate nodes or merging edge bindings;
  - the Hook canvas UI contract now reads the Rust WebView2 contract from the extracted `runtime/app.rs` and
    `runtime/types.rs` ownership modules as well as the stable `lib.rs` facade. This repairs a stale source-contract
    assertion without weakening the runtime requirements.
- Post-split performance and numeric hardening:
  - connected-component traversal now uses an indexed queue instead of repeated `Array.shift()`, making the
    traversal O(V+E); a 10,000-node linear-canvas regression covers the boundary;
  - workflow interface inference builds incoming/outgoing and first-target-port indexes in one edge pass, and
    workflow Art export uses a raw-node-to-workflow-node map instead of repeated node scans;
  - viewport fitting and slider conversion sanitize non-finite/degenerate dimensions, ranges and step counts so
    public geometry helpers cannot return NaN or Infinity for malformed surface input.
- Lifetime review found no timers, listeners, object URLs, persistent caches or other owned asynchronous resources
  in the split service. Remaining performance debt is explicit: per-edge `edgeEndpoints` and
  `edgeWorldEndpoints` still perform two linear node lookups, giving O(E*V) work when callers project every edge;
  the binding helper can also reach O(E^2) for an unusually high number of incoming ports. Neither path has a
  demonstrated current regression, so no mutation-sensitive cache was introduced speculatively. Snapshot polling,
  transport response bounds and item-count policy remain owned by `App.tsx` and the daemon rather than this pure
  transformation layer. The unchanged 790-effective-line `hookCanvas.test.ts` remains ratcheted migration debt.
- Fresh local evidence:
  - final focused Hook canvas suites: 38 passed, including secret suppression, unsafe/duplicate identifier
    rejection, large traversal, fallback geometry and degenerate viewport regressions;
  - final desktop `npm test`: 159 passed; `npm run typecheck`: passed; `npm run build`: passed;
  - `cargo check --locked --manifest-path apps/desktop/src-tauri/Cargo.toml`: passed;
  - `Test-HookCanvasUiContract.ps1`: passed after its split-aware Rust source ownership repair;
  - effective-line checker tests: 15 passed; ratchet passed over 544 files with 2 above 1,500, 23 from
    701-1,500 and 9 from 501-700;
  - strict UTF-8/no-BOM/final-newline/trailing-whitespace checks passed for all Batch 18 files, and
    `git diff --check` passed.
- No release was built for this individual batch; Phase 79 remains in progress.

### 2026-08-23 - Phase 79-D batch 19: Hook canvas thumbnail boundary

- Split `apps/desktop/src/components/hook/HookCanvasThumbnail.tsx` from 998 effective / 1,096 physical lines
  into a 378-effective / 404-physical-line orchestration facade plus focused toolbar, surface/minimap,
  workflow-interface, parameter-exposure, node-properties, rename-dialog, pure view-model and viewport-hook
  modules. The largest extracted module is `useHookCanvasViewport.ts` at 327 / 353; `HookCanvasSurface.tsx`
  is 182 / 187. Every resulting component/helper is below 500 effective lines, so no cohesion exception was
  added.
- Preserved the named `HookCanvasThumbnail` and `WorkflowArtCreationRequest` exports and the App wrapper's
  callback contract. The static workspace contract was updated to read the toolbar's moved JSX in addition to the
  facade, preserving assertions for the removed visual-workflow entry, save-workflow placement, zoom controls and
  dark-canvas styling. No Hook component files had parallel edits before this batch.
- Post-split safety and accessibility hardening:
  - global pointer handlers now use the latest minimap projection through a ref, are installed only while nodes are
    present, and isolate primary pointer IDs; pointer jitter below the drag threshold no longer updates the full
    viewport or triggers a canvas rerender;
  - the canvas exposes a named keyboard-focusable region with arrow/Home pan and +/- zoom shortcuts, zoom exposes
    a value text, node buttons expose `aria-pressed`, workflow actions announce status/error messages, parameter
    value controls receive accessible names, and the rename dialog now has `aria-modal`, focus trapping, Escape,
    body-scroll restoration and previous-focus restoration;
  - workflow-list refreshes use a generation guard so a stale overlapping response cannot replace a newer list after
    save, rename or delete. Saved snapshot loading keeps its existing cancellation guard.
- Security review found no new XSS or URL/path injection: daemon strings remain React-escaped, preview paths and
  workflow IDs retain service-layer validation, and Art secret/YAML protections remain in `hookCanvas` modules.
  The existing 16 MiB response bound remains the transport boundary; no unproven node-count cap or speculative
  virtualisation was added. Base URL allowlisting remains an application configuration boundary, not a component
  refactor assumption.
- Fresh local evidence:
  - pre-split structural baseline: 42 focused tests and desktop typecheck passed;
  - post-split focused suites: 42 existing tests plus 3 new thumbnail helper regressions passed;
  - final desktop `npm test`: 162 passed; `npm run typecheck`: passed; `npm run build`: passed;
  - `cargo check --locked --manifest-path apps/desktop/src-tauri/Cargo.toml`: passed;
  - `Test-HookCanvasUiContract.ps1`: passed;
  - effective-line checker tests: 15 passed; ratchet passed over 553 files with 2 above 1,500, 22 from
    701-1,500 and 9 from 501-700;
  - strict UTF-8/no-BOM/final-newline/trailing-whitespace checks passed for the Batch 19 files, and
    `git diff --check` passed.
- No release was built for this individual batch; Phase 79 remains in progress.

### 2026-08-23 - Phase 79-D batch 20: desktop application composition boundary

- Split `apps/desktop/src/App.tsx` from 7,426 effective / 7,794 physical lines into a below-500-effective-line
  application composition owner and focused modules for application-shell primitives, Art editing/creation,
  registry and marketplace flows, feedback, devices, plugin security, Hook bridge, MCP, settings and About UI.
  The extracted production modules include ownership comments; no resulting file exceeds the 700-line soft cap.
- Split the former monolithic Art/UI source-contract test into focused Art, settings and wizard suites plus a
  shared source loader. This keeps the existing static contract coverage while assigning each assertion group to
  the module family it protects.
- Post-split security hardening:
  - plugin credential and publisher-key plaintext is no longer fetched and inserted into the DOM when the security
    panel loads. The list renders masked summaries; only an explicit edit/reveal action fetches one secret, and
    request-generation guards discard stale reveal responses after a newer request or unmount;
  - external marketplace, repository and update links now pass through one HTTPS-only URL normalizer that rejects
    embedded credentials before either native Tauri opening or the browser fallback. Focused regressions cover the
    allowed and rejected URL shapes.
- Post-split lifetime and performance hardening:
  - application startup no longer issues the initial snapshot refresh and local-service readiness loop in
    parallel. Mount cleanup invalidates request gates and suppresses later snapshot/state work while preserving the
    exact `waitForLoomOnline(refreshSnapshot)` Hook canvas contract;
  - Hook canvas polling now runs only while its section is active, and device polling uses a single-flight guard so
    a slow refresh cannot overlap the next interval;
  - Art wizard source/MCP discovery, settings cache snapshots and security reveal actions use mounted/request
    generations to prevent stale asynchronous writes after a newer operation or unmount.
- Five cohesive 501-700-line modules remain as explicit, hash-bound soft-cap exceptions through 2026-09-30:
  `ArtEditDialog.tsx` (610), `RegistryPanel.tsx` (652), `useAddArtWizardController.ts` (569),
  `SettingsPanel.tsx` (539) and `useSettingsPanelController.ts` (676). Their view/controller or dialog boundaries
  have already been separated; each exception records ownership, rationale, approval and verification commands.
- Explicit residual risk and follow-up:
  - dispatched native Tauri invocations cannot be cancelled by the renderer; generation/mount gates prevent stale
    renderer updates but do not abort native work already in progress;
  - the readiness loop retains its source-level Hook canvas contract and therefore does not receive the App mount
    `AbortSignal`; cleanup still prevents follow-on normal work and state writes;
  - the desktop UI still relies heavily on source-contract tests because the repository has no React component
    runtime harness. Art pending-request, focus and rapid-request races should receive behavioral tests when that
    harness is introduced;
  - the pre-existing unreferenced `HookCanvasView.tsx` was not removed without a dedicated ownership decision;
  - `apps/desktop/src/styles.css` remains the sole hard-cap file at 6,906 effective / 8,104 physical lines and is the
    next mandatory Phase 79 split target.
- Fresh local evidence after all Batch 20 code changes:
  - final desktop `npm test`: 164 passed; `npm run typecheck`: passed; `npm run build`: passed;
  - `cargo check --locked --manifest-path apps/desktop/src-tauri/Cargo.toml`: passed;
  - `Test-HookCanvasUiContract.ps1`: passed;
  - effective-line checker tests: 15 passed; ratchet passed over 581 files with 1 above 1,500, 21 from
    701-1,500 and 14 from 501-700, with no ratchet violations;
  - strict UTF-8/no-BOM/final-newline/trailing-whitespace checks passed for all 31 Batch 20 files, and
    `git diff --check` passed.
- No release was built for this individual batch; Phase 79 remains in progress.

### 2026-08-24 - Phase 79-D batch 21: desktop stylesheet ownership boundary

- Split `apps/desktop/src/styles.css` from 6,906 effective / 8,104 physical lines into a stable
  23-effective / 24-physical-line ordered-import facade and 23 responsibility-owned CSS modules for foundation
  tokens/reset, shell, shared workspace surfaces, Art, workflow creation, Hook canvas, settings, devices,
  responsive layout, late tooling/theme overrides, MCP and cross-feature accessibility. Every module is below 500
  effective lines; the largest are `devices.css` at 441 / 517 and `workspace.css` at 432 / 501. No cohesion
  exception was added.
- Preserved behavior and cascade ordering explicitly:
  - before replacing the monolith, concatenating the 22 pure split payloads reproduced the original normalized
    source byte-for-byte with SHA-256 `6052eb34696ae97edcb08c49e86189d9a0dabcd24d207633ae5bfcde9daecfe2`;
  - the final accessibility module is intentionally imported after all feature and theme overrides;
  - `desktopStyleSource.ts` now reads the local ordered import graph for source-contract tests, rejects duplicate,
    remote and path-traversing imports, and the focused import-graph tests require every direct `styles/*.css` file
    to be imported exactly once in the approved cascade order with an ownership comment and no nested imports;
  - independent post-split comparison against the Git baseline found only the three intended accessibility changes
    below, with no reordered, lost, duplicated or partially split rule, keyframe or media block.
- Post-split accessibility hardening:
  - native interactive controls now have one final visible `:focus-visible` fallback, including a forced-colors
    `Highlight` mapping;
  - `prefers-reduced-motion: reduce` now covers every desktop animation and constrains all feature transitions,
    rather than disabling only the MCP busy indicator;
  - confirmation backdrops and dialogs now have bounded viewport height and internal vertical scrolling, preserving
    content and actions under high browser zoom or unusually long messages;
  - forced-colors mode removes the active Hook edge drop shadow and maps the edge stroke to `Highlight`.
- Security, resource and performance review found no additional actionable defect: CSS modules contain no remote,
  data or file URLs, no dynamic `attr()`/`content` injection and no external resource ownership; universal selectors
  are limited to the standard box-sizing reset and the user-opted reduced-motion block. The only filtered canvas
  edge is the active edge, and the remaining shadows/animations are scoped rather than demonstrated hot paths, so
  no speculative rendering rewrite was introduced.
- Explicit residual boundaries:
  - CSS source-contract tests cannot replace browser parsing and rendering validation; the canonical gate therefore
    retains both the Rsbuild production build and a real-browser smoke;
  - global shell overflow and the Hook canvas `touch-action: none` behavior were retained because inner scroll
    ownership and canvas drag/zoom semantics require a product-level interaction decision rather than a stylesheet
    split assumption;
  - direct targeted TypeScript tests require the repository's active Node toolchain; the canonical `npm test`
    command is the supported proof and passed under the active environment.
- Fresh local evidence after all Batch 21 changes:
  - final desktop `npm test`: 169 passed; `npm run typecheck`: passed; `npm run build`: passed in 0.63 seconds;
  - the production build emitted one 162,010-byte CSS asset containing all checked foundation, shell, Art, workflow,
    Hook, settings, device, MCP, reduced-motion and forced-colors sentinels;
  - `cargo check --locked --manifest-path apps/desktop/src-tauri/Cargo.toml`: passed;
  - `Test-HookCanvasUiContract.ps1` and `Test-GitHubActionsContract.ps1`: passed;
  - effective-line checker tests: 15 passed; ratchet passed over 607 files with 0 above 1,500, 21 from
    701-1,500 and 14 from 501-700, with no ratchet violations or CSS module above 500;
  - Playwright loaded the real desktop page at `127.0.0.1:1423`, rendered the shell/MCP surface, and reported no
    stylesheet error. The only console failures were the expected unavailable local daemon requests to port 8765;
  - strict UTF-8/no-BOM/final-newline/trailing-whitespace checks passed for all 31 Batch 21 files, and
    `git diff --check` passed. The temporary browser session and its untracked CLI metadata were removed.
- No release was built for this individual batch; Phase 79 remains in progress. The hard-cap queue is now empty;
  the next highest mandatory split target is `scripts/smoke-release.ps1` at 1,458 effective / 1,704 physical lines.

### 2026-08-24 - Phase 79-D batch 22: release-smoke execution boundary

- Split `scripts/smoke-release.ps1` from 1,458 effective / 1,704 physical lines into a stable 71-effective /
  76-physical-line orchestration facade and 11 responsibility-owned modules for assertions, image checks, HTTP
  status, process trees, evidence, process capture, cloud and MCP Registry fixtures, isolated release phases, the
  main release smoke and focused child smokes. The largest resulting file is `Release.ps1` at 427 / 473;
  `LoomReleaseLayout.ps1` is 223 / 245 and every new or changed Batch 22 source/test file is below 500 effective
  lines, so no cohesion exception was added.
- Preserved the release-smoke contract before hardening:
  - the facade keeps the original package/evidence parameters, release-layout and shared-port imports, local smoke,
    three focused smokes and summary schema;
  - before replacing the monolith, concatenating the facade bootstrap, the ordered module payloads and the
    orchestration suffix reproduced the original nonblank payload exactly, with normalized payload SHA-256
    `f963919ec1123db7f23970092326a2b01e6c32b3562a27d8612e22e63c7d8744`;
  - `Test-StandaloneReleaseContract.ps1` now requires the exact module set and load order, parses every module,
    rejects nested module-directory imports and evaluates behavior sentinels across the aggregate source rather
    than assuming a monolithic implementation.
- Post-split security hardening:
  - failure evidence skips credential/token-bearing path segments, including the extensionless persisted
    `control-plane/daemon-token`, caps copied files at 16 MiB, avoids reparse-point traversal and redacts JSON,
    assignment, header, Bearer and query-string secret forms;
  - evidence filenames are single JSON path segments, evidence directory chains reject reparse points, generated
    temporary roots are direct recognized children of `TEMP`, and cleanup refuses unrelated or reparse roots;
  - all JSON/status HTTP helpers require an absolute loopback HTTP(S) URI without user information and disable
    automatic redirects, preventing a loopback response from redirecting the smoke to a non-loopback target;
  - `LoomReleaseLayout.ps1` now enforces lexical package containment and rejects reparse points in the package root,
    manifest, executable/runtime chain, CLI ZIP, extraction root and extracted entries. The integrity contract creates
    a real runtime junction and proves it is rejected without following or deleting its target.
- Post-split lifetime and performance hardening:
  - captured child processes use `System.Diagnostics.Process` with concurrent stdout/stderr reads, bounded waits,
    reliable exit-code capture and process-tree cleanup; the generated `run.cmd`/`cmd.exe` boundary was removed;
  - focused smokes now have a five-minute timeout, tokenized daemon smoke owns its process in a `finally`, daemon
    health polling is shared, descendant traversal uses an indexed linear queue and diagnostic-copy failures cannot
    replace the original smoke failure;
  - cloud and MCP Registry fixture readers now accumulate into bounded `MemoryStream` buffers and scan request
    delimiters linearly instead of repeatedly copying and UTF-8 decoding the full request. Cloud requests are capped
    at 16 MiB and decoded with byte-accurate header/body slices limited by `Content-Length`; Registry requests are
    capped at 1 MiB;
  - the summary is written once to its run path and once to `latest`, removing the duplicate pair of writes.
- Added `Test-SmokeReleaseModules.ps1` and wired it into the Windows CI pre-generated-output gate. It dot-sources
  modules without executing the release entrypoint and dynamically verifies loopback rejection, secret redaction,
  daemon-token evidence suppression, protected temporary cleanup, direct stdout/stderr plus exit-code capture, fixture
  bounds and the absence of command wrappers. `Test-GitHubActionsContract.ps1` requires the new CI invocation.
- Explicit residual boundaries:
  - this batch used non-mutating dry-run/layout/module contracts and did not build a release candidate or run the full
    release smoke against packaged executables; the formal release remains deferred to the Phase 79 closure gate;
  - inherited parent environment variables and shared-port bind/check timing remain process/environment boundaries;
    no unproven environment allowlist or port reservation protocol was introduced in a structural batch;
  - the release smoke still executes the cloud text tool while treating its cloud image and multipart tools as
    configuration contracts. Multipart runtime execution remains covered by daemon Rust tests rather than this
    package smoke and can be promoted into the release smoke when a dedicated input/evidence contract is approved.
- Fresh local evidence after all Batch 22 changes:
  - 16 Batch 22 PowerShell files parsed under Windows PowerShell 5.1.26100.9168;
  - `Test-SmokeReleaseModules.ps1`, `Test-StandaloneReleaseContract.ps1`, `Test-ReleaseIntegrityTamper.ps1`,
    `Test-StandaloneLayout.ps1` and `Test-GitHubActionsContract.ps1`: passed;
  - effective-line checker tests: 15 passed; the fresh ratchet passed over 619 files with 0 above 1,500, 20 from
    701-1,500 and 14 from 501-700, with no violation and no Batch 22 file above 500;
  - strict UTF-8/no-BOM/final-newline/trailing-whitespace checks and Batch 22 `git diff --check`: passed.
- No release was built for this individual batch; Phase 79 remains in progress. The smoke-release mandatory split
  debt is closed and the 701-1,500 queue falls from 21 to 20 files.

### 2026-08-24 - Phase 79-D batch 23: modular Stock Monitor JavaScript Surface

- Split `art-packages/samples/stock-monitor/surface/main.js` from 1,435 effective / 1,549 physical lines into an
  85-effective / 93-physical-line registration facade and 10 responsibility-owned modules for constants and state,
  bounded data normalization, markup/style, actions, summary DOM, market DOM, chart painting, chart interaction,
  render coordination and lifecycle ownership. The modules range from 74 to 180 effective lines; the largest is
  `data.js` at 180 / 187, so every resulting Surface file is below the 500-line acceptable limit.
- Added the optional package-local `surface/main.js.sources.json` convention instead of changing the public Surface
  protocol. For JavaScript variants, the daemon loads descriptor sources in declared order and the existing entry
  last inside one strict IIFE, then exposes the result as the same single leased JavaScript resource expected by
  Hook. A package without the descriptor still returns its entry bytes exactly; no `SurfaceVariant`, Hook or
  plugin-CLI compatibility field was added.
- Kept mount and remount validation on the same loader and bounded the new assembly boundary:
  - descriptors are UTF-8 JSON, at most 64 KiB, schema version 1 and contain 1-32 unique `.js` paths that cannot
    repeat the entry;
  - every source uses the existing canonical immutable-package containment resolver, must be UTF-8 and is capped by
    the existing 512 KiB JavaScript source limit; checked aggregate arithmetic also caps the final wrapper, modules,
    separators and entry at 512 KiB;
  - mount responses retain the stable `invalid_surface_javascript` code but no longer expose filesystem paths or OS
    read errors to the client; detailed errors remain in daemon stderr.
- Post-split security, lifetime and performance hardening:
  - host-provided history, order-book, favorite and display-text values are independently bounded to 2,000 rows,
    10 levels, 8 favorites and 400 characters before rendering; dynamic state remains assigned through DOM text
    nodes rather than interpolated into markup;
  - downsampling and chart extrema/volume scans use bounded single passes without per-bucket `slice`/`map`/`reduce`
    arrays, and the chart paint key prevents a full redraw when revision, view, period and history length are
    unchanged;
  - the delayed initial refresh is now owned and cancelled with the other timers; cleanup cancels hover/resize
    animation frames, disconnects the observer and releases stylesheet and chart caches; ResizeObserver callbacks
    now allocate and execute no full chart redraw while the Surface is suspended.
- Split the expanded Stock Monitor PowerShell test implementation into a 306-effective / 331-physical-line contract
  and a 287 / 296 helper module. Source and ZIP contracts now resolve and assemble the descriptor sources, require
  the exact 10-module package payload, and verify that every declared module is included in the built ZIP. The VM
  contract exercises hostile collection caps and carries static regression sentinels for the bounded chart and
  lifecycle paths.
- Added daemon regression coverage for exact legacy entry bytes, ordered modular assembly, repeated-entry rejection,
  package escape rejection and the aggregate 512 KiB limit. The aggregate case deliberately uses a source exactly
  at the per-file limit so only wrapper/entry overhead can trigger the aggregate error.
- Explicit residual boundaries:
  - the descriptor is a Loom package/daemon convention, not a new cross-repository Surface protocol field; a raw
    consumer that bypasses Loom assembly cannot execute the split entry by itself;
  - canonical containment assumes the installed package tree is not concurrently rewritten while it is read. A
    local principal that can mutate that tree can already replace the entry JavaScript; no platform-specific
    directory-handle/open-at dependency was introduced in this structural batch;
  - the performance proof covers bounded complexity, allocation sentinels and VM behavior, not a browser timing
    benchmark. Canvas throughput remains a candidate for a future measured workload rather than a speculative
    rewrite.
- Fresh local evidence after all Batch 23 changes:
  - workspace `cargo fmt --all -- --check` passed; `cargo check --locked -p loom-daemon` passed; focused daemon
    JavaScript Surface tests passed 2 tests with 279 filtered out;
  - all 11 descriptor-ordered JavaScript files parsed, and the Stock Monitor Surface VM contract passed;
  - Stock Monitor and seven-package source contracts passed; a fresh seven-package build under Loom `.tmp` passed
    both the packaged sample-Art contract and packaged Stock Monitor contract, then the temporary build was removed;
  - effective-line checker tests passed all 15 tests; the fresh ratchet passed over 631 files with 0 above 1,500,
    19 from 701-1,500 and 13 from 501-700, with no violation and no Batch 23 source/test file above 500;
  - strict UTF-8/no-BOM/final-newline/trailing-whitespace checks and scoped `git diff --check` passed.
- No release was built for this individual batch; Phase 79 remains in progress. The Stock Monitor mandatory split is
  closed, the 701-1,500 queue falls from 20 to 19 files, and the 501-700 queue falls from 14 to 13 files.

### 2026-08-24 - Phase 79-G batch 24: Art Store package, persistence and HTTP boundaries

- Split `apps/art-store/src/lib.rs` from 1,377 effective / 1,497 physical lines into a stable crate facade and
  responsibility modules for public models/errors, validation, bounded ZIP intake, manifest projection, catalog
  scanning, storage paths/reads, global and certification indexes, publisher identity/key rotation, signature
  verification, publication, filesystem safety and persistence. The original tests remain behind the crate facade,
  while post-split security regressions have their own module. The server tests are separately owned by
  `main_tests.rs`.
- Every resulting Art Store source file is below the normal 500-effective-line ceiling. The largest are the
  crate facade at 483 effective / 521 physical lines and the cohesive HTTP server at 496 / 551; publisher ownership
  is 270 / 288, the security suite is 200 / 219 and atomic persistence is 177 / 194. The remaining modules range
  from 17 to 135 effective lines. No 501-700 cohesion exception was added.
- Preserved the crate-root API through explicit re-exports: catalog/publication/publisher/storage functions,
  constants and serde models remain available from `loom_art_store`. Existing camelCase/snake_case wire fields,
  route names, success content types, package paths, SHA-256 sidecar format, canonical signature digest and publisher
  rotation message are unchanged. `MAX_PUBLISHED_ZIP_BYTES` is an additive public budget constant. The pure
  structural boundary passed the same 17 tests before intentional hardening began.
- Post-split archive, path and signature hardening:
  - published ZIPs are capped at 64 MiB compressed, 4,096 entries, 128 MiB per entry and 512 MiB expanded; entries
    above 1 MiB with a compression ratio over 200:1, unsafe/traversing/backslash names and ZIP symbolic links are
    rejected before manifest or signature processing;
  - manifest and signature documents are independently capped at 1 MiB. Canonical package hashing streams through
    a 64 KiB buffer instead of retaining every expanded entry and checks declared length plus an extra byte;
  - stored Art, binary, framework, index and sidecar reads require bounded regular files contained by the canonical
    store root and reject Unix symbolic links and Windows reparse points. Catalog scans apply the same package bound
    and do not follow linked Art directories or files.
- Post-split persistence and concurrency hardening:
  - publisher mutations and publication/index assignment use an `fs2` store-wide exclusive lock with a five-second
    contention timeout, preventing duplicate global IDs and lost JSON updates across threads or processes;
  - JSON, package and sidecar writes use uniquely claimed temporary files, flush data and atomically replace the
    destination. Windows replacement uses `MoveFileExW` with replace/write-through flags; Unix replacement syncs
    the parent directory;
  - the publish transaction now keeps catalog identity/version checks, immutable package activation, sidecar update
    and global-ID assignment under the same lock. Concurrent different-package publications receive unique IDs,
    while concurrent identical publications are idempotent and retain one assignment.
- Post-split HTTP, exposure and resource hardening:
  - the server uses four workers behind a bounded 32-connection queue, 15-second socket timeouts, 64 KiB header and
    64 MiB request-body ceilings, and 503 backpressure instead of spawning an unbounded thread per connection;
  - request lines require an origin-form target and HTTP/1.0 or HTTP/1.1. Headers require strict UTF-8 and valid
    token names, reject whitespace-before-colon, malformed/control-bearing fields, any `Transfer-Encoding`,
    conflicting lengths, truncated bodies and bytes beyond the declared body;
  - publisher registration is accepted only from a loopback TCP peer. This closes the confirmed externally bound
    server namespace-claim path while retaining the local daemon contract. A built-server network smoke proved a
    loopback registration/read succeeds and the same endpoint reached through a non-loopback interface returns 403;
  - request/parser, ZIP/JSON and filesystem failures no longer expose local paths or parser internals in responses.
    Package/stored-resource limit failures return 413, and detailed internal diagnostics remain on stderr.
- Regression coverage grew from 17 to 25 passing Rust tests: 21 library tests and four server tests. New cases cover
  archive byte/entry/document/compression budgets, traversal/backslash/symlink ZIP metadata, concurrent unique and
  identical publications, strict HTTP framing and headers, shortened/excess bodies, response redaction and the
  non-loopback registration gate. Existing tests continue to cover publication conflicts and idempotent sidecars,
  stable global IDs, publisher rotation and active-key signature verification.
- Explicit residual boundaries:
  - canonical path and reparse checks use path-based platform APIs, so an uncooperative local principal that can
    replace the protected store root concurrently retains an ordinary check/open or check/write TOCTOU window.
    Closing it requires handle-relative `openat`/no-follow or Windows handle traversal and a defined local-writer
    permission model, not a facade extraction;
  - catalog GET and publication identity checks still scan bounded package ZIPs. Publication holds the store lock
    during that scan, so a large catalog can cause another writer to hit the five-second timeout. A trusted immutable
    catalog cache needs measured invalidation semantics before replacing the current source-of-truth scan;
  - HTTP responses remain complete bounded `Vec` values: a worker can retain a 64 MiB request plus a binary/framework
    response of up to 256 MiB, and four workers therefore have a high but finite theoretical peak. Streaming files
    requires a response-ownership/API change and runtime measurement; no unmeasured compatibility rewrite was mixed
    into this batch.
- Fresh local evidence after all Batch 24 code changes:
  - workspace `cargo fmt --all -- --check` passed; `cargo test --locked -p loom-art-store` passed all 25 tests;
  - focused direct-dependent `cargo test --locked -p loom-daemon art_store --lib` passed 3 tests with 278 filtered
    out, and `cargo build --locked -p loom-art-store -p loom-daemon` passed;
  - the built Art Store network smoke passed health, loopback registration/read and non-loopback 403 assertions;
  - repaired `Test-LoomArtStoreGlobalId.ps1` passed its PowerShell 5.1 parse and the real two-daemon smoke: stable
    publisher/global IDs across two signed versions, catalog propagation and clean-store installation all passed;
  - the effective-line checker passed all 15 tests; the fresh ratchet passed over 645 files with 0 above 1,500,
    18 from 701-1,500 and 13 from 501-700, with no violation and no Art Store source above 500;
  - all 17 Art Store Rust/manifest files passed UTF-8/no-BOM/LF/final-newline/trailing-whitespace checks, and
    `git diff --check` passed.
- The initial repository-owned `Test-LoomArtStoreGlobalId.ps1` run exposed stale test infrastructure rather than a
  product bypass: the 285-physical-line script sent no administrator Bearer token and stopped at `unauthorized`; a
  temporary authenticated diagnostic then exposed its stale/missing `cloud_api` framework fixture through
  `framework_not_ready`. The script now generates a per-run token without printing it, sends the Bearer credential,
  can rebuild current Debug framework artifacts on request and asserts framework readiness instead of discarding the
  install response. It trusts the signed Art publisher without changing the unrelated unsigned first-party framework
  policy, and the final authenticated publish/install smoke passed. The separate 1,242-effective-line
  `Invoke-LoomFrameworkArtStoreHookSmoke.ps1` remains mandatory Phase 79-G split debt.
- No release was built for this individual batch; Phase 79 remains in progress. The Art Store mandatory split is
  closed and the 701-1,500 queue falls from 19 to 18 files.

### 2026-08-24 - Phase 79-G batch 25: plugin authoring CLI, package I/O and conformance boundaries

- Split `apps/plugin-cli/src/lib.rs` from 1,288 effective / 1,358 physical lines into a 42 / 51 lexical facade and
  focused CLI dispatch, package validation, signing/trust, deterministic package creation, scaffolding, conformance,
  filesystem-safety and test modules. Lexical `include!` ownership deliberately preserves the existing private symbol
  graph without widening the crate API beyond `run`; the help text, schema names, commands and success output remain
  unchanged.
- Every resulting plugin-cli Rust file is below the normal 500-effective-line ceiling. The largest retained legacy
  suite is 436 effective / 468 physical lines; filesystem safety is 319 / 342, validation is 312 / 325, the new
  security suite is 208 / 234, deterministic packaging is 199 / 207, and all other files range from 4 to 119
  effective lines. No 501-700 cohesion exception was added.
- Post-split filesystem and resource hardening:
  - package roots, nested directories, payloads, manifests, executables and destinations reject Unix symbolic links,
    Windows reparse points and non-regular special files. Contained payload paths are canonical-root checked and are
    reopened without following the final link (`O_NOFOLLOW` on Unix and `FILE_FLAG_OPEN_REPARSE_POINT` on Windows);
  - manifest/scene JSON reads are capped at 1 MiB and signing-key reads at 64 KiB, with declared-size and extra-byte
    checks. Key generation now uses private-permission temporary files and atomic replacement rather than a direct
    path write; signing and trust operations parse the already validated bounded handle instead of checking and then
    reopening an attacker-replaceable key path;
  - package output and SHA-256 sidecar destinations reject links and non-files. An output resolving inside its source
    tree is rejected before directory creation, preventing self-inclusion and source truncation.
- Post-split packaging and persistence hardening:
  - package enumeration remains capped at 4,096 files and 512 MiB uncompressed, rejects links/reparse points and
    case-insensitive collisions, then streams each revalidated file into the deterministic Deflate ZIP instead of
    allocating every payload. A second streaming 64 KiB pass computes the archive digest without retaining the ZIP;
  - ZIP and sidecar data are written to uniquely claimed same-directory temporaries, flushed and atomically replaced.
    Windows uses long/UNC-aware `MoveFileExW` replacement with replace/write-through flags; Unix syncs the parent
    directory. Existing ZIP ordering, permissions, compression, `.zip.sha256` naming and success report are preserved;
  - the pack path now reuses its completed tree inspection during contract validation, removing one redundant full
    `O(files + directories)` traversal while retaining per-file reopen/revalidation during streaming.
- Conformance hardening retains the validated canonical executable open through process completion, passes the
  canonical validated Art root in the request, keeps the existing 30-second, four-process and 8 MiB-per-stream limits,
  and no longer reflects child-controlled stdout, stderr or failure-status strings in CLI errors. Errors report only
  exit code and bounded byte counts; the successful protocol request/response and success message are unchanged.
- Regression coverage grew from 9 to 18 passing Rust tests. New cases cover manifest bounds, source-contained output,
  byte-for-byte deterministic streaming ZIPs, replacement of existing archive/sidecar files, package-root/payload,
  archive, sidecar and output-parent links/reparse points, signing-key link rejection without target overwrite, and
  conformance diagnostic suppression. The existing end-to-end test still performs keygen, sign, trust, deterministic
  pack, framework/Art installation, real process conformance and revocation.
- Explicit residual boundaries:
  - component validation and atomic destination ownership remain path based. The retained Windows executable handle
    denies write/delete sharing, but an uncooperative Unix principal that can replace checked package parents or the
    canonical executable path retains an ordinary parent-component TOCTOU window. Closing it requires handle-relative
    `openat`/`fexecve` or Windows directory-handle traversal and a defined hostile-local-writer model;
  - signature document creation and trust-store ACL/read/write semantics remain owned by the still-oversized
    `loom_plugin_security` crate. The CLI now rejects unsafe inputs/destinations and protects private key creation, but
    duplicating the canonical-signature implementation here would create two security authorities; its atomic and
    bounded persistence audit remains scheduled with that crate's mandatory split;
  - an archive and its digest are each crash-safe but cannot be atomically committed as a two-file transaction, and a
    hostile framework can still fill its bounded-lifetime conformance temp tree before best-effort recursive cleanup.
    Layout-level transactional publication and runtime disk quotas require product contracts owned outside this CLI.
- Fresh local evidence after all Batch 25 changes:
  - workspace `cargo fmt --all -- --check` passed; `cargo test --locked -p loom-plugin-cli` passed all 18 tests and
    `cargo build --locked -p loom-plugin-cli` passed;
  - the effective-line checker passed all 15 tests; the fresh ratchet passed over 654 files with 0 above 1,500,
    17 from 701-1,500 and 13 from 501-700, with no violation and no plugin-cli source above 500;
  - scoped `git diff --check` passed for plugin-cli, the workspace lockfile and this Phase 79 record.
- No release was built for this individual batch; Phase 79 remains in progress. The plugin-cli mandatory split is
  closed and the 701-1,500 queue falls from 18 to 17 files.

### 2026-08-24 - Phase 79-G batch 26: framework Art Store/Hook smoke boundaries

- Split `scripts/Invoke-LoomFrameworkArtStoreHookSmoke.ps1` from 1,242 effective / 1,363 physical lines into a
  447 / 488 orchestration script and eight responsibility-owned modules for assertions/redaction, real-path safety,
  process lifecycle, loopback HTTP, Hook Bridge WebSockets, deterministic fixture archives, service fixtures and Art
  fixtures. The pure structural extraction was checked against the original script with seven extracted segments,
  four retained segments and zero mismatches before behavior hardening.
- Every resulting script is below the normal 500-effective-line ceiling. The two fixture-definition modules are
  223 / 232 and 216 / 231; process ownership is 212 / 238, archive ownership is 208 / 224, Hook Bridge ownership is
  156 / 171, HTTP ownership is 109 / 126, path ownership is 93 / 105 and assertion/redaction ownership is 70 / 82.
  The new focused module test is 192 / 206 and the split-aware standalone release contract remains 477 effective
  lines. No 501-700 cohesion exception was added.
- The pre-hardening real smoke exposed a current product-contract mismatch in the old fixture builder rather than a
  daemon regression. Windows `Compress-Archive` emitted nested entry names such as `runtime\main.ps1`; the hardened
  Art Store correctly rejects backslash archive names, so only the root-manifest-only `store-cloud-art` and
  `store-workflow-art` appeared in the catalog. The structural-parity run reproduced the same two-Art result.
- Post-split filesystem, archive and publication hardening:
  - caller-selected repository/package/artifact/evidence paths, binaries, framework ZIPs, fixture sources, process
    working directories and log destinations now resolve to the expected file kind and reject a reparse point at the
    selected endpoint. Loaded smoke modules are independently rejected when they are reparse points;
  - fixture entry paths must be relative, reject empty/dot/dot-dot, invalid, trailing-dot/space and Windows device
    segments, remain canonically inside a private stage and reject reparse points in copied directory trees;
  - fixture ZIPs are streamed through `ZipArchive` in sorted order with forward-slash names and a fixed 1980 timestamp.
    ZIP and SHA-256 sidecar files are published from unique same-directory temporaries with atomic single-file
    replacement, and source payloads are streamed rather than retained as a second in-memory copy. This repaired the
    real six-Art catalog contract while making repeated fixture ZIP bytes deterministic.
- Post-split network, lifetime and diagnostic hardening:
  - every JSON helper accepts only unauthenticated loopback HTTP URIs. HTTP readiness uses one overall deadline with
    short attempts; TCP readiness disposes both the client and asynchronous wait handle on every path;
  - Hook Bridge sends and receives are capped at 1 MiB, fragmented text is accumulated as bytes in a bounded
    `MemoryStream`, decoded once with strict UTF-8 and governed by one cancellation deadline. Terminal-response
    scanning now has a 30-second overall deadline instead of allowing 64 independent ten-second waits;
  - spawned process binaries, working directories and log endpoints are revalidated immediately before launch.
    Cleanup performs bounded descendant discovery before and after parent termination, stops late children, waits with
    a bound and disposes every `Process` object. The success summary/stdout is now published only after cleanup;
    failed discovery, a lingering process or failed Hook/MCP teardown changes the result to `failed`;
  - failed Hook executions no longer serialize the child-controlled response into errors. Summary, cleanup and
    outward failure messages redact the daemon credential and Bearer values and cap retained diagnostic text; failed
    WebSocket, Hook and MCP cleanup is visible as a bounded warning rather than silently discarded.
- Added `Test-FrameworkArtStoreHookSmokeModules.ps1` to the Windows CI pre-generated-output gate. It covers unsafe and
  reserved archive paths, loopback-only HTTP, secret/Bearer redaction, WebSocket bounds, junction rejection,
  real parent/child process-tree teardown, byte-for-byte deterministic forward-slash ZIPs, normalized timestamps,
  sidecars, file/directory copies and stage traversal rejection. Private temporary-directory cleanup revalidates the
  endpoint and expected root before recursive removal. The standalone release contract parses and aggregates the
  wrapper plus all eight modules and asserts the new security/resource boundaries.
- Fresh local evidence after all Batch 26 changes:
  - `Test-StandaloneReleaseContract.ps1`, `Test-FrameworkArtStoreHookSmokeModules.ps1` and
    `Test-GitHubActionsContract.ps1` passed under Windows PowerShell 5.1;
  - the real Debug `Invoke-LoomFrameworkArtStoreHookSmoke.ps1 -SkipBuild` passed with all six catalog IDs, four ready
    frameworks, six `loom.hook.v1` formal executions, two MCP candidates with selected index 1 and a value-kind Python
    output. The scoped post-run process audit found no daemon, Art Store or fixture-process leak;
  - the effective-line checker passed all 15 tests; the fresh ratchet passed over 663 files with 0 above 1,500,
    16 from 701-1,500 and 13 from 501-700, with no violation and no Batch 26 source/test file above 500;
  - strict PowerShell parsing, UTF-8/no-BOM/final-newline/trailing-whitespace validation and scoped
    `git diff --check` passed.
- Explicit residual boundaries:
  - `PackageDir`, `EvidenceRoot`, `FrameworkArtifactRoot` and the host Python selected from `PATH` remain explicit
    operator-trust inputs. This batch validates their selected endpoints and file kinds but does not impose a release
    root, publisher signature or host-interpreter allowlist that the smoke CLI has never promised;
  - free-port discovery and later bind retain the ordinary cross-process reservation race. Eliminating it requires
    passing an owned listener into the Rust/Python processes and changes their launch protocol;
  - Windows PowerShell 5.1 `Invoke-RestMethod` buffers each local JSON response. Timeouts bound lifetime but not bytes;
    replacing it needs a shared bounded HTTP client and is intentionally not duplicated inside one smoke script;
  - archive and checksum publication are each crash-safe but are not a two-file transaction. Process ownership relies
    on bounded CIM/WMI discovery and force termination rather than a Windows Job Object; discovery or a lingering
    descendant now fails the smoke, but a hostile child could still escape after the final bounded inspection.
- No release was built for this individual batch; Phase 79 remains in progress. The framework Art Store/Hook smoke
  mandatory split is closed and the 701-1,500 queue falls from 17 to 16 files.

### 2026-08-24 - Phase 79-G batch 27: public protocol and Surface ownership boundaries

- Split `crates/loom_protocol/src/lib.rs` from 746 effective / 821 physical lines into a 23 / 32 crate facade and
  responsibility-owned schema, package, execution, Art-runtime, validation and test modules. Split
  `crates/loom_protocol/src/surface.rs` from 1,218 effective / 1,334 physical lines into a 49 / 64 Surface facade and
  package/host metadata, resources/ports, scenes/patches/events, actions/results, validation and test modules. Private
  implementation modules are re-exported from the same crate-root and `surface` paths, so existing Rust imports,
  constants, error types and schema access paths remain unchanged.
- Every resulting Batch 27 protocol file is below the normal 500-effective-line ceiling. The largest files are
  `package.rs` at 282 / 313, `surface/tests.rs` at 281 / 303, `surface/actions.rs` at 270 / 312 and
  `surface/validation.rs` at 258 / 276. The remaining files range from 22 to 176 effective lines; no 501-700 cohesion
  exception was added. All 11 embedded JSON schema constants retain their original names and same-depth
  `include_str!` paths.
- The structural pass preserved all Serde field names, tags, defaults and unknown-field behavior. Independent review
  compared the new facades and submodules with the checked-in implementations and found no public API, wire-format,
  constant, validation-error or test-migration regression. Direct consumers in the daemon, plugin CLI and tool
  registry compile through the existing crate-root re-exports.
- Post-split performance and memory hardening:
  - framework protocol advertisement now uses an order-preserving `HashSet<&str>` membership index. It retains the
    primary protocol first and the first occurrence of every supported protocol while reducing duplicate detection
    from quadratic to expected linear time; negotiation constructs the advertised list once rather than rebuilding it
    on the error path;
  - Surface scene validation now uses an explicit heap stack instead of recursive calls. Reversed child insertion
    preserves the former left-to-right depth-first error order, while a 4,096-level regression proves the validator no
    longer consumes one call frame per scene node. The test dismantles the synthetic tree iteratively so its destructor
    does not obscure the validator result.
- Framework execution previously bypassed the existing 4 MiB framework-metadata bound and used unbounded
  `read_to_string` immediately before protocol negotiation. It now reuses the framework storage bounded reader, which
  rejects linked/non-file endpoints, checks declared size, limits the open-handle read to one extra byte and rejects
  growth beyond the ceiling. Invalid UTF-8 is reported as a protocol encoding error rather than as a missing package.
  Focused tests prove an over-limit manifest is rejected before JSON parsing and invalid UTF-8 takes the structured
  protocol-error path.
- Security conclusions retained rather than converted into incompatible protocol changes:
  - the schema intentionally permits slash/dot characters in generic Surface identifiers and permits additional
    properties in several envelopes, so this batch did not add path semantics or blanket `deny_unknown_fields`;
  - the daemon already binds snapshot/patch/event identities to their routed instance and attachment, checks current
    revisions, applies patch operations through node/index/root/path guards, revalidates the final node tree, validates
    host-issued resource leases and caps action uploads. Tightening the language-neutral validator beyond those current
    schema contracts requires a separately versioned cross-host decision;
  - the explicit validator stack does not by itself bound Serde parsing, recursive `SurfaceNode` destruction or the
    daemon's recursive node-mutation helpers. Declarative scene files have a 1 MiB byte ceiling, and persisted Surface
    state has a separate JSON-depth guard, but a shared scene depth/node budget remains follow-up work at the actual
    parse and mutation boundaries;
  - the shared bounded reader still has the ordinary metadata-check-to-open race, and persistent MCP host identity is
    based on path/metadata/manifest rather than an executable content hash. Closing those hostile-local-writer windows
    requires handle-relative or file-identity-aware design, especially on Windows, rather than duplicating a partial
    path check in the protocol crate.
- Fresh local evidence after all Batch 27 changes:
  - `cargo test --locked -p loom_protocol` passed all 28 tests; `cargo test --locked -p loom_tool_registry` passed its
    complete suite, including the new manifest-boundary cases; direct daemon, plugin CLI and tool-registry consumer
    compilation passed;
  - the effective-line checker passed all 15 tests; the fresh ratchet passed over 675 files with 0 above 1,500,
    14 from 701-1,500 and 13 from 501-700, with no violation and no Batch 27 protocol file above 500;
  - package-scoped formatting, UTF-8/no-BOM/final-newline/trailing-whitespace validation and scoped
    `git diff --check` passed. The local Rust toolchain does not have the Clippy component installed, so the attempted
    `cargo clippy -p loom_protocol --all-targets -- -D warnings` gate was unavailable rather than reported as passed.
- No release was built for this individual batch; Phase 79 remains in progress. Both public-protocol mandatory splits
  are closed and the 701-1,500 queue falls from 16 to 14 files.

## 2026-08-24 Batch 28 - credential and Art-settings private-store boundaries

- Split `crates/loom_tool_registry/src/credentials.rs` from 964 effective / 1,010 physical lines into a 14 / 21
  stable facade and focused type, error, persistence, protection, value, grant and test modules. Split
  `crates/loom_tool_registry/src/art_settings.rs` from 833 effective / 901 physical lines into a 16 / 23 stable facade
  and model, persistence, metadata, binding, parameter, validation and test modules. Existing
  `loom_tool_registry::credentials::*` and `loom_tool_registry::art_settings::*` public paths, Serde field names,
  filenames and schema numbers remain unchanged.
- Every resulting Batch 28 module is below the normal 500-effective-line ceiling. The largest files are
  `art_settings/tests.rs` at 425 / 448, `credentials/tests.rs` at 405 / 420, the shared `private_store.rs` at 200 / 219,
  `art_settings/store.rs` at 183 / 198 and `credentials/grants.rs` at 187 / 194. Production responsibility modules
  otherwise range from 37 to 156 effective lines; no 501-700 cohesion exception was added.
- Added one crate-private private-document boundary shared by both stores:
  - `fs2` sibling-file locks serialize cross-process read-modify-write operations, eliminating lost updates and the
    fixed temporary-file collision in Art settings;
  - writes use unique `create_new` temporary files, flush content before replacement, restore private permissions,
    and sync the parent directory on Unix. Windows replacement now canonicalizes only the parent rather than following
    the destination's final component;
  - final file and lock opens use Unix `O_NOFOLLOW` or Windows `FILE_FLAG_OPEN_REPARSE_POINT`, then inspect the opened
    handle metadata and reject linked/reparse or non-file objects. Bounded reads take at most one byte beyond the limit
    so growth after metadata inspection cannot cause an unbounded allocation.
- Credential hardening after the split:
  - credential documents are limited to 4 MiB, individual input values to 1 MiB, and JSON credential values to 32
    nesting levels. Output is size-checked before replacement, and unknown schema versions are rejected rather than
    silently interpreted;
  - invalid stored expiration strings are treated as expired by every Art, MCP and global-value grant path. This
    closes the former fail-open parse behavior while keeping management summaries/reveal available for diagnosis;
  - requested grant membership now uses an order-preserving `HashSet<&str>` index instead of repeatedly scanning the
    request list. Concurrent-upsert tests use a synchronized start and prove all distinct records survive.
- Art settings hardening after the split:
  - documents are limited to 4 MiB and 32 nesting levels, with explicit caps for Arts, per-Art maps, individual default
    JSON values and free-form text. Future schema versions and valid-but-over-budget documents are rejected without
    overwriting or misclassifying them as syntax corruption;
  - corruption recovery remains compatible for malformed/truncated JSON, but backups now use private `create_new`
    files and retain only the three newest recovery copies. Canonical replacement uses the shared unique-temporary
    path rather than `art-user-settings.json.tmp`;
  - metadata projection defensively removes defaults for parameters declared secret even if a non-daemon caller
    constructs an invalid `ArtUserSettings`. The daemon management handler was independently rechecked: it already
    rejects secret defaults, stores `secretValues` only through `CredentialStore`, and persists only credential
    bindings in Art settings and tool metadata.
- Independent review found no blocking Rust API, Serde or file-schema compatibility regression. Residual boundaries
  retained rather than represented as complete fixes:
  - non-Windows credential protection remains the historical permission-restricted Base64 representation. Replacing
    it truthfully requires a versioned keyring/envelope-encryption migration, not a renamed encoding marker;
  - final-component no-follow handles close the direct file redirection addressed in this batch, but a hostile local
    writer with authority to mutate ancestor directories can still race pathname permission repair or replace path
    identities. Full closure requires handle-relative traversal and file-identity comparison across Windows and Unix;
  - `control_plane_root_for_tool` still derives the credential root from installed `artPackage.dir` ancestry (with an
    environment fallback). Existing external-package and cloud-path contracts deliberately allow Art directories
    outside the normal control-plane tree, so binding this helper to a single root requires an explicit runtime API and
    compatibility migration rather than a partial path-string check;
  - corruption-backup pruning scans the small private control-plane directory only on recovery. It is O(sibling files)
    but no longer permits unbounded retained Art-settings backups.
- Fresh local evidence after all Batch 28 changes:
  - `cargo fmt --all -- --check` passed; `cargo test --locked -p loom_tool_registry` passed all 224 tests, including
    synchronized credential/settings concurrency, size, schema, depth, backup-retention and secret-projection cases;
  - `cargo check --locked -p loom-daemon -p loom-plugin-cli -p loom_tool_registry` passed. The full daemon package test
    gate passed 281 library tests and 8 integration tests;
  - the effective-line checker passed all 15 tests; the fresh ratchet passed over 690 files with 0 above 1,500,
    12 from 701-1,500 and 13 from 501-700, with no Batch 28 file above 500;
  - scoped `git diff --check` and UTF-8/no-BOM/final-newline/trailing-whitespace checks passed. The already-known local
    Clippy component absence was not misreported as a passed lint gate.
- No release was built for this individual batch; Phase 79 remains in progress. The mandatory 701-1,500 queue falls
  from 14 to 12 files.

## 2026-08-24 Batch 29 - durable run evidence and workflow persistence boundaries

- Split `crates/loom_durable/src/run_store.rs` from 1,068 effective / 1,161 physical lines into a 66 / 76 public
  facade and focused core-validation, in-memory, SQLite, SQLite-path and test modules. Split
  `crates/loom_workflow_store/src/lib.rs` from 858 effective / 975 physical lines into a 29 / 34 public facade and
  error, model, store, filesystem, validation, graph-to-YAML, YAML-to-graph, helper and test modules. Existing
  crate-root exports, trait signatures, error variants, Serde names, SQLite schema version, workflow filenames and
  index metadata shape remain available at their original paths.
- Every resulting Batch 29 file is below the normal 500-effective-line ceiling. The largest production modules are
  `run_store/sqlite.rs` at 445 / 473, workflow `storage.rs` at 273 / 298 and workflow `store.rs` at 220 / 244. Tests
  were split again rather than exceeding the ceiling: durable SQLite tests are 318 / 338, durable memory tests are
  195 / 216 and workflow tests are 359 / 398. The remaining modules range from 3 to 189 effective lines; no 501-700
  cohesion exception was added.
- Durable run-store hardening after the structural pass:
  - run documents are capped at 8 MiB, event fields at 2 MiB, JSON nesting at 64 levels and one caller-supplied event
    batch at 4,096 items. Both in-memory and SQLite writes enforce the same limits; SQLite reopen validates raw text
    length before parsing stored run/event JSON;
  - SQLite writers now start `IMMEDIATE` transactions. A synchronized eight-connection regression exposed that
    rewriting `PRAGMA journal_mode=WAL` on every open could produce a Windows `locking protocol` failure, so existing
    WAL stores now use a read-only confirmation and only non-WAL stores perform the migration;
  - the primary database path is resolved through a checked regular parent and opened with SQLite's
    `SQLITE_OPEN_NOFOLLOW`. Existing linked/reparse or non-file destinations are rejected, a newly created parent is
    made private, and the database file receives private permissions. The existing foreign-key, WAL, FULL synchronous,
    busy-timeout, schema-version, quick-check, record-integrity and deterministic recovery contracts remain intact.
- Workflow-store security, correctness and performance hardening:
  - one crate-private filesystem boundary now provides an `fs2` cross-process lock, final-handle no-follow/reparse
    checks, bounded UTF-8 reads, unique `create_new` temporaries, flush-before-replace, Windows parent-only
    canonicalization, Unix parent sync and private file/directory permissions. Save/delete/list read-modify-write
    operations share the lock; workflow and index replacement are individually atomic;
  - workflow YAML is capped at 8 MiB, the JSON index at 4 MiB, nesting at 64 levels, YAML values at 100,000, nodes at
    4,096, edges at 16,384 and indexed workflows at 4,096. Workflow IDs now additionally reject control characters,
    overlong names, trailing dot/space and Windows device names;
  - graph encoding rejects missing/blank node IDs instead of silently discarding those nodes. Recursive dependency
    collection validates every non-sticker Art identity before returning it, and directory listing now reports
    malformed YAML instead of publishing a misleading zero-node entry;
  - graph encoding no longer clones the complete node/edge arrays and no longer scans all edges for every node. An
    order-preserving target index reduces edge association from quadratic `nodes x edges` work to expected linear
    indexing plus emitted edges. Save metadata is derived from the already decoded graph, and list metadata parses each
    YAML document once rather than twice.
- Independent API, security, performance, test and architecture reviews were sampled back to their cited lines. The
  actionable missing-ID, dependency-validation, malformed-listing, SQLite path and concurrent-WAL findings were fixed.
  Explicit residual boundaries remain:
  - a workflow YAML file and `workflow_index.json` are individually crash-safe but not one multi-file transaction. A
    crash after YAML replacement can leave an orphan that the existing list reconciliation recovers;
  - final-component no-follow, canonical-parent selection and parent-only Windows replacement do not provide a fully
    handle-relative filesystem namespace. A hostile process able to rename ancestor directories can still race path
    identity checks. Full closure requires `openat2`/handle-relative traversal or a custom SQLite VFS on each platform;
  - SQLite accepts an explicit operator-selected path rather than a fixed trusted root. Existing external override
    parents are not recursively ACL-rewritten, so sidecar confidentiality still depends on that parent's access policy;
  - SQLite open-time integrity validation and interrupted-run recovery remain proportional to all stored records, and
    `get_events` retains the public all-events return contract. Pagination, retention and background validation require
    a versioned persistence/API decision rather than a silent truncation in this refactor;
  - workflow listing remains an O(number of YAML files) reconciliation while holding the store lock. Replacing it with
    mtime/version-based incremental indexing requires a durable invalidation contract. The current bound prevents
    unbounded document size/count but not linear directory work;
  - the current Windows host lacks symbolic-link creation privilege (`os error 1314`), so the Windows symlink fixture
    took its explicit privilege-unavailable path. The SQLite flag and Windows reparse checks compile and the Unix test
    exercises the redirection contract, but a privileged Windows reparse/junction runtime gate remains release-lab
    evidence rather than something claimed by this local run.
- Fresh local evidence after all Batch 29 changes:
  - `cargo fmt --all -- --check` passed. One locked test invocation passed `loom_durable` 25 tests,
    `loom_workflow_store` 11, `loom_hook_bridge` 2, `loom_workflow_runtime` 22, daemon library 281 and daemon CLI
    integration 8, plus all related doc tests;
  - the effective-line checker passed all 15 tests. The fresh report scanned 707 files with 0 above 1,500, 10 from
    701-1,500 and 13 from 501-700, zero violations and no Batch 29 file above 500;
  - scoped `git diff --check` and UTF-8/no-BOM/final-newline/trailing-whitespace validation passed.
- No release was built for this individual batch; Phase 79 remains in progress. Both persistence-layer mandatory
  splits are closed and the 701-1,500 queue falls from 12 to 10 files.

## 2026-08-24 Batch 30 - standalone release build and verification boundaries

- Split `scripts/build-release.ps1` from 1,107 effective lines into a 269 / 287 composition entry and nine focused
  modules: catalog 142 / 153, common policy 155 / 179, plan 72 / 75, execution 136 / 145, framework packages 91 / 97,
  MCP packages 87 / 91, Art packages 100 / 107, metadata 51 / 58 and archives 200 / 213. Split
  `scripts/verify-release.ps1` from 850 effective lines into a 240 / 259 composition entry and seven focused modules:
  common verification 202 / 228, desktop payload 15 / 19, CLI/SDK payload 86 / 95, framework packages 67 / 77,
  MCP packages 113 / 121, Art packages 109 / 119 and supply-chain metadata 70 / 76.
- Preserved the build and verifier CLI parameters, dry-run schema, success JSON fields, manifest schema, package layout,
  desktop/CLI/plugin-SDK ZIP names and SHA-256 sidecars. Entry scripts load the shared layout boundary and their exact
  helper sets in an AST-verified order. `Get-GitDirty` retains the original `--untracked-files=all` behavior, and command
  logs retain their `Arguments:` field while redacting token, secret, password and API-key values.
- The standalone release contract was itself kept below the normal ceiling rather than waived: its modular structure,
  captured-process and release-security checks moved to `scripts/tests/standalone-release/ReleaseContracts.ps1` at
  121 / 131, leaving the entry test at 445 / 476. The focused path-safety suite is 147 / 164. Every Batch 30 source and
  test file is at most 500 effective lines; `LoomReleaseLayout.ps1`, which is shared by build, verify and smoke entry
  points, remains one cohesive 474 / 514 boundary.
- Post-split filesystem, integrity, memory and performance hardening:
  - package-relative paths reject rooted paths, traversal, empty segments, controls, NTFS ADS syntax, trailing dots or
    spaces and Windows device names. Release enumeration refuses reparse points, uses an explicit directory stack and
    never follows recursive links;
  - file hashing is streaming and handle-based. Verification rehashes every trust use and compares cached SHA-256 rather
    than trusting length and `LastWriteTimeUtc`; a regression replaces a file with different same-length bytes, restores
    the original timestamp and proves that verification rejects the drift;
  - manifest, catalog, sidecar, SBOM, provenance and archive-entry reads now have explicit byte bounds. ZIP validation
    applies the same safe-path policy, rejects case-insensitive normalized duplicates, caps entries at 65,536 and total
    uncompressed bytes at 16 GiB. CLI extraction additionally caps its sole `loom.exe` at 512 MiB and streams it through
    `CreateNew` instead of invoking whole-archive `Expand-Archive`;
  - payload/archive copies use locked source and create-new destination streams, flush before trust publication and
    recheck reparse boundaries. Temporary archive stages are removed leaf-first without recursive deletion following a
    newly introduced link;
  - build-command output streams through one buffered writer with a 16 MiB log cap instead of performing `Get-Item` and
    `AppendAllText` for every line. Checksum line collection and archive entry collection use generic lists rather than
    quadratic PowerShell array append loops. Full-tree checksum hashing is intentionally retained because it is the final
    independent integrity pass over generated and copied artifacts.
- Independent architecture, security, compatibility, performance and test reviews were sampled back to their cited
  lines. The stale metadata-only hash cache, incomplete ZIP normalization, unsafe whole-archive extraction, recursive
  stage deletion, per-line log I/O, weak text-only module ordering check and missing `Arguments:` compatibility field
  were fixed. Explicit residual boundaries remain:
  - PowerShell pathname APIs cannot make ancestor traversal handle-relative. A hostile local process that can rename a
    checked output ancestor may still race the immediately repeated reparse checks around directory creation, file
    creation or stage deletion. Full closure requires a platform-native handle-relative filesystem helper and identity
    comparison, not additional `Test-Path` calls;
  - build tools and optional smoke scripts keep their historical unbounded execution time. Adding forced process-tree
    termination would change long-running Cargo/npm/release behavior and needs an explicit timeout/cancellation contract;
  - package enumeration and final checksums remain O(files), and verifier trust uses may rehash a file more than once.
    Those costs are deliberate fail-closed choices; metadata-only caching was removed because it was bypassable.
- Fresh local evidence after all Batch 30 changes:
  - `Test-StandaloneReleaseContract.ps1`, `Test-SmokeReleaseModules.ps1`, `Test-ReleaseIntegrityTamper.ps1`,
    `Test-ReleasePathSafety.ps1`, `Test-ArtPluginBoundaryContract.ps1` and `Test-GitHubActionsContract.ps1` passed under
    Windows PowerShell 5.1;
  - effective-line checker tests passed all 15 tests. The fresh ratchet scanned 725 files with 0 above 1,500, 8 from
    701-1,500 and 13 from 501-700, with zero violations and no Batch 30 file above 500;
  - all `scripts/**/*.ps1` files were ASCII-safe, UTF-8 without BOM, free of trailing whitespace and ended with a final
    newline. Scoped `git diff --check` passed.
- No release was built for this individual batch; Phase 79 remains in progress. Both release-script mandatory splits are
  closed and the 701-1,500 queue falls from 10 to 8 files.

## 2026-08-24 Batch 31A - stock-api Node runtime boundaries

- Split `mcp-server-packages/stock-api/runtime/stock-api-entry.js` from 1,004 effective / 1,071 physical lines into a
  27 / 31 compatibility entry and seven responsibility modules under `runtime/stock-api`: immutable constants and
  schemas 153 / 157, request executors 223 / 229, shared helpers and bounded caches 106 / 121, response parsers 227 / 245,
  provider adapters and fallback 165 / 178, JSON-RPC server/framing 228 / 249 and bounded HTTP transport 169 / 188. The
  entry retains the original four CommonJS exports, `require.main === module` guard and vendored stock-api server path.
  The module graph is acyclic and points from the entry/server/executors toward provider, parser, helper, transport and
  constant leaves; no generated or vendored dependency was modified.
- Preserved the wrapper's version, seven-tool declaration, normalized market-series/order-book shapes, Eastmoney host
  retry order, Xueqiu/pysnowball/auto fallback, loopback-test override contract, TTL cache behavior and concurrent
  newline-delimited JSON-RPC response semantics. Package construction and verification now require the exact seven
  runtime module paths in addition to the facade.
- Post-split security, lifetime and performance hardening:
  - provider bodies reject declared or streamed data above 5 MiB, grow through one bounded byte accumulator without a
    final full-size copy, cancel failed/oversized readers and release their locks. Fetch timeouts remain abort-backed and
    provider errors do not expose request headers or credentials;
  - JSON-RPC frames remain capped at 1 MiB. At most eight upstream requests execute and sixteen complete lines wait.
    The residual input string is capped at one maximum frame plus its delimiter, including when a custom stream delivers
    a single attacker-sized `data` chunk that `pause()` cannot retract. Overflow is rejected without retaining that
    chunk, framing resumes at a newline, and terminal input-stream errors clear retained work instead of becoming an
    unhandled EventEmitter exception;
  - successful provider caches remain TTL-bound and capped at 64 entries. Host retry remains bounded by both rounds and
    an operation deadline; the structural split added no dependency, timer or process lifetime.
- Independent architecture, security, test and scope reviews were sampled back to their cited symbols. The confirmed
  single-chunk backlog bypass was fixed and covered by a recovery regression. Suggestions to duplicate upstream MCP
  protocol-version negotiation and every release-verifier assertion in the focused package test were not treated as
  stock-api behavior defects: the wrapper deliberately passes negotiation to the pinned upstream server, while the final
  release gate runs both the package contract and the full release verifier.
- Fresh local evidence after all Batch 31A changes:
  - `node --test mcp-server-packages/stock-api/runtime/stock-api-entry.test.js` passed all 8 byte-buffer, cancellation,
    JSON, concurrency, backpressure, backlog-cap and framing-recovery tests;
  - `Test-LoomStockApiMcpServer.ps1` passed the independent version/tool/quote/BJ-code/candle/bounded-history/series/
    five-day/retry/TTL-cache/order-book/source contract, including the existing oversized-frame resynchronization gate;
  - `Build-LoomMcpServerPackages.ps1` built both packages in an isolated temporary root and
    `Test-LoomMcpServerPackageContract.ps1` passed package identity, tool, archive, supply-chain, Node and exact stock-api
    module checks; the temporary root was then removed after containment verification;
  - the effective-line checker passed all 15 tests. The fresh ratchet scanned 732 files with 0 above 1,500, 7 from
    701-1,500 and 13 from 501-700, with zero violations and every Batch 31A file below 500 effective lines.
- No release was built for this individual batch; Phase 79 remains in progress. The stock-api Node runtime mandatory
  split is closed and the 701-1,500 queue falls from 8 to 7 files.

## 2026-08-24 Batch 31B - Stock Monitor PowerShell runtime boundaries

- Split `art-packages/samples/stock-monitor/runtime/main.ps1` from 1,031 effective / 1,157 physical lines into a
  154 / 171 composition and response entry plus seven responsibility modules under `runtime/lib`: immutable limits and
  process state 14 / 16, strict domain conversion 202 / 235, MCP result handling 80 / 89, output/envelope construction
  183 / 210, protocol and action parsing 216 / 255, snapshot orchestration 190 / 200 and bounded market-data transforms
  185 / 204. The entry anchors package lookup to its own runtime directory and loads the fixed
  `Constants -> Protocol -> Domain -> Mcp -> Transforms -> Snapshot -> Output` graph. Quote, history, order-book/tape,
  favorites and Surface response ordering remain unchanged.
- Preserved the package version and four MCP-call contract, thirteen market periods, action IDs and budgets, formal quote
  schema, authoritative-state fallback, empty result state-patch merge invariant, history-warning behavior, two-level
  fixture output, near-realtime tick path, exact package-local runtime paths and existing JavaScript/declarative Surface
  integration. Package construction, source tests and release verification now require the exact Stock Monitor
  PowerShell module set; the seven-package source and artifact contracts also require the exact curated ZIP name set.
- Post-split security, lifetime and performance hardening:
  - stdin is read as strict UTF-8 with a 4 MiB byte cap and a single leading BOM allowance. Oversized input is discarded
    while the pipe is drained, so retained memory stays bounded and the parent does not receive a broken pipe. A lexical
    string/escape-aware depth scan rejects nesting above 32 before Windows PowerShell's JSON parser runs; the decoded
    graph is then checked iteratively for depth and 100,000 total values;
  - request IDs, action IDs, periods, symbols, numbers, booleans and provider text use strict type boundaries. Request-ID
    truncation preserves UTF-16 surrogate pairs, invalid order-book levels fall back to their bounded ordinal, and
    string `false`, objects and arrays cannot become truthy booleans or provider numbers;
  - provider diagnostics retain safe fixture text but replace credential indicators, Bearer values, Windows/Unix/UNC/
    environment-variable/registry paths and overlong/control-bearing values before they enter stored or displayed state;
  - history retains only the newest 2,000 rows without materializing an unbounded pipeline. Arrays and `IList` inputs use
    direct tail indexes; generic enumerables use a fixed-capacity queue. Favorites and order-book projections likewise
    stop at their declared first-N bounds;
  - child-process tests read stdout/stderr concurrently, write stdin asynchronously with its own five-second timeout,
    terminate a timed-out child and dispose every process. Windows command-line quoting now doubles only backslashes
    before a quote or the closing quote instead of corrupting ordinary path separators.
- Independent architecture, security, test and scope reviews were sampled back to their cited functions. The confirmed
  pre-parser nesting exposure, UNC diagnostic disclosure, surrogate truncation, invalid order-book level, synchronous
  stdin write, Windows quoting and non-exact ZIP-set findings were fixed and covered. Explicit residual boundaries remain:
  - an oversized writer that never closes stdin can wait until the existing 50-second runtime/host budget because the
    runtime deliberately drains the host-owned pipe; adding an internal timer would require coordinated cancellation and
    process-tree semantics;
  - total node count is necessarily checked after `ConvertFrom-Json` on Windows PowerShell 5.1, which has no parser depth
    switch. The 4 MiB byte limit and pre-parser nesting scan bound that exposure, but they cannot make parsing allocation
    free;
  - a non-indexable provider enumerable still requires O(N) traversal to select its newest rows, although retained memory
    is capped at 2,000. Local manifest/runtime budget files remain trusted package input, and systems without the target
    market time zone retain the existing UTC fallback.
- Fresh local evidence after all Batch 31B changes:
  - `Test-LoomStockMonitorRuntimeHardening.ps1` (86 / 98) passed strict scalar, bounded history, order-book ordinal,
    diagnostic redaction, action correlation, 100,000-node, 4 MiB stdin and pre-parser depth regressions;
  - `Test-LoomStockMonitorArt.ps1` and `Test-LoomSampleArtPackageContract.ps1` passed from source. A fresh Release build
    produced seven packages in an isolated temporary root, and the artifact contract passed the exact ZIP/runtime module,
    certification, stock-api and executable Stock Monitor checks before the temporary root was containment-checked and
    removed;
  - `Test-SmokeReleaseModules.ps1` and `Test-StandaloneReleaseContract.ps1` passed. The effective-line checker passed all
    15 tests; the ratchet scanned 740 files with 0 above 1,500, 6 from 701-1,500 and 13 from 501-700, with zero violations;
  - scoped `git diff --check`, strict UTF-8/no-BOM/final-newline/trailing-whitespace checks and the ASCII-only check over
    all 82 `scripts/**/*.ps1` files passed. Every Batch 31B production/test file is below 500 effective lines.
- No release was built for this individual batch; Phase 79 remains in progress. The Stock Monitor PowerShell runtime
  mandatory split is closed and the 701-1,500 queue falls from 7 to 6 files.

## 2026-08-24 Batch 32 - Workflow Studio services and Hook canvas test ownership

- Split `apps/desktop/src/services/workflowStudio.ts` from 860 effective / 959 physical lines into a 41 / 44 public
  compatibility facade plus nine responsibility modules under `services/workflowStudio`: public contracts 152 / 170,
  shared limits and safe record helpers 30 / 35, JSON template inference 167 / 188, command parsing 85 / 97, MCP schema
  conversion 68 / 75, workflow graph/YAML handling 231 / 252, tool-definition normalization 175 / 190, workflow
  interface inference 154 / 162 and graph-limit validation 42 / 46. The facade preserves every original named value and
  type export; module dependencies remain one-way and no package or runtime dependency was added.
- Split `apps/desktop/src/services/hookCanvas.test.ts` from 790 effective / 852 physical lines into the retained snapshot,
  layout and presentation suite 285 / 304, shared immutable fixtures 105 / 110, graph behavior tests 98 / 108, interface
  inference tests 49 / 56 and Art bundle tests 196 / 208. The original 32 tests and their assertions are preserved exactly
  as 14 + 9 + 6 + 3 tests, and the existing `src/**/*.test.ts` discovery contract loads every partition automatically.
- Post-split security, lifetime and performance hardening:
  - imported command/JSON/YAML text, command tokens, template depth/value/port counts, workflow nodes, per-node fields,
    total dependency edges and total parameters now have explicit rejection bounds. JSON templates use an iterative stack,
    YAML lines and dependency lists are scanned incrementally, graph serialization budgets dependency output before joining,
    and workflow inference builds its incoming-dependency set without a `flatMap` peak allocation;
  - arbitrary imported JSON, cURL header and YAML `with` keys are installed as own data properties, preserving legitimate
    `__proto__` and `constructor` fields without invoking prototype setters. Exact cURL token validation rejects `curlfoo`,
    while the tokenizer retains the original Unicode-whitespace behavior;
  - graph update/add/delete operations validate caller-provided collections before copying, clone dependency arrays and
    parameter records for every returned node, preserve source-graph immutability and rewire renamed dependencies. Local
    per-inference normalization caches avoid repeated schema walks without adding process-global state or retained timers.
- Independent architecture, security, performance, test and scope reviews were sampled back to their cited symbols. The
  confirmed unbounded graph-edit inputs, shared `with` aliases, `curlfoo` acceptance, Unicode-whitespace drift, YAML line
  array, inference `flatMap` and serializer dependency-allocation findings were fixed and covered. Explicit residual
  boundaries remain: a valid near-1 MiB JSON template temporarily retains its source, parsed tree, transformed tree and
  output string, and a single YAML scalar can approach the 4 MiB document cap before rejection. Both peaks are bounded;
  eliminating them would require streaming JSON/YAML semantics outside this behavior-preserving split.
- Fresh local evidence after all Batch 32 changes:
  - the two Workflow Studio suites passed all 11 public-facade, prototype-key, command, graph immutability, graph limit,
    YAML round-trip and MCP schema tests; the complete desktop suite passed all 175 tests;
  - `npm run typecheck` passed and a fresh `npm run build` completed the Rsbuild production bundle;
  - the effective-line checker passed all 15 tests. The fresh ratchet scanned 754 files with 0 above 1,500, 4 from
    701-1,500 and 13 from 501-700, with zero violations. Every Batch 32 production and test file is below 500 effective
    lines;
  - scoped `git diff --check` and strict UTF-8/no-BOM/final-newline/trailing-whitespace checks passed for all 16 Batch 32
    files.
- No release was built for this individual batch; Phase 79 remains in progress. Both mandatory splits are closed and the
  701-1,500 queue falls from 6 to 4 files: `crates/loom_process/src/lib.rs` (923),
  `scripts/Invoke-LoomDaemonConcurrencySmoke.ps1` (867), `crates/loom_plugin_security/src/lib.rs` (858) and
  `scripts/tests/Test-LoomSampleArtInstallExecution.ps1` (707) effective lines.

## 2026-08-24 Batch 33 - Bounded process-supervision crate

- Split `crates/loom_process/src/lib.rs` from 923 effective / 1,024 physical lines into a 14 / 18 public facade plus eight
  responsibility modules: sanitized command/environment construction 74 / 92, stable errors 34 / 37, platform isolation
  157 / 186, long-lived managed children 66 / 76, public models 71 / 90, executable path handling 60 / 68, bounded
  one-shot execution 209 / 229 and package tests 329 / 354. The facade re-exports the exact original public structs,
  fields, methods, functions and error variants/strings; downstream callers require no import changes. Module dependencies
  remain one-way and no Cargo dependency or feature was added.
- Preserved environment allowlisting and its single image-search loopback seam, separate argument passing, Windows deep
  path adaptation, package-root executable containment, Windows job-object limits/accounting, Unix process groups, bounded
  stdout/stderr capture, timeout/cancellation/resource-limit diagnostics, cancellable and non-cancellable entry points,
  managed stdio ownership and the original ten platform-aware tests.
- Post-split security, lifetime and performance hardening:
  - when the leader exits normally, the runner now terminates its isolation group before joining pipe threads. This closes
    stdout/stderr/stdin handles inherited by a detached descendant instead of letting an otherwise successful execution
    wait forever after its leader has exited;
  - every missing managed-child pipe path now kills and waits for the child before returning `MissingPipe`, and
    `ManagedChild::terminate` conservatively cleans up after a `try_wait` error instead of treating it as a completed child;
  - deadline construction uses `Instant::checked_add`; an unrepresentable `Duration::MAX` becomes an intentional unbounded
    deadline rather than panicking before supervision begins. Environment-mutating unit tests share a poison-tolerant lock,
    removing their process-global variable race inside the test binary.
- Independent architecture, security, lifecycle, portability, test and scope reviews were sampled back to their cited
  symbols. The confirmed inherited-pipe deadlock was fixed with a background-descendant regression; the ten original test
  names and assertions remain intact, with two new overflow/lifecycle regressions. Explicit residual boundaries remain:
  - `memory_bytes`, `max_processes` and peak tree accounting use Windows job objects. Unix process groups provide process
    tree termination but no equivalent safe tree-wide memory/process-count boundary; the public fields now document that
    platform limitation instead of implying otherwise;
  - `ProcessSpec.current_dir` and `ProcessSpec.env` remain trusted caller configuration. The lower-level process crate does
    not impose package-root containment on an arbitrary working directory and deliberately applies explicit caller
    environment entries after the host allowlist; containment stays with the package/runtime callers that construct specs.
- Fresh local evidence after all Batch 33 changes:
  - `cargo fmt --package loom_process -- --check` passed. `cargo test --locked -p loom_process` passed all 12 tests,
    including the detached-descendant pipe regression and the extreme-timeout regression;
  - `cargo check --locked` passed for `loom_sandbox`, `loom_tool_registry`, `loom_mcp` and `loom-plugin-cli`; the independent
    `framework-packages/runtime-host` workspace also passed its locked Cargo check;
  - the effective-line checker passed all 15 tests. The fresh ratchet scanned 762 files with 0 above 1,500, 3 from
    701-1,500 and 13 from 501-700, with zero violations. Every `loom_process` source/test file is below 500 effective lines;
  - scoped `git diff --check` and strict UTF-8/no-BOM/final-newline/trailing-whitespace checks passed for all nine
    `loom_process` files.
- No release was built for this individual batch; Phase 79 remains in progress. The `loom_process` mandatory split is
  closed and the 701-1,500 queue falls from 4 to 3 files: `scripts/Invoke-LoomDaemonConcurrencySmoke.ps1` (867),
  `crates/loom_plugin_security/src/lib.rs` (858) and `scripts/tests/Test-LoomSampleArtInstallExecution.ps1` (707)
  effective lines.

## 2026-08-24 Batch 34 - Daemon concurrency smoke boundaries

- Split `scripts/Invoke-LoomDaemonConcurrencySmoke.ps1` from 867 effective / 945 physical lines into a 447 / 481
  orchestration entry plus four responsibility modules under `scripts/daemon-concurrency-smoke`: assertions, bounded
  evidence IO and redaction 106 / 122; exact process identity and isolated startup 137 / 148; bounded HTTP/status helpers
  96 / 111; and the loopback Gateway fixture 252 / 262. The entry retains its parameter surface, `LoomSmokePorts.ps1`
  dependency, request ordering, named-event synchronization, JSON evidence shape and cleanup/failure contract. The modules
  are dot-sourced in dependency order and remain Windows PowerShell 5.1 compatible.
- Added a 182 / 200 focused helper regression suite and extended both the smoke contract and standalone-release contract.
  The release contract requires the exact four-module set and ownership comments, so a source build cannot silently omit a
  newly extracted runtime dependency.
- Post-split security, lifetime and performance hardening:
  - the Gateway fixture now bounds accept at 30 seconds, socket reads/writes at 5 seconds, headers at 64 KiB, bodies at
    1 MiB and the total buffered request accordingly. It requires one decimal `Content-Length`, rejects duplicate length or
    content-type headers, requires the `application/json` media type and decodes the exact body bytes with strict UTF-8;
  - process cleanup binds PID to both canonical executable path and creation time, rechecks that identity immediately before
    termination and refuses an ambiguous/reused PID. A named machine-local mutex serializes the brief parent-environment
    mutation needed by Windows `Start-Process`, with unconditional restoration and mutex disposal;
  - stdout/stderr evidence reads are capped at 4 MiB with retained head/tail context. Authorization, Loom tokens and common
    key/value or query-string forms for tokens, API keys, passwords, secrets and cookies are redacted before evidence is
    written. Gateway request capture stores only an authentication boolean and already-redacted message content.
- Independent architecture, security and Windows PowerShell 5.1 reviews were sampled back to their cited functions. The
  confirmed permissive UTF-8 decoder, optional body length, missing JSON media-type check and generic secret-redaction gaps
  were fixed and covered. Explicit residual boundaries remain:
  - an operating-system failure while reading process creation time makes cleanup fail conservatively and leaves the smoke
    red instead of risking termination of an unrelated reused PID;
  - head/tail evidence truncation is byte-bounded, so a multi-byte character cut exactly at a retained boundary may render as
    a replacement character. The byte and memory bounds remain correct, and changing the evidence boundary would not change
    daemon behavior;
  - the full packaged-daemon concurrency scenario is intentionally deferred to the final Phase 79 package gate. This batch
    exercised the extracted fixture, cleanup identity and evidence helpers directly and did not publish an intermediate
    release.
- Fresh local evidence after all Batch 34 changes:
  - `Test-LoomDaemonConcurrencySmokeContract.ps1` passed, including live stalled-read, oversized-body, missing-length,
    invalid-UTF-8, Unicode byte-length, PID-reuse refusal, bounded-log and generalized-redaction helper probes;
  - `Test-StandaloneReleaseContract.ps1` and `Test-SmokeReleaseModules.ps1` passed. The effective-line checker passed all 15
    tests; the fresh ratchet scanned 767 files with 0 above 1,500, 2 from 701-1,500 and 13 from 501-700, with zero
    violations. Every Batch 34 source/test file is below 500 effective lines;
  - scoped `git diff --check` and strict ASCII/no-BOM/final-newline/trailing-whitespace checks passed for all eight Batch 34
    files.
- No release was built for this individual batch; Phase 79 remains in progress. The daemon-concurrency smoke mandatory
  split is closed and the 701-1,500 queue falls from 3 to 2 files: `crates/loom_plugin_security/src/lib.rs` (858) and
  `scripts/tests/Test-LoomSampleArtInstallExecution.ps1` (707) effective lines.

## 2026-08-24 Batch 35 - Plugin signing, trust and private-storage boundaries

- Split `crates/loom_plugin_security/src/lib.rs` from 858 effective / 921 physical lines into a 27 / 32 public facade and
  nine responsibility modules: bounded IO and atomic replacement 188 / 201, deterministic package digest 164 / 175,
  stable public errors 39 / 42, signing-key/trust-policy models 73 / 81, platform ACL/mode repair 188 / 199, key and package
  signing 96 / 109, trust-store persistence/mutation 119 / 136, signature verification 75 / 81 and focused tests 304 /
  324. The facade re-exports the exact original public types, functions and error variants/messages; downstream imports and
  serialized camelCase/snake_case fields remain unchanged.
- Preserved the canonical digest protocol exactly: sorted slash-normalized relative path bytes, NUL, little-endian `u64`
  byte length, NUL and file bytes. The original three unit-test names and assertions remain, and the existing Unix/Windows/
  fallback permission implementations retain their cfg boundaries.
- Post-split security, lifetime and performance hardening:
  - trust stores are limited to 4 MiB, signing-key documents to 64 KiB and package signature documents to 1 MiB before JSON
    parsing. Their declared schema versions are validated; loaded trust stores reject duplicate `(publisher_id, key_id)`
    records so record order cannot hide a revoked duplicate;
  - a signed manifest publisher must bind the same key ID as both the manifest signature metadata and signature document.
    Unknown publishers remain cryptographically `Verified`, revoked keys remain `Revoked`, and a trusted record with a
    different public key fails verification instead of changing classification;
  - signing keys now use the same synced, same-directory atomic replacement and private 0700/0600 or protected-DACL path as
    trust stores. Package signature JSON is also atomically replaced. Pre-existing signature/ancestor symlinks and parent
    traversal are rejected, with a second path check immediately after hashing and before the write;
  - package files are hashed through a fixed 64 KiB buffer instead of allocating up to the full 512 MiB package limit.
    Each opened file must retain its enumerated length, premature EOF/growth is rejected, symlinks are rechecked before open,
    and private-tree repair iterates `ReadDir` directly instead of collecting an unbounded per-directory vector.
- Independent architecture, security, platform and test reviews were sampled back to their cited functions. The confirmed
  publisher key-binding gap, unbounded document reads, plain private-key/signature writes, duplicate trust ambiguity,
  whole-file digest allocation and repair traversal allocation were fixed and covered. A downstream signed-framework test
  fixture that declared only `signature.keyId` was corrected to include the publisher key ID produced by every production
  signing path. Explicit residual boundaries remain:
  - a package directory writable by an attacker during signing or verification still has a handle-level TOCTOU window
    between symlink inspection and file open/rename. Full closure requires immutable staging plus Unix `openat`/no-follow
    and Windows directory-handle/reparse-point traversal coordinated with execution; path rechecks and length checks reduce
    the window but do not pretend to provide that architecture;
  - trust-store and private-key parent paths remain trusted control-plane configuration. A hostile concurrent replacement of
    one of those parents with a symlink/junction is outside this crate's current pathname-based ACL API;
  - Unix atomic replacement syncs the file and parent directory. Windows uses `MoveFileExW` with replace-existing and
    write-through, but Windows exposes no equivalent parent-directory fsync through the current implementation.
- Fresh local evidence after all Batch 35 changes:
  - `cargo fmt --package loom_plugin_security -- --check` and locked `cargo check --all-targets` passed. The crate passed all
    11 Windows unit tests, including key binding, schema/duplicate rejection, bounded/private key IO, bounded signature IO,
    streamed digest/exclusion, trust status and policy-matrix regressions. The Unix-only pre-existing signature-symlink
    regression remains cfg-gated for Unix CI;
  - locked downstream `cargo check --all-targets` passed for `loom_mcp`, `loom_tool_registry`, `loom_durable`,
    `loom_workflow_store`, `loom-plugin-cli`, `loom-art-store` and `loom-daemon`. Downstream tests passed 18 plugin-CLI tests,
    65 MCP tests and, after correcting the invalid signed fixture, a fresh 224/224 tool-registry tests;
  - the effective-line checker passed all 15 tests. The fresh ratchet scanned 776 files with 0 above 1,500, 1 from
    701-1,500 and 13 from 501-700, with zero violations. Every Batch 35 production/test file is below 500 effective lines.
- No release was built for this individual batch; Phase 79 remains in progress. The plugin-security mandatory split is
  closed and the 701-1,500 queue falls from 2 to the final file:
  `scripts/tests/Test-LoomSampleArtInstallExecution.ps1` (707 effective lines).

## 2026-08-24 Batch 36 - Installed sample Art orchestration and fixture boundaries

- Split the final mandatory file, `scripts/tests/Test-LoomSampleArtInstallExecution.ps1`, from 707 effective / 755 physical
  lines into a 442 / 460 orchestration entry and four responsibility modules: shared package/process/evidence/cleanup
  primitives 282 / 312, bounded authenticated JSON transport and Surface polling 119 / 129, offline MCP/Art package fixture
  transforms 100 / 114 and the bounded image-search API fixture 96 / 99. A 115 / 131 source-and-helper contract protects
  the exact module order and security-critical behavior. Every Batch 36 source/test file is below 500 effective lines.
- Preserved the complete real-smoke order and assertions: start the image and stock fixtures before the daemon; install and
  verify four frameworks; install both independent MCP packages and bind only the image-search credential; install/list all
  seven Arts; validate image-search metadata and all 19 Color Transfer parameters; execute the six image Arts; attach,
  refresh and verify the Stock Monitor JavaScript Surface; verify upstream fixture requests; uninstall image-search before
  stock so each unused MCP dependency is removed at the correct boundary; optionally exercise the large-image diagnostic
  limit; then restore the inherited environment and clean up. The final success contract remains exactly seven Arts and two
  MCP packages.
- Post-split security, lifetime and performance hardening:
  - package ZIP reads are capped at 128 MiB, stream into an exact byte array and reject premature EOF or growth. JSON request
    bodies are capped at 192 MiB; decompressed daemon responses are stream-bounded to 256 MiB, error bodies to 1 MiB, and
    response JSON is decoded with strict UTF-8 before parsing;
  - PowerShell fixture arguments are serialized through a UTF-8-no-BOM JSON payload and hashtable splatting instead of
    manually concatenated command text. The small launcher command uses the Windows backslash/quote algorithm, the trusted
    Windows PowerShell executable under `$PSHOME`, direct `ProcessStartInfo`, and file-backed stdout/stderr, eliminating the
    former undrained redirected-pipe deadlock. The live helper contract covers an empty string, spaces plus quotes, a trailing
    backslash and paths containing spaces;
  - the generated image-search HTTP fixture caps headers at 64 KiB and applies five-second read/write timeouts. Its
    credential-bearing endpoint and the stock endpoint both require unauthenticated loopback HTTP and reject query/fragment
    injection;
  - diagnostics read at most 1 MiB and redact bearer credentials, common token/key/password/secret/cookie forms and the known
    fixture key. Cleanup snapshots descendants before terminating the fixture root, validates the exact GUID-named temp-root
    containment, refuses every observed reparse point, clears package read-only attributes and deletes entries bottom-up
    without a final recursive traversal.
- Independent behavior, security and Windows PowerShell 5.1 reviews were sampled back to their cited functions. The confirmed
  command quoting, redirected-pipe, unbounded package/response, unbounded fixture header, raw diagnostic and recursive cleanup
  gaps were fixed and covered. Explicit residual boundaries remain:
  - the daemon executable, artifact roots and optional large-image path are intentionally operator-trusted absolute or
    repository-relative inputs. Constraining them to the source tree would prevent this smoke from validating an external
    release payload, so this script is not an untrusted-user execution boundary;
  - pathname inspection cannot fully close a hostile concurrent reparse swap between an attribute check and enumeration.
    Bottom-up non-recursive deletion prevents the final delete from following a newly swapped directory; complete closure
    would require Windows directory handles opened with reparse-point controls;
  - process descendants are a Windows CIM snapshot. A child that races to create a detached descendant after that snapshot
    can escape cleanup; a Job Object would be required for a kernel-enforced lifetime boundary. The fresh smoke left no new
    temp directory, while one pre-existing directory dated 2026-08-07 was deliberately preserved as unrelated state.
- Fresh local evidence after all Batch 36 changes:
  - `Test-LoomSampleArtInstallExecutionContract.ps1`, `Test-LoomSampleArtPackageContract.ps1` and
    `Test-GitHubActionsContract.ps1` passed. CI now runs the new source/helper contract in its focused Windows PowerShell
    contract step;
  - the real install/execution smoke passed all six image Arts, the Stock Monitor Surface, seven Arts total and two MCP
    packages using `target/debug/loom-daemon.exe` plus the authored framework/MCP/Art artifact roots. Its final run completed
    without cleanup warnings;
  - the effective-line checker passed all 15 tests. The fresh ratchet scanned 781 files with 0 above 1,500, 0 from
    701-1,500 and 13 from 501-700, with zero violations. All five changed/new 501-700 entries retain valid exact exceptions
    and the other eight are unchanged baseline files;
  - scoped PowerShell ASCII/UTF-8-no-BOM/LF/final-newline/trailing-whitespace checks passed, and repository-wide
    `git diff --check` reported no whitespace errors.
- No release was built for this individual batch. The final mandatory split is closed: the Phase 79 hard-limit and mandatory
  queues are both zero. Phase 79 now moves to its final repository/build/package/release closure gate.

## 2026-08-24 Loom final pre-release closure

- The complete Loom source gate was rerun after Batch 36. `cargo fmt --all -- --check`, locked workspace all-target checking,
  and `cargo test --workspace --locked` passed. The fresh workspace result is 825 tests across 59 suites. The daemon package
  passed 281 tests; the desktop Tauri crate passed 49 tests; and the detached framework runtime host passed 36 tests.
- One full-workspace rerun initially exposed two signed-framework fixture failures. The signed test manifest carried
  `signature.keyId` but omitted the matching `publisher.keyId`, so strict signature verification correctly returned
  `SignatureMetadataMismatch`. Only the signed fixture in `apps/daemon/src/tests/suite/part_04.rs` was corrected; the unsigned
  fixture remains unsigned. Both focused callers, the full daemon package, and the fresh workspace gate then passed. A single
  Hook revision test failure did not reproduce in focused serial runs or either full rerun, so no product or timing assertion
  was weakened.
- Desktop gates passed from a fresh `npm ci`: 175 Vitest tests, TypeScript typecheck, and the Vite production build. The
  desktop Tauri manifest and detached runtime-host manifest also passed their own format, locked check, and test gates.
- Stock Monitor contract closure found a test-host timing boundary rather than a runtime payload regression. The eighth
  sequential request was a 424-byte `stock_interval_commit`; Windows PowerShell was still alive, had not emitted stderr and
  had not consumed the pipe within the former five-second stdin-write allowance. The helper now retains a bounded 20-second
  cold-start/write allowance, reports only sequence/byte/process state, never interpolates request or stderr content, safely
  tolerates process-exit races, and observes stdin/stdout/stderr tasks before disposal. Fresh Stock Monitor Art and runtime
  hardening contracts passed, including the 4 MiB stdin, depth-32, 100,000-node and secret-redaction boundaries.
- The MCP package contract initially read a stale ignored `stock-api.zip` created before the seven runtime modules were split.
  A fresh isolated build under `.tmp/mcp-servers-check` recursively included those source modules; exact ZIP/hash/runtime
  validation then passed for both independent MCP packages. The stock API Node suite passed all eight bounded-body,
  cancellation and stdio-backpressure tests. No stale generated ZIP is part of the source commit.
- The malicious-plugin gate contained a post-split false-green: Cargo returns zero for a stale filter that matches no tests.
  Archive and network tests had moved into `loom_security`, while framework/install filters needed their new lifecycle,
  recovery, activation, install-core or policy module segments. The script now performs a PowerShell-5.1-compatible
  `cargo test -- --list` discovery and rejects zero matches before execution. Fresh serial cases executed real tests and
  passed for archive, signature, dependency, network, process and lifecycle attack surfaces; the archive rerun specifically
  executed one secure-archive test and one unsafe-framework-ZIP test instead of the prior zero-test result.
- Fresh package/release contracts passed for Sample Arts, installed Sample Art execution source, independent MCP servers,
  Art frameworks, Art plugin boundaries, standalone layout, standalone release composition, smoke modules, release path
  safety, integrity tampering, and GitHub Actions ordering. The real Batch 36 install/execution smoke had already passed seven
  Arts, two MCP packages and all six image Arts without a new cleanup residue.
- The effective-line checker still passes all 15 lexer/policy tests. Strict mode scans 781 files with 0 above 1,500, 0 from
  701-1,500 and 13 from 501-700. All 13 soft-limit files now have exact current hashes, responsibilities, specific cohesion
  reasons, independent approval, tests, and a `2026-09-30` review deadline in
  `scripts/effective-code-lines-exceptions.json`; unchanged baseline status is no longer used as the sole justification.
- The clean-source release ID reserved for the next step is `20260824-phase79-rc1` under
  `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom`. This entry does not claim that artifact exists or passes yet;
  the release remains pending until the complete Phase 79 source set is committed, `build-release.ps1 -RequireCleanSource`
  creates that unused directory, and `verify-release.ps1 -RunSmoke -RequireCleanSource` verifies the exact package.
- Commit `4b1c38f` supplied the first clean-source candidate. `build-release.ps1 -RequireCleanSource` successfully created
  `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260824-phase79-rc1` with 58 checksum entries, the desktop and
  daemon executables, CLI, Plugin SDK, four frameworks, two MCP servers, seven Sample Arts, two SBOM formats and provenance.
  Its exact-package verifier then correctly stopped before completion: the split `smoke-release/Focused.ps1` resolved all
  three top-level focused scripts relative to its own module directory instead of the repository `scripts` directory.
- The focused-smoke resolver now uses the explicit repository scripts root. `Test-SmokeReleaseModules.ps1` mocks process
  execution and dynamically verifies the exact `-File` path and 300-second budget for Gateway planning, run persistence and
  daemon concurrency; the focused module and standalone release contracts pass. RC1 is preserved as a failed-verifier
  diagnostic candidate and is not the final release. The next unused clean-source target is `20260824-phase79-rc2`; it must
  be rebuilt from the resolver-fix commit and pass the complete exact-package smoke verifier before Loom closure is claimed.
