# Loom Hook canvas smoke regression fix plan

## Goal

Restore the stable Hook-canvas UI entrypoints that the packaged WebView2 smoke
depends on, then prove the full `verify-release.ps1 -RunSmoke` chain passes
again on a parent-scoped Loom release.

## Tasks

- [x] Add failing source-contract coverage for the missing stable visual
  workflow entry target and inspector selector.
- [x] Restore a dedicated Hook-canvas visual-workflow entry in the live
  thumbnail with a stable `data-testid`.
- [x] Wire the Hook sync page button back into the existing `hook-live`
  workflow-opening path so the full visual canvas is reachable again.
- [x] Restore the explicit advanced-technical-information disclosure smoke
  target in the desktop app.
- [x] Update the WebView inspector to click the dedicated visual-workflow
  target instead of the removed legacy toolbar selector.
- [x] Re-run the Hook canvas contract, desktop typecheck/tests/build, packaged
  Hook canvas smoke, and full `verify-release.ps1 -RunSmoke`.
- [x] Generate a new parent-scoped Loom release after the regression fix.
