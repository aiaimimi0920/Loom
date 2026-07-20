# Phase 33: Settings, Session, and MCP Package Compatibility

## Goal

Restore the remaining old ArtLoom settings/session/package surfaces that were still missing after Phase 32:

- General, Engine, Hotkey, Quick bindings, and System settings UI.
- `read_arthook_session` compatibility.
- `check_mcp_package_installed` and safe `install_mcp_package` compatibility.

## Implemented

### Daemon

- Added `POST /v1/mcp/package/check`.
  - Compatibility name: `check_mcp_package_installed`.
  - Performs a non-destructive Python import probe.
- Added `POST /v1/mcp/package/install-plan`.
  - Compatibility name: `install_mcp_package`.
  - Returns a pip command preview with `sideEffect=false`; it does not run arbitrary package installation.
- Added `GET /v1/hook-bridge/session`.
  - Compatibility name: `read_arthook_session`.
  - Reads the ArtHook session file when available and falls back to `{ stickers: [], links: [] }`.
- Added `/v1/artloom-compat/*` settings endpoints:
  - `GET /v1/artloom-compat/settings`
  - `PUT /v1/artloom-compat/settings`
  - `GET /v1/artloom-compat/shortcuts`
  - `PUT /v1/artloom-compat/shortcuts/{shortcutId}`
  - `GET /v1/artloom-compat/app-paths`
  - `POST /v1/artloom-compat/system/autostart`
  - `POST /v1/artloom-compat/system/minimize-to-tray`
- Fixed the existing `/v1/configuration/claims?app=...` routing bug by matching the stripped route path but reading the query from the original request path.
- Increased daemon test WebSocket timeouts from 2s to 10s because Windows PowerShell script-backed Art tests take about 4s on this machine.

### Hook Bridge crate

- Added `read_arthook_session` to the legacy method catalog.
- Added parser and handler support for `HookBridgeRequest::ReadArtHookSession`.
- Added tests for parsing, catalog inclusion, and handler response.

### Desktop UI

- Settings now exposes old ArtLoom-style cards:
  - `General settings`
  - `Engine settings`
  - `Hotkey settings`
  - `Quick bindings`
  - `System settings`
- MCP now exposes `MCP package compatibility` with:
  - `check_mcp_package_installed`
  - `install_mcp_package`
  - safe install command preview
- Hook Bridge now exposes an `ArtHook session` card with `read_arthook_session`.
- UI keeps the `Loom` product name and modern-gradient/glass visual baseline.

## Verification before release

Commands run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1
npm run typecheck --prefix Loom/apps/desktop
npm run build --prefix Loom/apps/desktop
```

```powershell
cd Loom
cargo fmt --check
cargo test -p loom_hook_bridge
cargo test -p loom_tool_registry
cargo test -p loom-daemon -- --test-threads=1
```

Observed results:

- Parity contract passed.
- Desktop TypeScript typecheck passed.
- Desktop Rsbuild build passed.
- Rust format check passed.
- `loom_hook_bridge`: 22 passed.
- `loom_tool_registry`: 18 passed.
- `loom-daemon`: 58 lib tests + 2 CLI contract tests passed with `--test-threads=1`.

Browser UI smoke:

- Built desktop UI opened at `http://127.0.0.1:1427/` through a temporary Node static server.
- Settings snapshot confirmed:
  - `General settings`
  - `Engine settings`
  - `Hotkey settings`
  - `Shortcut recorder`
  - `Quick bindings`
  - `System settings`
  - `Auto-start at login`
  - `Minimize to tray`
  - `Python Interpreter`
  - `ComfyUI API Endpoint`
- MCP snapshot confirmed:
  - `MCP package compatibility`
  - `check_mcp_package_installed`
  - `install_mcp_package`
  - `Install command preview`
- Hook snapshot confirmed:
  - `ArtHook session`
  - `Read ArtHook session`
  - existing desktop Hook sync controls.

## Release

Generated release:

```text
C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\loom-settings-session-compat-phase33
```

Executables:

```text
loom.exe
loom-daemon.exe
loom-desktop.exe
```

Zip package:

```text
C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\loom-settings-session-compat-phase33\packages\Loom-loom-settings-session-compat-phase33-windows-x64.zip
```

Zip sha256:

```text
3318749e0e6860c8bfb1111ae62440f5f438313c91fe5588efbc7505b5f2b59e
```

Release smoke:

```text
C:\Users\Public\nas_home\AI\GameEditor\Neuro\output\smoke\runs\20260613-061516-Loom-67600-ec55108b27b74935b4c8419e01873f46\release-local-apps-loom-settings-session-compat-phase33-Loom-summary.json
C:\Users\Public\nas_home\AI\GameEditor\Neuro\output\smoke\latest\release-local-apps-loom-settings-session-compat-phase33-Loom-summary.json
```

Release smoke evidence included:

- `hookBridgeMethods` contains `read_arthook_session`.
- `mcpMarketplace.packageCheckCommand = "check_mcp_package_installed"`.
- `mcpMarketplace.packageInstallCommand = "install_mcp_package"`.
- `mcpMarketplace.packageInstallSideEffect = false`.
- `artHookSession.method = "read_arthook_session"`.
- `artHookSession.protocol = "artloom-compat"`.
- On this host, the real ArtHook session file was present:
  - `artHookSession.available = true`
  - `artHookSession.stickerCount = 3`
  - `artHookSession.linkCount = 1`

Formal release verification:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-release.ps1 -VersionId loom-settings-session-compat-phase33 -Apps Loom
```

Observed result:

```text
status: passed
gitHead: 5c487f7ecef4023eb553c9656ebbce09ce786efa
gitDirty: false
checksumEntries: 31
```
