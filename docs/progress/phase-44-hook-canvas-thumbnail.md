# Phase 44: Hook Canvas Thumbnail and Visual Workflow

## Goal

Make Screenshot Sync understandable for UI-first users by showing the real
Hook node canvas, including relative node placement, image previews, and links,
instead of making YAML or protocol text the primary representation.

## Delivered

- Added the daemon-owned `GET /v1/hook-bridge/canvas` snapshot model.
- Added the node-ID-only preview endpoint
  `GET /v1/hook-bridge/canvas/nodes/{nodeId}/preview`.
- Normalized flat Hook sessions and nested Hook Bridge payload shapes into a
  deterministic canvas model with bounds, classification, warnings, and
  revision tracking.
- Enforced canonical image-root checks, supported-image magic validation,
  response size limits, and bearer authentication on preview reads.
- Added fit-to-bounds desktop layout math and retained-last-good-snapshot
  behavior for offline or temporarily unreadable sessions.
- Added a real Screenshot Sync thumbnail with node images, missing-preview
  degradation, edge lines, connection state, and an entry into the full visual
  workflow canvas.
- Added full-canvas selection and a read-only node inspector. Selection is
  retained when a Hook update refreshes the canvas and is cleared if its node
  disappears.
- Changed Hook Bridge refresh handling to debounce updates. Workflow and Art
  updates refresh the canvas without forced navigation; instantiate events also
  open the Hook visual workflow.
- Moved YAML, cURL, raw JSON, protocol, session-path, IPC, and shared-memory
  compatibility surfaces into collapsed `高级技术信息` disclosures.
- Added isolated CDP/WebView2 smoke tooling, exact executable-path cleanup,
  release verifier integration, and Windows CI contract coverage.
- Added an isolated Hook Bridge URL configuration so packaged smoke can use a
  dynamic bridge port without touching a user's existing port 19820 instance.
- Fixed the desktop startup race where the first canvas read could run before
  the daemon became ready. Automatic reads now wait for an online daemon and
  retry when the online base URL or Hook canvas invalidation revision changes;
  manual refresh remains available while the last valid snapshot is retained.

## User Path

1. Open **截图同步**.
2. Inspect the live Hook node arrangement and previews.
3. Select **打开可视化工作流** or click a node to enter the full canvas.
4. Expand **高级技术信息** only when YAML or compatibility diagnostics are
   needed.

## Verification

The source-level gates for this phase are:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\Test-HookCanvasUiContract.ps1
npm --prefix apps/desktop test
npm --prefix apps/desktop run typecheck
npm --prefix apps/desktop run build
cargo test --locked -p loom-daemon hook_canvas -- --nocapture
cargo test --locked --manifest-path apps\desktop\src-tauri\Cargo.toml --lib
```

The packaged UI smoke writes API JSON, CDP JSON, screenshots, process
snapshots, and a summary below the supplied evidence root. It uses isolated
`APPDATA`, `LOCALAPPDATA`, control-plane, configuration, WebView2, daemon, and
Hook Bridge ports, and only stops candidate PIDs after validating their exact
executable paths.

The final candidate for this phase was built from source commit
`e8eb505ec41164ef5ce2a677dc88505ffea3f1ec`:

```text
C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260722-hook-canvas-e8eb505
```

Its manifest reports `gitDirty=false`, one root `Loom.exe`,
`runtime\loom-daemon.exe`, and 32 checksum-covered files. The release
verifier completed with `smoke=passed` and `hookCanvasSmoke=passed`.

Representative evidence:

- Direct packaged Hook canvas smoke:
  `target/runtime-smoke/hook-canvas-5edb52b3f463463884ad7a5e3d4013ea`
- Formal verifier Hook canvas smoke:
  `target/runtime-smoke/hook-canvas/hook-canvas-094d74c3505943f5aac22b2fb3e89b4d`

The direct smoke observed three initial nodes and one edge, then four nodes
after an `art_hook/instantiate` update. YAML was not visible by default, the
advanced disclosure remained closed, the full visual canvas opened, and
`unexpectedProcessesAfterCleanup=0`.

## Boundaries

The Hook repository and the user's real Hook session remain read-only. YAML is
still supported for persistence, import/export, and diagnostics; it is no
longer the default user-facing workflow surface.
