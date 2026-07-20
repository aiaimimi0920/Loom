# Phase 29: Hook Bridge Settings Compatibility

## Goal

Restore old ArtLoom Hook Bridge settings and shortcut methods that were still
advertised by Loom but were not implemented, then prove the behavior in the
packaged Loom release.

## Tasks

- [x] P29.1 Final-audit source sampling
  - Acceptance: identify the old ArtLoom settings/shortcut surfaces and the
    current Loom compatibility gap.
  - Evidence:
    - `docs/loom/analysis/phase-29-final-audit-hook-settings.md` records old
      `settings.rs`, `system_settings.rs`, `ipc_service.rs`, current Loom
      behavior, implementation, validation, release evidence, and remaining
      final-matrix classification work.
    - Old ArtLoom IPC declared `get_settings`, `get_shortcuts`,
      `update_art_param`, and `sync_shortcuts`.
    - Loom already advertised those methods in the Hook Bridge method list, but
      `handle_request` returned `Hook bridge method is not implemented`.

- [x] P29.2 Contract-first Hook Bridge settings compatibility
  - Acceptance: targeted Rust test and release contract fail before
    implementation and pass after implementation.
  - Evidence:
    - Added `handler_answers_legacy_settings_and_shortcuts`.
    - Initial RED failure:
      `left: String("error")`, `right: "success"`.
    - `scripts/tests/test-loom-artloom-parity-contract.ps1` now asserts:
      - `Test-LoomHookBridgeSettingsCompatibility`
      - `hookBridgeSettings`
    - Final contract run passed:
      `Loom ArtLoom parity release contract passed.`

- [x] P29.3 Hook Bridge handlers
  - Acceptance: Hook Bridge returns successful legacy-compatible payloads for
    settings, shortcuts, art parameter update acknowledgement, and shortcut
    sync.
  - Evidence:
    - `Loom/crates/loom_hook_bridge/src/lib.rs` implements:
      - `get_settings`
      - `get_shortcuts`
      - `update_art_param`
      - `sync_shortcuts`
    - `get_settings` returns `general.theme = "system"`,
      `general.language = "zh-Hans"`, `engine.python_interpreter =
      "python.exe"`, and default shortcut entries.
    - `get_shortcuts` returns default entries for `capture`, `ocr`,
      `color_picker`, and `cancel`.
    - `update_art_param` returns the requested `art_id`, `param_id`, and
      `value`.
    - `sync_shortcuts` returns `synced = true` and the default shortcuts.
    - `cargo test --manifest-path Loom/Cargo.toml -p loom_hook_bridge handler_answers_legacy_settings_and_shortcuts --offline -- --nocapture`
      passed.

- [x] P29.4 Release smoke proof
  - Acceptance: the packaged Loom release proves the restored methods through
    the real Hook Bridge WebSocket and all prior parity smoke paths remain
    green.
  - Evidence:
    - `scripts/smoke-release-local-apps.ps1` adds
      `Test-LoomHookBridgeSettingsCompatibility`.
    - Smoke exercises:
      - `get_settings`
      - `get_shortcuts`
      - `update_art_param`
      - `sync_shortcuts`
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release-exes.ps1 -Apps Loom -VersionId loom-hook-settings-phase29 -Force`
      generated `release\Loom\loom-hook-settings-phase29` with `loom.exe`,
      `loom-daemon.exe`, and `loom-desktop.exe`.
    - `release\Loom\loom-hook-settings-phase29\packages\Loom-loom-hook-settings-phase29-windows-x64.zip`
      was generated with size `50101437` bytes and sha256
      `6a5623a38584e9206967f7337929a4534cb0d0ebdc1840146ca181e17e36d389`.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-release.ps1 -VersionId loom-hook-settings-phase29 -Apps Loom`
      passed formal verification with
      `gitHead = e0081f7412875cd31848081154530cfe9fdae0ca`,
      `gitDirty = false`, and 31 checksum entries.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\smoke-release-local-apps.ps1 -VersionId loom-hook-settings-phase29 -Apps Loom`
      passed. The smoke summary is:
      `output\smoke\runs\20260612-224237-Loom-63436-fbe6f33fc618425695b06a5f913d1777\release-local-apps-loom-hook-settings-phase29-Loom-summary.json`.
    - Smoke evidence includes:
      - `hookBridgeSettings.settingsTheme = "system"`
      - `hookBridgeSettings.shortcutCount = 4`
      - `hookBridgeSettings.updatedArtId = "fixture-art"`
      - `hookBridgeSettings.synced = true`
      - `pythonArtSourceImport.scriptToolExecution = "source import saw release source helper"`
      - `mcpMarketplace.connectionTestSuccess = true`
      - `managementCrud.workflowDeleted = true`
      - `pythonArtToolExecution = "python art saw release installed python art"`
      - `cloudMultipartArtNode.multipartSeen = true`
      - `realOcrImage.fullTextLength = 63`
      - `workflowArtNode.success = true`
      - `workflowAhrpProcess.status = "Success"`

## Notes

- Old source reference is read-only:
  `Z:\project\project\ArtNexus\ArtLoom`.
- Phase 29 restores the ArtHook-facing compatibility methods. It does not
  replace Loom's managed configuration UI.
- The final ArtLoom parity matrix is still required before claiming the Loom
  migration is彻底 complete.
