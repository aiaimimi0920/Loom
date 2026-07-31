# Phase 54: Hook error-message sync

## Status

Complete.

## Why this phase exists

Phase 53 synchronized the failed Art-node status itself so Loom finally stopped
showing a stale image preview when Hook had already entered the failed state.

One more mismatch remained:

- Hook showed both `执行失败` **and** the concrete backend error reason
- Loom only showed the generic failure title, without the detailed cause

That left the browser view technically correct about failure, but still less
useful than Hook during real debugging.

## Implemented

- Extended daemon Hook canvas nodes to carry optional `errorMessage`.
- Extended daemon runtime overlays so failed nodes now retain both:
  - `status = error`
  - the concrete runtime error text
- Applied that overlay to both:
  - `art_loom/execute_art_node`
  - best-effort matched `art/process`
- Included the runtime error message in the Hook canvas revision overlay so the
  desktop cannot keep a stale generic-error snapshot after the detailed message
  changes.
- Updated the desktop Hook canvas presentation helper to emit:
  - title: `执行失败`
  - detail: the synced failure reason
- Updated `HookCanvasNode.tsx` and styles so the browser view renders the same
  failure-reason text directly inside the failed Art node.
- Updated the Hook canvas inspector so the selected failed node also exposes the
  synced failure reason in the side panel.
- Extended the packaged Hook canvas UI smoke and contract to assert the failed
  Art node reason text survives in the Loom browser view.

## Verification

Commands run:

```powershell
npm run typecheck --prefix .\apps\desktop
npm test --prefix .\apps\desktop -- hookCanvas.test.ts
cargo test -p loom-daemon hook_canvas -- --nocapture
cargo test -p loom-daemon ahrp_process -- --nocapture
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\tests\Test-HookCanvasUiContract.ps1
```

Results:

- desktop typecheck: passed
- desktop Hook canvas service tests: passed
- daemon Hook canvas tests: passed (`33` tests)
- daemon AHRP tests: passed (`7` tests)
- Hook canvas UI contract: passed

## Release

Final parent-scoped release for this phase:

```text
C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260729-hook-error-message-sync
```
