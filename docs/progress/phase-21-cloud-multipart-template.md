# Phase 21: Cloud Multipart Template Parity

## Goal

Restore old ArtLoom cloud API multipart/template behavior on top of the Phase 16
JSON cloud runtime, including packaged release proof through Hook Bridge Art
node execution.

## Tasks

- [x] P21.1 Source audit and runtime contract
  - Acceptance: source-backed audit identifies old ArtLoom cloud config fields,
    template forms, multipart file-field behavior, and current Loom gaps.
  - Evidence:
    - `docs/loom/analysis/phase-21-cloud-multipart-template-audit.md` records
      the old source files, execution config fields, template forms, multipart
      skip rules, file-field detection, UI behavior, and Loom implementation
      boundaries.
- [x] P21.2 Tool registry multipart/template runtime
  - Acceptance: registry cloud API execution accepts old `url`, `contentType`,
    `headers`, and `body` config and sends templated multipart requests.
  - Evidence:
    - `cargo test --manifest-path Loom/Cargo.toml -p loom_tool_registry execute_cloud_api_tool_supports_artloom_multipart_template_contract --offline -- --nocapture`
      passed.
    - The test proves old `url` deserialization, route/header/body template
      substitution, multipart file upload, skipped empty/disabled fields, and no
      unresolved templates.
- [x] P21.3 Hook Bridge cloud image input path parity
  - Acceptance: daemon Hook Bridge cloud Art node execution provides
    `{{inputs.input.path}}` with an actual temp file path while preserving
    `input_base64` for existing JSON cloud tools.
  - Evidence:
    - `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon daemon_hook_bridge_executes_cloud_api_multipart_art_node_with_input_file --offline -- --nocapture`
      passed.
    - The test proves `/multipart/image`, templated `X-Trace`, multipart
      content type with boundary, `file` part, `loom-cloud-input-*` filename,
      prompt field substitution, and no unresolved templates.
- [x] P21.4 Packaged release multipart smoke
  - Acceptance: regenerated Loom release smoke proves the restored old ArtLoom
    multipart/template cloud config using packaged `loom-daemon.exe`.
  - Evidence:
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1`
      passed.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release-exes.ps1 -Apps Loom -VersionId loom-cloud-multipart-phase21 -Force`
      generated `release\Loom\loom-cloud-multipart-phase21` with
      `loom.exe`, `loom-daemon.exe`, and `loom-desktop.exe`.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-release.ps1 -VersionId loom-cloud-multipart-phase21 -Apps Loom`
      passed formal verification with
      `gitHead = a216192eb5f7a591bf5c7dfbc84201561aebdbdf`,
      `gitDirty = false`, and 18 checksum entries.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\smoke-release-local-apps.ps1 -VersionId loom-cloud-multipart-phase21 -Apps Loom`
      passed. The smoke summary includes
      `cloudMultipartArtNode.success = true`,
      `cloudMultipartArtNode.multipartSeen = true`,
      `cloudMultipartArtNode.fileFieldSeen = true`,
      `cloudMultipartArtNode.tempFilenameSeen = true`,
      `cloudMultipartArtNode.promptSeen = true`,
      `cloudMultipartArtNode.traceSeen = true`, and
      `cloudMultipartArtNode.unresolvedTemplateSeen = false`.

## Notes

- Old source reference is read-only:
  `Z:\project\project\ArtNexus\ArtLoom`.
- Keep visible product naming as Loom:
  `loom.exe`, `loom-daemon.exe`, `loom-desktop.exe`.
- Protocol compatibility names such as `art_loom/*`, `art_hook/*`,
  `art/process`, `shared_memory`, `image_path`, `image_base64`,
  `image_buffer`, and old cloud template names remain intentionally supported.
- Phase 21 restores multipart/template cloud parity. It does not complete the
  full Loom migration. Known later work still includes embedded Python
  packaging parity, fuller desktop workflow editor/import/interface inference UI
  parity, and a final full-source audit against old ArtLoom.
