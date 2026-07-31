# Phase 45: Installable Frameworks and Art Store Closure

## Status

Complete.

## Why this phase exists

Loom already had an in-progress implementation for installable execution
frameworks, framework-gated Art package installation, and a local art store,
but the work was not recorded as a dedicated task line in the project docs.
That made the feature easy to miss and also hid a real implementation drift:
the Python Art runtime-resolution helper and its new framework-runtime-first
tests had diverged.

This phase closes that gap by:

- recording the feature as a tracked Loom phase;
- fixing the remaining framework-runtime precedence bug in
  `loom_tool_registry`;
- adding desktop API regression coverage for framework and art-store routes;
- documenting the user/operator path in `README.md`; and
- generating a fresh parent-scoped Loom release.

## Implemented

- Reconciled `resolve_python_executable_from(...)` with the installable
  `python_art` framework contract so a framework-provisioned runtime under
  `LOOM_FRAMEWORK_RUNTIMES_DIR` is preferred before packaged or PATH Python
  fallbacks.
- Kept `resolve_python_executable()` and the test helper on the same precedence
  chain instead of maintaining two slightly different code paths.
- Added desktop `loomApi` regression tests covering:
  - `GET /v1/frameworks`;
  - `POST /v1/frameworks/{id}/install`;
  - `POST /v1/frameworks/{id}/uninstall`;
  - `GET /v1/arts/store/catalog`;
  - `POST /v1/arts/store/install`.
- Updated `README.md` to document:
  - the six framework ids;
  - the install/readiness flow;
  - `framework_not_ready`;
  - the local art store server and `LOOM_ART_STORE_URL`;
  - the runtime precedence for `python_art`.
- Added the dedicated task/plan file
  `docs/superpowers/plans/2026-07-29-loom-installable-frameworks-art-store-closure.md`.

## Verification

Commands run:

```powershell
cargo test -p loom_tool_registry framework -- --nocapture
cargo test -p loom_tool_registry -- --nocapture
npm test --prefix apps/desktop
cargo test -p loom-daemon -- --nocapture
npm run typecheck --prefix apps/desktop
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\build-release.ps1 `
  -VersionId 20260729-installable-frameworks-art-store `
  -OutputRoot C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\verify-release.ps1 `
  -PackageDir C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260729-installable-frameworks-art-store
```

Results:

- `cargo test -p loom_tool_registry framework -- --nocapture`: 11 passed, 0 failed.
- `cargo test -p loom_tool_registry -- --nocapture`: 41 passed, 0 failed.
- `npm test --prefix apps/desktop`: 47 passed, 0 failed.
- `cargo test -p loom-daemon -- --nocapture`: 146 daemon library tests and
  8 CLI contract tests passed.
- `npm run typecheck --prefix apps/desktop`: exited 0.
- Parent-scoped release build completed successfully.
- Release verification completed with:
  - `filesChecked = 32`
  - `smoke = not-run`
  - `hookCanvasSmoke = not-run`

## Release

Generated:

```text
C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260729-installable-frameworks-art-store
```

Release manifest summary:

```text
gitHead: fbd4a50ebc98d985912092116f6fbfa776587531
gitDirty: true
checksumEntries: 32
desktop zip sha256: 0b5cf18c8ff5c1f0fce6edbb8e650d6ebdf69d3fd2e8e5e5e19d8cf4a2a5980d
cli zip sha256: f5b8a5ad97ea35148bfe2ad3af18ac237225ec44ce67a0702fbfcc735a678922
```

## Boundaries

This phase closes the Loom-side framework/art-store feature line only. It does
not add framework installation UI beyond the existing Loom desktop surfaces,
does not bundle `loom-art-store` into the Windows desktop release payload, and
does not change the already-existing Hook canvas or release-boundary work.
