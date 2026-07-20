# Phase 23: Desktop Workflow Studio

## Goal

Restore a meaningful Loom desktop UI parity layer for old workflow manager,
Smart Import, workflow interface inference, and workflow-backed tool creation
without reintroducing old visible product names or old frontend dependencies.

## Tasks

- [x] P23.1 Source audit and desktop UI boundary
  - Acceptance: source-backed audit identifies old workflow manager/editor,
    Smart Import parser, workflow interface inference, current Loom desktop
    gaps, and the Phase 23 recovery boundary.
  - Evidence:
    - `docs/loom/analysis/phase-23-desktop-workflow-studio-audit.md`
      records old `App.tsx`, `AddArtModal.tsx`,
      `cliTemplateParser.ts`, `workflowArtInterface.ts`,
      workflow-manager/editor files, current Loom gaps, daemon endpoint
      constraints, implementation design, and non-goals.

- [x] P23.2 Desktop workflow studio utilities
  - Acceptance: Loom desktop has a local utility layer for cURL Smart Import,
    response templating, lightweight workflow YAML parsing, and workflow
    interface inference.
  - Evidence:
    - `Loom/apps/desktop/src/services/workflowStudio.ts` implements
      `parseCurlCommand`, `autoTemplateResponse`, `parseWorkflowYamlLite`, and
      `inferWorkflowArtInterface`.
    - The utility layer reads current `LoomToolDefinition[]` through
      `unknown`-first type guards instead of `any`.

- [x] P23.3 Desktop API and Tauri write bridge
  - Acceptance: desktop can write workflow bundles and generated tool
    definitions through current daemon APIs.
  - Evidence:
    - `Loom/apps/desktop/src/services/loomApi.ts` implements
      `putJsonViaTauri`, `saveWorkflowBundle`, and `saveToolDefinition`.
    - `Loom/apps/desktop/src-tauri/src/lib.rs` exposes
      `get_loom_daemon_json` and `put_loom_daemon_json`, with loopback-only
      daemon URL validation preserved.
    - `cargo test --manifest-path Loom/apps/desktop/src-tauri/Cargo.toml --offline -- --nocapture`
      passed with 4 tests.
    - `cargo check --manifest-path Loom/apps/desktop/src-tauri/Cargo.toml --offline`
      passed.
    - `cargo fmt --manifest-path Loom/apps/desktop/src-tauri/Cargo.toml -- --check`
      passed.

- [x] P23.4 React Workflow Studio UI
  - Acceptance: desktop exposes an editable workflow studio surface with Smart
    Import, interface inference, and workflow-to-tool wrapping.
  - Evidence:
    - `Loom/apps/desktop/src/App.tsx` implements `WorkflowStudioPanel`.
    - `Loom/apps/desktop/src/styles.css` adds Workflow Studio layout, form,
      textarea, preview, and saved-workflow list styles.
    - Browser UI smoke against `http://127.0.0.1:1423/` confirmed visible
      `Workflow Studio`, `Smart Import`, `Infer workflow interface`, and
      `Wrap workflow as Loom tool`.
    - Browser interaction smoke confirmed Smart Import preview and interface
      inference preview render correctly when no development daemon is running.
    - No visible `NeuroLoom` or `Neuro` product prefix was reintroduced in the
      desktop source.

- [x] P23.5 Contract, release, and smoke
  - Acceptance: parity contract passes; a regenerated Loom release contains
    the desktop executable and keeps all previously restored runtime parity
    smoke paths green.
  - Evidence:
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1`
      passed.
    - `npm --prefix Loom/apps/desktop run typecheck` passed.
    - `npm --prefix Loom/apps/desktop run build` passed.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release-exes.ps1 -Apps Loom -VersionId loom-desktop-workflow-studio-phase23 -Force`
      generated `release\Loom\loom-desktop-workflow-studio-phase23` with
      `loom.exe`, `loom-daemon.exe`, and `loom-desktop.exe`.
    - `packages\Loom-loom-desktop-workflow-studio-phase23-windows-x64.zip`
      was generated with size `49997281` bytes.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-release.ps1 -VersionId loom-desktop-workflow-studio-phase23 -Apps Loom`
      passed formal verification with
      `gitHead = 1103a98b852f7e92ffbb8e1655da8614c05f43d9`,
      `gitDirty = false`, and 29 checksum entries.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\smoke-release-local-apps.ps1 -VersionId loom-desktop-workflow-studio-phase23 -Apps Loom`
      passed. The smoke summary is:
      `output\smoke\runs\20260612-190801-Loom-90608-51f7f6a3dcfc48118ee9600d155571e2\release-local-apps-loom-desktop-workflow-studio-phase23-Loom-summary.json`.
    - Smoke evidence includes
      `desktopExe = "...release\Loom\loom-desktop-workflow-studio-phase23\loom-desktop.exe"`,
      `pythonToolExecution.packagedPython = true`,
      `workflowToolExecution = "script saw release workflow runtime"`,
      `workflowArtNode.type = "success"`,
      `workflowAhrpProcess.status = "Success"`,
      `cloudMultipartArtNode.multipartSeen = true`, and
      `realOcrImage.fullTextLength = 63`.

## Notes

- Old source reference is read-only:
  `Z:\project\project\ArtNexus\ArtLoom`.
- Keep visible product naming as Loom:
  `loom.exe`, `loom-daemon.exe`, `loom-desktop.exe`.
- Protocol compatibility names such as `art_loom/*`, `art_hook/*`,
  `art/process`, `shared_memory`, `image_path`, `image_base64`,
  `image_buffer`, and old cloud template names remain intentionally supported.
- Phase 23 restores a practical desktop Workflow Studio layer. It does not
  restore the full old visual graph editor, React Flow/AntD route clone, or
  complete the whole Loom migration.
- Known later work still includes deciding whether richer Python Art plugin
  management surfaces are required and running the final full-source audit
  against old ArtLoom.
