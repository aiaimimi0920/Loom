# Phase 14: Native Image Filter Runtime

## Goal

Restore old ArtLoom's built-in local native image filters in Loom and expose
them through Hook Bridge `art_loom/execute_art_node` and AHRP `art/process`.

## Tasks

- [x] P14.1 Native image filter crate
  - Acceptance: `cargo test --manifest-path Loom/Cargo.toml -p loom_native_image`
    proves `invert`, `pixelate`, unknown-art errors, and PNG base64 wrapping.
  - Evidence:
    - RED: `cargo test --manifest-path Loom/Cargo.toml -p loom_native_image -- --nocapture` failed before `process_art` existed.
    - GREEN: `cargo test --manifest-path Loom/Cargo.toml -p loom_native_image` -> 3 tests passed.
    - `cargo fmt --manifest-path Loom/Cargo.toml --all -- --check` -> passed.
- [x] P14.2 Hook Bridge native fallback
  - Acceptance: `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon`
    proves native filters execute without a registry tool through both
    `art_loom/execute_art_node` and AHRP `art/process`.
  - Evidence:
    - RED: `cargo test --manifest-path Loom/Cargo.toml -p loom_hook_bridge image_success -- --nocapture` failed before `execute_art_node_image_success_response` existed.
    - RED: `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon native_art -- --nocapture` failed before daemon native fallback because response `type` was `error`.
    - GREEN: `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon native_art -- --nocapture` -> 2 tests passed.
    - GREEN: `cargo test --manifest-path Loom/Cargo.toml -p loom_hook_bridge` -> 15 tests passed.
    - GREEN: `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon` -> 36 lib tests, 2 binary contract tests passed.
    - `cargo check --manifest-path Loom/Cargo.toml -p loom-daemon` -> passed.
    - `cargo fmt --manifest-path Loom/Cargo.toml --all -- --check` -> passed.
- [x] P14.3 Release native filter smoke
  - Acceptance: regenerated Loom release smoke proves packaged
    `loom-daemon.exe` handles native image filter execution and records
    `nativeImageFilter` evidence.
  - Evidence:
    - RED: `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1` failed until `Test-LoomHookBridgeNativeImageFilter` existed.
    - GREEN: `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1` -> passed.
    - PowerShell parser check for `scripts\smoke-release-local-apps.ps1` -> passed.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release-exes.ps1 -Apps Loom -VersionId loom-native-image-6e8fb058 -Force` -> generated `release\Loom\loom-native-image-6e8fb058`.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-release.ps1 -VersionId loom-native-image-6e8fb058 -Apps Loom` -> passed with `gitDirty = false`.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\smoke-release-local-apps.ps1 -VersionId loom-native-image-6e8fb058 -Apps Loom` -> passed.
    - Smoke summary: `output\smoke\runs\20260612-075410-Loom-61532-2e25f8d7fbce44a89c9d8465f861a822\release-local-apps-loom-native-image-6e8fb058-Loom-summary.json`.
    - Native image evidence: `requestId = release-native-image-filter`, `status = Success`, `artId = core.image.invert`, `outputType = base64`, `width = 1`, `height = 1`, `outputChanged = true`.

## Evidence

Phase 14 completed the native image filter runtime layer:
packaged `loom-daemon.exe` now handles known local native image filters without
registry tool definitions through both `art_loom/execute_art_node` and AHRP
`art/process`, returning PNG base64 data URLs.

## Notes

- Native filters in scope: `pixelate`, `blur`, `grayscale`, `brightness`,
  `contrast`, `invert`, plus their `core.image.*` aliases.
- This phase restores base64 PNG input/output only.
- Python/script/shader, cloud image API, workflow graph execution, shared
  memory, and OCR remain for later phases.
- Old source reference is read-only:
  `Z:\project\project\ArtNexus\ArtLoom`.
- Keep visible product naming as Loom.
