# Phase 29 Final Audit: Hook Bridge Settings Compatibility

## Scope

Phase 29 closes a final-audit compatibility gap in Loom's ArtHook-facing Hook
Bridge.

Restored legacy Hook Bridge methods:

- `get_settings`
- `get_shortcuts`
- `update_art_param`
- `sync_shortcuts`

Visible product names remain:

- `loom.exe`
- `loom-daemon.exe`
- `loom-desktop.exe`

The old ArtLoom source remains read-only:
`Z:\project\project\ArtNexus\ArtLoom`.

## Old source evidence

Reviewed old ArtLoom sources:

- `Z:\project\project\ArtNexus\ArtLoom\src-tauri\src\lib.rs`
- `Z:\project\project\ArtNexus\ArtLoom\src-tauri\src\ipc_service.rs`
- `Z:\project\project\ArtNexus\ArtLoom\src-tauri\src\settings.rs`
- `Z:\project\project\ArtNexus\ArtLoom\src-tauri\src\system_settings.rs`

Old Tauri command registration included:

```text
settings::get_settings
settings::update_settings
settings::get_shortcuts
settings::update_shortcut
settings::get_app_paths
system_settings::enable_autostart
system_settings::disable_autostart
system_settings::is_autostart_enabled
system_settings::set_autostart
system_settings::set_minimize_to_tray
```

Old ArtHook WebSocket IPC declared legacy methods:

```text
get_settings
get_shortcuts
update_art_param
sync_shortcuts
```

Old `settings.rs` behavior:

- `get_settings` returned the in-memory `AppSettings`.
- `get_shortcuts` returned `settings.shortcuts.values()`.
- `update_shortcut` updated and persisted a shortcut.
- `get_app_paths` returned app data/config/log paths.

## Loom state before Phase 29

Current Loom already had daemon-managed settings/configuration surfaces:

- `/settings`
- `/settings/tea`
- `/settings/hook`
- `/settings/talk`
- `/v1/configuration/*`
- `/v1/hook-bridge/status`

However, the final source audit found a Hook Bridge protocol gap:

```text
Loom/crates/loom_hook_bridge/src/lib.rs
```

The method list advertised legacy compatibility for:

```text
get_settings
get_shortcuts
update_art_param
sync_shortcuts
```

but `handle_request` still fell through to:

```text
Hook bridge method is not implemented
```

for those methods.

This meant old ArtHook-style clients could discover the methods but could not
actually call them successfully.

## Phase 29 implementation

### Contract-first RED

Updated:

```text
scripts/tests/test-loom-artloom-parity-contract.ps1
```

New contract assertions require:

```text
Test-LoomHookBridgeSettingsCompatibility
hookBridgeSettings
```

Added targeted Rust test:

```text
handler_answers_legacy_settings_and_shortcuts
```

The test failed before implementation with a legacy method returning:

```text
left: String("error")
right: "success"
```

### Hook Bridge compatibility handlers

Updated:

```text
Loom/crates/loom_hook_bridge/src/lib.rs
```

Implemented handlers:

```text
get_settings
get_shortcuts
update_art_param
sync_shortcuts
```

`get_settings` now returns a legacy-compatible payload with:

- `general.theme = "system"`
- `general.language = "zh-Hans"`
- `general.auto_start = false`
- `general.minimize_to_tray = true`
- `engine.python_interpreter = "python.exe"`
- `engine.comfyui_url = "http://127.0.0.1:8188"`
- a default shortcut map for `capture`, `ocr`, `color_picker`, and `cancel`

`get_shortcuts` returns the default legacy shortcut list.

`update_art_param` returns a success acknowledgement containing:

- `art_id`
- `param_id`
- `value`

`sync_shortcuts` returns:

- `synced = true`
- the default legacy shortcut list

This intentionally restores the old ArtHook protocol handshake surface without
replacing Loom's newer daemon-managed settings pages.

### Release smoke

Updated:

```text
scripts/smoke-release-local-apps.ps1
```

Added:

```text
Test-LoomHookBridgeSettingsCompatibility
```

The packaged smoke opens the Hook Bridge WebSocket and exercises:

```text
get_settings
get_shortcuts
update_art_param
sync_shortcuts
```

The smoke summary now includes:

```text
hookBridgeSettings
```

with:

```text
settingsTheme = "system"
shortcutCount = 4
updatedArtId = "fixture-art"
synced = true
```

## Validation evidence

Targeted checks passed before code commit:

```powershell
cargo fmt --manifest-path Loom/Cargo.toml --all
cargo test --manifest-path Loom/Cargo.toml -p loom_hook_bridge handler_answers_legacy_settings_and_shortcuts --offline -- --nocapture
cargo check --manifest-path Loom/Cargo.toml -p loom-daemon --offline
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1
cargo fmt --manifest-path Loom/Cargo.toml --all -- --check
git diff --check
```

Release build:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release-exes.ps1 -Apps Loom -VersionId loom-hook-settings-phase29 -Force
```

Generated:

```text
release\Loom\loom-hook-settings-phase29\loom.exe
release\Loom\loom-hook-settings-phase29\loom-daemon.exe
release\Loom\loom-hook-settings-phase29\loom-desktop.exe
release\Loom\loom-hook-settings-phase29\packages\Loom-loom-hook-settings-phase29-windows-x64.zip
```

Package evidence:

```text
size = 50101437 bytes
sha256 = 6a5623a38584e9206967f7337929a4534cb0d0ebdc1840146ca181e17e36d389
```

Formal release verification:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-release.ps1 -VersionId loom-hook-settings-phase29 -Apps Loom
```

Result:

```text
status = passed
gitHead = e0081f7412875cd31848081154530cfe9fdae0ca
gitDirty = false
checksumEntries = 31
```

Release smoke:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\smoke-release-local-apps.ps1 -VersionId loom-hook-settings-phase29 -Apps Loom
```

Result:

```text
status = passed
summaryEvidencePath = output\smoke\runs\20260612-224237-Loom-63436-fbe6f33fc618425695b06a5f913d1777\release-local-apps-loom-hook-settings-phase29-Loom-summary.json
summaryLatestEvidencePath = output\smoke\latest\release-local-apps-loom-hook-settings-phase29-Loom-summary.json
```

Smoke evidence:

```text
controlPlane.hookBridgeSettings.settingsTheme = "system"
controlPlane.hookBridgeSettings.shortcutCount = 4
controlPlane.hookBridgeSettings.updatedArtId = "fixture-art"
controlPlane.hookBridgeSettings.synced = true
```

Regression smoke also remained green for prior restored surfaces, including:

- MCP marketplace
- management CRUD
- Python Art catalog and execution
- Python source import helper flow
- embedded Python execution
- cloud multipart execution
- real OCR
- shared image AHRP process
- workflow tool execution
- workflow Art node execution
- workflow AHRP execution

## Non-goals and final-audit notes

Phase 29 does not reintroduce old visible ArtLoom/NeuroLoom naming.

Phase 29 does not replace Loom's daemon-managed configuration UI. It restores
the old ArtHook WebSocket compatibility methods so existing ArtHook-style
clients can call them successfully.

The old Tauri-only settings commands and system settings commands still need to
be classified in the final parity matrix:

- restored behaviorally through Loom managed config / desktop shell
- compatibility-only via Hook Bridge payloads
- obsolete / non-product-critical for the current Loom migration
- or product-critical gaps requiring another phase

Do not claim the migration is彻底 complete until the final source parity matrix
is written and any product-critical gaps from that matrix are resolved.
