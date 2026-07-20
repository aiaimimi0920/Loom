# Phase 18: Shared Image I/O

## Goal

Restore the first ArtLoom-compatible shared image I/O layer in Loom: named
RGBA8 shared image buffers, AHRP `shared_memory` input, and shared-memory AHRP
output for shared-memory callers.

## Tasks

- [x] P18.1 Shared image buffer store
  - Acceptance: `cargo test --manifest-path Loom/Cargo.toml -p loom_shared_image -- --nocapture`
    proves create/write/read/list/release and PNG data URL conversion.
  - Evidence:
    - `cargo test --manifest-path Loom/Cargo.toml -p loom_shared_image -- --nocapture`
      passed with 4 unit tests.
- [x] P18.2 Daemon shared-image API and AHRP routing
  - Acceptance: `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon shared -- --nocapture`
    proves helper endpoints and AHRP `shared_memory` runtime.
  - Evidence:
    - `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon shared -- --nocapture`
      passed with 2 targeted tests:
      `daemon_shared_image_api_create_list_get_delete_contract` and
      `daemon_hook_bridge_executes_shared_memory_ahrp_process`.
    - `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon`
      passed with 48 library tests and 2 CLI integration tests.
    - `cargo check --manifest-path Loom/Cargo.toml -p loom-daemon` passed.
    - `cargo fmt --manifest-path Loom/Cargo.toml --all -- --check` passed.
- [x] P18.3 Release shared image smoke
  - Acceptance: regenerated Loom release smoke records
    `sharedImageAhrpProcess` evidence while keeping Loom-only executable names.
  - Evidence:
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1`
      passed.
    - Release id: `loom-shared-image-2107b89c`.
    - Release directory:
      `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\loom-shared-image-2107b89c`.
    - Executables: `loom.exe`, `loom-daemon.exe`, `loom-desktop.exe`.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-release.ps1 -VersionId loom-shared-image-2107b89c -Apps Loom`
      passed with `gitDirty=false`.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\smoke-release-local-apps.ps1 -VersionId loom-shared-image-2107b89c -Apps Loom`
      passed.
    - Smoke summary:
      `C:\Users\Public\nas_home\AI\GameEditor\Neuro\output\smoke\runs\20260612-143907-Loom-43428-c612623d4a7540d39e110197164a7ef7\release-local-apps-loom-shared-image-2107b89c-Loom-summary.json`.

## Evidence

Phase 18 restored the first shared-image runtime layer:

- `loom_shared_image` owns RGBA8 named shared buffers and PNG data URL
  conversion.
- `loom-daemon` exposes:
  - `GET /v1/shared-images`
  - `POST /v1/shared-images`
  - `GET /v1/shared-images/<handle>`
  - `DELETE /v1/shared-images/<handle>`
- Hook Bridge AHRP `art/process` now accepts ArtLoom-compatible
  `input.type = "shared_memory"` descriptors and returns
  `output.type = "shared_memory"` for shared-memory callers.
- Packaged release smoke proved native `core.image.invert` through
  shared-memory input/output:
  `sharedImageAhrpProcess.outputType = "shared_memory"` and
  `sharedImageAhrpProcess.outputRgba = "245,235,225,255"`.

## Notes

- This phase follows the approved "方案 B 分层恢复" path after workflow graph
  runtime.
- Old source reference is read-only:
  `Z:\project\project\ArtNexus\ArtLoom`.
- Keep visible product naming as Loom:
  `loom.exe`, `loom-daemon.exe`, `loom-desktop.exe`.
- OCR and fuller image helper packs remain later phases.
