# Phase 53: Hook live-runtime failure sync

## Status

Complete.

## Why this phase exists

Phase 52 fixed the desktop/browser presentation rule for failed Art nodes once
Loom already received `status = error`.

The remaining real-world drift was deeper:

- Hook's live UI marks failed Art nodes from runtime delivery events
- the live Hook workflow sync (`art_loom/overwrite_workflow`) does not persist
  that runtime failure state into the synced node payload
- Loom's browser view therefore kept rendering the Art node like a normal image
  preview node whenever the underlying preview still existed

In practice, a failed Art node could still show the incoming image preview in
Loom even though Hook already showed `执行失败`.

## Implemented

- Refactored `apps/daemon/src/hook_canvas.rs` so a Hook canvas document can be
  built from an in-memory serialized workflow snapshot, not only from
  `session.json`.
- Added a daemon-side live Hook workflow cache keyed by the Hook live workflow
  id (`hook-live` / `arthook-live`).
- Taught the Hook bridge path to capture `art_loom/overwrite_workflow`
  snapshots and use that live graph as the preferred browser-view source.
- Added daemon-owned runtime node-status overlays for Hook Art nodes:
  - `art_loom/execute_art_node` sets `processing` and then `ready` / `error`
  - `art/process` performs a best-effort node match against the live Hook graph
    and overlays the same runtime statuses
- Folded runtime overlay state into the Hook canvas revision so the desktop UI
  cannot silently keep an older `ready` snapshot after a failure arrives.
- Reused the active Hook canvas loader for:
  - `/v1/hook-bridge/canvas`
  - `/v1/hook-bridge/canvas/nodes/{nodeId}/preview`
  - Hook-canvas workflow export

## Verification

Commands run:

```powershell
cargo test -p loom-daemon hook_canvas -- --nocapture
cargo test -p loom-daemon ahrp_process -- --nocapture
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\build-release.ps1 `
  -VersionId 20260729-hook-live-runtime-failure-sync `
  -OutputRoot C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\verify-release.ps1 `
  -PackageDir C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260729-hook-live-runtime-failure-sync `
  -RunSmoke
```

Results:

- daemon Hook canvas test slice: passed (`33` tests)
- daemon AHRP process slice: passed (`7` tests)
- release build: passed
- release verification: passed with
  - `smoke=passed`
  - `hookCanvasSmoke=passed`
  - `hookErrorPreviewSmoke=passed`
  - `frameworkArtStoreHookSmoke=passed`

## Release

Final parent-scoped release for this phase:

```text
C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260729-hook-live-runtime-failure-sync
```
