# Phase 36: Desktop Startup and Hook Live Workflow Visibility

## Goal

Close the user-visible gaps reported after Phase 35:

1. `loom-desktop.exe` should be the normal Loom UI entry, not a UI that requires
   users to understand and manually start a daemon first.
2. "Hook Bridge" should not be the primary user-facing term. The desktop should
   present it as Hook screenshot sync while preserving old protocol names for
   compatibility diagnostics.
3. Hook-generated live workflows must be visible and openable from the workflow
   workbench.

## Root cause

- `loom-daemon.exe` is the local loopback runtime service. Before this phase the
  desktop only read `http://127.0.0.1:8765`; it did not start the sibling daemon.
- Hook live workflows are saved through the compatibility aliases
  `hook-live` / `arthook-live`, which map to `latest.yaml`.
- `WorkflowStore::list_workflows()` skipped `latest.yaml`, so
  `GET /v1/workflows` never returned the Hook live workflow. The desktop
  workflow lists are sourced from `GET /v1/workflows`, so the user could not see
  or open the generated Hook workflow.
- `art_loom/instantiate_workflow` broadcasted `art_hook/instantiate` but did
  not persist the incoming graph. If Hook only sent the instantiate message,
  there was no `latest.yaml` for the desktop to open.

## Changes

- Added desktop Tauri command `start_loom_daemon`.
  - It checks `/health` first.
  - If offline, it starts sibling `loom-daemon.exe` from the same release
    directory as `loom-desktop.exe`.
  - The desktop overview now exposes `启动 Loom 本地服务` and auto-attempts one
    background startup after an offline snapshot.
- Renamed the user-facing Hook nav/page to `截图同步` / `Hook 截图同步`.
  - Protocol method names such as `art_hook/instantiate` and
    `art_loom/update_workflow_node` remain visible only as compatibility
    diagnostics.
- Added Hook live workflow visibility.
  - `latest.yaml` is now listed as canonical workflow id `hook-live`.
  - `art_loom/instantiate_workflow` now writes the incoming graph to
    `hook-live` / `latest.yaml` while preserving the legacy broadcast.
  - The desktop labels it as `Hook 实时工作流`.
  - Hook sync page exposes `打开 Hook 工作流`.
  - Workflow Manager exposes `在工作台打开` / `打开 Hook 工作流`.
  - Workflow Studio can load a requested workflow id and shows a clear Chinese
    empty-state if `hook-live` does not exist yet.
- Updated startup docs in `Loom/README.md` and `docs/loom/README.md`.

## Validation

RED tests were added first and observed failing:

```powershell
cargo test --manifest-path Loom\Cargo.toml -p loom_workflow_store list_workflows_includes_hook_live_alias_when_latest_yaml_exists -- --nocapture
cargo test --manifest-path Loom\apps\desktop\src-tauri\Cargo.toml daemon_sidecar_path_uses_sibling_loom_daemon_exe -- --nocapture
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1
```

GREEN/regression validation:

```powershell
cargo test --manifest-path Loom\Cargo.toml -p loom_workflow_store
cargo test --manifest-path Loom\Cargo.toml -p loom_hook_bridge
cargo test --manifest-path Loom\apps\desktop\src-tauri\Cargo.toml
npm run typecheck --prefix Loom\apps\desktop
npm run build --prefix Loom\apps\desktop
cargo test --manifest-path Loom\Cargo.toml -p loom-daemon -- --test-threads=1
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1
```

Results:

- `loom_workflow_store`: 4 tests passed.
- `loom_hook_bridge`: 23 tests passed.
- `loom-desktop` Tauri backend: 5 tests passed.
- `loom-daemon`: 59 lib tests and 2 CLI contract tests passed with
  `--test-threads=1`.
- Desktop typecheck and Rsbuild build passed.
- Parity contract passed.
- Browser smoke against `http://127.0.0.1:1423` confirmed:
  - `启动 Loom 本地服务` is visible on Overview.
  - nav shows `截图同步`.
  - Hook sync page shows `打开 Hook 工作流`.
  - clicking it switches to `工作流工作台` and shows
    `还没有 Hook 实时工作流，请先从 Hook 生成或保存一次工作流。` when the local service
    is not running.

## Release

Generated release:

```text
release\Loom\loom-desktop-start-hook-workflow-phase36
```

Executables:

```text
loom.exe
loom-daemon.exe
loom-desktop.exe
```

Package:

```text
release\Loom\loom-desktop-start-hook-workflow-phase36\packages\Loom-loom-desktop-start-hook-workflow-phase36-windows-x64.zip
```

Package sha256:

```text
ae0bb87e01edf60c269bc0b4d61dc7707c4851040b8b64eea8eeb653e044c19a
```

Formal release verification:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-release.ps1 -VersionId loom-desktop-start-hook-workflow-phase36 -Apps Loom
```

Result:

```text
status: passed
gitHead: 0a5c37b9fba3513d7f058e16b498530ddfc27d3b
gitDirty: false
checksumEntries: 31
```

## User-facing startup answer

- Normal startup: run `loom-desktop.exe`.
- The "daemon" is Loom's local service behind the UI. It is packaged as
  `loom-daemon.exe` and should normally be started by the desktop automatically.
- Manual fallback/debug startup: run `loom-daemon.exe`, then run
  `loom-desktop.exe`.
- `loom.exe` is the CLI, not the desktop UI.
