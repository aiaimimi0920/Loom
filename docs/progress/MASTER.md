# Loom Migration Progress Master

## Task

Maintain Loom as Neuro's independently versioned AI brain and orchestration
runtime, with a reproducible standalone repository and parent submodule pin.

## Definition

Loom owns agent planning, workflows, memory, durable orchestration, safe tool
execution, hooks, and Gateway-backed model access. Gateway remains separate and
owns provider routing, credentials, and API relay. Platform and Hook remain
separate projects.

## Analysis documents

- [Project overview](../analysis/loom-project-overview.md)
- [Module inventory](../analysis/loom-module-inventory.md)
- [Risk assessment](../analysis/loom-risk-assessment.md)
- [Source migration matrix](../analysis/loom-source-migration-matrix.md)
- [Final ArtLoom parity matrix](../analysis/final-artloom-parity-matrix.md)
- [Phase 31 ArtLoom desktop UX parity audit](../analysis/phase-31-artloom-ux-parity-audit.md)

## Plan documents

- [Task breakdown](../plan/loom-task-breakdown.md)
- [Dependency graph](../plan/loom-dependency-graph.md)
- [Milestones](../plan/loom-milestones.md)
- [Implementation plan](../superpowers/plans/2026-06-03-loom-migration-implementation-plan.md)

## Phase summary

- [x] Phase 0: Migration audit and source map (3/3 tasks) [details](./phase-0-migration-audit.md)
- [x] Phase 1: Workspace skeleton (2/2 tasks) [details](./phase-1-workspace-skeleton.md)
- [x] Phase 2: Core and durable runtime (3/3 tasks) [details](./phase-2-core-durable.md)
- [x] Phase 3: Agents and workflows (4/4 tasks) [details](./phase-3-agents-workflows.md)
- [x] Phase 4: Gateway, sandbox, and hooks (3/3 tasks) [details](./phase-4-integrations.md)
- [x] Phase 5: Daemon and CLI (3/3 tasks) [details](./phase-5-daemon-cli.md)
- [x] Phase 6: ArtLoom migration adapters (2/2 tasks) [details](./phase-6-artloom-adapters.md)
- [x] Phase 7: Final validation and baseline (2/2 tasks) [details](./phase-7-validation-baseline.md)
- [x] Phase 8: ArtLoom control-plane parity (7/7 tasks) [details](./phase-8-control-plane-parity.md)
- [x] Phase 9: Runtime bridge parity (5/5 tasks) [details](./phase-9-runtime-bridge-parity.md)
- [x] Phase 10: Hook WebSocket compatibility (2/2 tasks) [details](./phase-10-hook-websocket-compatibility.md)
- [x] Phase 11: Hook broadcast fanout (2/2 tasks) [details](./phase-11-hook-broadcast-fanout.md)
- [x] Phase 12: Art node execution runtime (3/3 tasks) [details](./phase-12-art-node-execution.md)
- [x] Phase 13: AHRP process compatibility (3/3 tasks) [details](./phase-13-ahrp-process-compatibility.md)
- [x] Phase 14: Native image filter runtime (3/3 tasks) [details](./phase-14-native-image-filter.md)
- [x] Phase 15: Python / script / shader runtime (3/3 tasks) [details](./phase-15-python-script-shader-runtime.md)
- [x] Phase 16: Cloud API runtime (3/3 tasks) [details](./phase-16-cloud-api-runtime.md)
- [x] Phase 17: Workflow graph runtime (3/3 tasks) [details](./phase-17-workflow-graph-runtime.md)
- [x] Phase 18: Shared image I/O (3/3 tasks) [details](./phase-18-shared-image-io.md)
- [x] Phase 19: OCR and image helper parity (3/3 tasks) [details](./phase-19-ocr-image-helper-parity.md)
- [x] Phase 20: Real OCR engine packaging (3/3 tasks) [details](./phase-20-real-ocr-engine.md)
- [x] Phase 21: Cloud multipart/template parity (4/4 tasks) [details](./phase-21-cloud-multipart-template.md)
- [x] Phase 22: Embedded Python packaging (4/4 tasks) [details](./phase-22-embedded-python-packaging.md)
- [x] Phase 23: Desktop Workflow Studio (5/5 tasks) [details](./phase-23-desktop-workflow-studio.md)
- [x] Phase 24: Python Art catalog parity (5/5 tasks) [details](./phase-24-python-art-catalog.md)
- [x] Phase 25: Management CRUD parity (5/5 tasks) [details](./phase-25-management-crud.md)
- [x] Phase 26: MCP Marketplace parity (5/5 tasks) [details](./phase-26-mcp-marketplace.md)
- [x] Phase 27: Workflow Graph UI parity (5/5 tasks) [details](./phase-27-workflow-graph-ui.md)
- [x] Phase 28: Python Source Import helper parity (5/5 tasks) [details](./phase-28-python-source-import.md)
- [x] Phase 29: Hook Bridge settings compatibility (4/4 tasks) [details](./phase-29-hook-settings-compatibility.md)
- [x] Phase 30: Loom naming cleanup (3/3 tasks) [details](./phase-30-loom-naming-cleanup.md)
- [x] Phase 31: ArtLoom desktop UX parity (10/10 tasks) [details](./phase-31-artloom-ux-parity.md)
- [x] Phase 32: Advanced Add Art editor parity (6/6 tasks) [details](./phase-32-advanced-add-art-editor-parity.md)
- [x] Phase 33: Settings/session/package compatibility (5/5 tasks) [details](./phase-33-settings-session-package-compat.md)
- [x] Phase 34: ArtLoom compatibility aliases (5/5 tasks) [details](./phase-34-compat-aliases.md)
- [x] Phase 35: Chinese desktop UI and Hook screenshot workflow entry (4/4 tasks) [details](./phase-35-cn-ui-hook-sync.md)
- [x] Phase 36: Desktop startup and Hook live workflow visibility (4/4 tasks) [details](./phase-36-desktop-start-hook-workflow.md)
- [x] Phase 37: Hook live workflow release smoke (3/3 tasks) [details](./phase-37-hook-live-release-smoke.md)
- [x] Phase 38: Desktop Chinese copy polish (4/4 tasks) [details](./phase-38-desktop-cn-polish.md)
- [x] Phase 39: Gateway-backed brain planning (7/7 tasks) [details](./phase-39-gateway-brain-plan.md)
- [x] Phase 40: Durable run and event persistence (7/7 tasks) [details](./phase-40-run-event-persistence.md)
- [x] Phase 41: Bounded daemon concurrency (7/7 tasks) [details](./phase-41-bounded-daemon-concurrency.md)
- [x] Phase 42: Single-entry Windows release packaging (6/6 tasks)
  [details](../superpowers/plans/2026-07-21-loom-single-entry-release.md)
- [x] Standalone repository publication and Neuro submodule integration
  [details](../superpowers/plans/2026-07-20-loom-standalone-repository-migration.md)

## Current status

Phase 1 workspace skeleton, Phase 2 core/durable runtime, Phase 3
agent/workflow runtime, Phase 4 integration/safety contracts, Phase 5
daemon/CLI surfaces, Phase 6 ArtLoom migration adapters, Phase 7 final
validation/baseline docs, Phase 8 ArtLoom control-plane parity, Phase 9 runtime
bridge parity, Phase 10 Hook WebSocket request/reply compatibility, Phase 11
subscribed Hook broadcast fanout, Phase 12 MCP-backed Art node execution, and
Phase 13 AHRP process compatibility, Phase 14 native image filter runtime,
Phase 15 registry-backed Python/script/shader-style runtime, Phase 16
registry-backed cloud API runtime, and Phase 17 registry-backed workflow graph
runtime are implemented. A later parity audit found that the original baseline
incorrectly deferred ArtLoom's MCP, registry, workflow store, and Hook bridge
capabilities.
Current validation covers formatting, workspace compile, workspace tests,
daemon/CLI smoke, CLI fixture smoke, ArtLoom conversion smokes, Loom local
capability discovery/invocation, `brain.plan`, run/event retrieval, the
non-loopback bearer-token guard, the packaged Loom desktop shell, Phase 8
control-plane API release smoke, Phase 9 runtime bridge release smoke, Phase 10
packaged Hook WebSocket handshake release smoke, Phase 11 packaged subscribed
WebSocket broadcast release smoke, Phase 12 packaged `execute_art_node` release
smoke, Phase 13 packaged AHRP `art/process` release smoke, and Phase 14
packaged native image filter release smoke, Phase 15 packaged
script/Python/shader runtime release smoke, Phase 16 packaged cloud API runtime
release smoke, Phase 17 packaged workflow graph runtime release smoke, and
Phase 18 packaged shared image I/O release smoke, Phase 19 packaged OCR
protocol plus image helper conversion release smoke, and Phase 20 packaged real
PaddleOCR/ONNX OCR release smoke, and Phase 21 packaged old ArtLoom cloud
multipart/template release smoke, and Phase 22 packaged embedded Python script
execution release smoke, Phase 23 packaged desktop Workflow Studio UI parity
plus release smoke, and Phase 24 packaged Python Art catalog plus
launcher-backed Python Art execution release smoke, and Phase 25 packaged
management CRUD release smoke, and Phase 26 packaged MCP Registry marketplace
plus connection-test release smoke, and Phase 27 packaged visual Workflow
Studio graph UI parity plus release smoke, and Phase 28 packaged Python source
import helper parity plus release smoke, Phase 29 packaged Hook Bridge settings
compatibility release smoke, Phase 30 Loom naming cleanup plus regenerated
release smoke, Phase 31 user-visible ArtLoom desktop UX parity plus
regenerated release smoke, Phase 32 advanced Add Art editor parity, and Phase
33 settings/session/package compatibility, and Phase 34 explicit ArtLoom
compatibility aliases, Phase 35 Chinese desktop UI plus visible Hook
screenshot-to-workflow entry, Phase 36 desktop-managed local service startup
plus Hook live workflow visibility, Phase 37 packaged Hook live workflow
persistence release smoke plus UTF-8 JSON response hardening, and Phase 38
desktop Chinese copy polish across local-service errors, MCP Marketplace, and
Workflow Studio warnings, and Phase 39 Gateway-backed brain planning with
formal packaged local, Gateway, and desktop auto-start evidence, and Phase 40
durable run/event persistence with formal restart, desktop, and Gateway evidence.
Phase 41 is complete at 7/7 tasks. The production daemon now uses bounded
request workers, approved routes can progress while Gateway work is blocked,
legacy control-plane routes retain a serialized boundary, and queue overload is
reported as a retryable HTTP 503 without creating run evidence. Full Rust,
workspace, desktop, Tauri, contract, package, and formal release validation has
passed. The current candidate is
`release/Loom/20260721-standalone-161b8aa`.

Phase 42 changes the Windows desktop distribution boundary without changing
the daemon API or process model. The packaged desktop candidate now exposes
only `Loom.exe` at its root; `runtime/loom-daemon.exe` and all daemon-owned
OCR, embedded-Python, and Python Art resources live below `runtime/`. The CLI
is published separately as `Loom-CLI-<versionId>-windows-x64.zip` containing
only `loom.exe`. The release verifier, desktop/persistence/Gateway/concurrency
smokes, checksums, and Actions release asset contract all cover this layout.
A final clean candidate must be generated from the committed Phase 42 SHA
before it replaces the historical Phase 41 candidate as the formal package.

The standalone repository is published at
`https://github.com/aiaimimi0920/Loom`. Runtime and package validation closed on
`161b8aaa2dd8f31016eb1910850ac7fbf5bc65b0`; the current main-branch runtime-test
head is `a3b081c869cae4a8b8115759276acb5ce6985acc`. Commits after `161b8aa` only
harden filesystem and segmented-TCP test fixtures, so the verified release was
not rebuilt. Neuro consumes Loom through a mode `160000` Git submodule instead
of a duplicated source tree.

Phase 39 is complete. It has a real OpenAI-compatible Gateway transport,
opt-in Gateway-backed `brain.plan`, strict model output validation, safe
planner status metadata, and queryable success/failure run evidence. Formal
candidate `release/Loom/20260719-051119-dcdc94a8` passed the release verifier,
unified local smoke, packaged Gateway smoke, and desktop sibling-daemon
auto-start smoke.

Phase 40 is complete at 7/7 tasks. Packaged `loom-daemon.exe` uses bundled
SQLite for run/event evidence, validates and recovers the database before
serving, preserves canonical stop/retry history, and has passed the formal
restart persistence, desktop sibling-daemon, Gateway, and unified release smoke
matrix. Candidate `release/Loom/20260719-082918-923fc5f8` is the Phase 40 formal
release candidate and historical predecessor to the current Phase 41 candidate.

Latest completed phases:

- Phase 38 is complete with regenerated release evidence. It removes remaining
  user-visible English and internal daemon wording from the desktop local
  service errors, MCP Marketplace labels/statuses/categories, and Workflow
  Studio fallback warnings while preserving the Phase 37 Hook live workflow
  release smoke evidence.
- Phase 39 is complete. The Gateway path is opt-in through
  `LOOM_GATEWAY_MODEL`; configured Gateway failure returns HTTP 502 and leaves
  failed run/event evidence. The formal candidate preserves scoped Loom source
  provenance and passed the complete release-level smoke matrix.
- Phase 40 is complete. Durable SQLite run/event evidence survives daemon
  restart, stale running records recover as explicit `daemon_restarted` failures
  without replay, and the formal candidate passed source, package, persistence,
  desktop sibling-daemon, Gateway, and unified release validation.
- Phase 41 is complete. The production daemon uses bounded request workers with
  `4/32` defaults, reserved health/status probes, an explicit approved-route
  concurrency allowlist, serialized legacy routes, retryable overload errors
  with no-run semantics, and graceful worker drain. The formal candidate passed
  source, desktop, package, persistence, Gateway, unified, and concurrency
  validation.
- Phase 42 is complete at the implementation level. It gives the Windows
  desktop package one visible `Loom.exe`, moves the daemon and support tree to
  `runtime\`, publishes the CLI as an independently checked ZIP, and updates
  the operator documentation and GitHub release asset contract. Final package
  provenance is taken from the clean commit recorded in `manifest.json`.
- Standalone publication is complete. The independent repository has portable
  Windows/Linux tests, self-contained release tooling, four active GitHub
  Actions workflows, Docker validation, dual licensing, provenance, and a
  public submodule URL. The final `a3b081c` runtime-test head passed hosted CI,
  Build Windows, and Docker workflows.

Completed Phase 8 tasks:

- P8.1 MCP core contracts.
- P8.2 Workflow store and graph codec.
- P8.3 Tool registry.
- P8.4 Hook bridge protocol contracts.
- P8.5 Daemon control-plane APIs.
- P8.6 Desktop control-plane surfaces.
- P8.7 Release parity smoke.

Last completed phase:

- Phase 42 adds the single-entry Windows release boundary without changing
  daemon HTTP semantics or merging processes. It separates `Loom.exe`,
  `runtime\loom-daemon.exe`, and the CLI ZIP, with release verifier and smoke
  coverage for both artifacts.

Previous completed phase:

- Phase 41 added bounded daemon request execution without automatic replay or
  forced cancellation, preserved serialized compatibility routes, and closed
  with formal release, unified local, persistence, Gateway, desktop, and
  concurrency evidence.

Earlier completed phase:

- Phase 40 added durable capability run/event evidence without adding a worker
  queue or replay semantics, and closed with formal release, unified local,
  packaged persistence, Gateway, and desktop sibling-daemon evidence.

Earlier foundation phase:

- Phase 39 connected `brain.plan` to the real Gateway only when explicitly
  configured, retained deterministic local planning by default, made Gateway
  failures queryable without leaking credentials or prompts, and closed with
  formal release, unified local, packaged Gateway, and desktop auto-start
  evidence.

Repository status:

- Standalone publication is complete and Phase 42 is the current release
  packaging baseline. Neuro owns the scoped `release/Loom` destination through an
  explicit release output parameter and tracks Loom through `.gitmodules` plus
  a mode `160000` gitlink. The initial parent integration commit is
  `86105d555a01ad31b00e1328a011eb0f12828c18`; parent pins are advanced only
  after the corresponding standalone Actions runs succeed.

Ongoing maintenance:

- Keep the final parity matrix aligned with user-visible requirements; do not
  downgrade UI-visible ArtLoom flows to API-only parity. Keep Hook screenshot
  synchronization visible as a UI route, not only as protocol methods.

## Next steps

1. Generate the final clean Phase 42 candidate under the mandated
   `Neuro\release\Loom` boundary from the committed standalone SHA, then run
   the verifier with the full smoke matrix. Keep
   `20260721-standalone-161b8aa` and all earlier candidates as immutable
   historical evidence.
2. Treat later test/documentation-only commits separately from runtime package
   provenance. Regenerate a candidate after any production source, resource,
   dependency, or release-tooling change.
3. If further ArtLoom gaps are reported, classify them as user-visible UI,
   protocol/runtime, packaging, or intentionally replaced before making changes.
4. Preserve product naming as `Loom`; `loom-desktop.exe` remains only the
   internal Tauri source target, while the packaged user entry is `Loom.exe`.
5. Start the next numbered phase only after defining its scope, evidence, and
   release boundary; keep historical candidates immutable.

## Standalone repository closure

- Public repository: `https://github.com/aiaimimi0920/Loom`.
- Standalone commit chain: `4749f11` initial publication, `a65443a` portable
  hosted-runner paths, `9a2c69f` portable APPDATA assertion, `161b8aa` isolated
  filesystem-backed daemon contracts, and `a3b081c` complete segmented HTTP
  fixture reads.
- Verified user-requested backup:
  `C:\Users\Public\nas_home\AI\GameEditor\_temp\Loom-standalone-backup-20260720-195938-be4bbb7b`.
  `backup-location-verification.json` records `verified=true`, 134/134 manifest
  files, 58,725,037 manifest bytes, 212 copied-tree files, and no failures.
- Preserved parent working copy:
  `C:\Users\Public\nas_home\AI\GameEditor\Neuro\_temp\Loom-parent-pre-submodule-20260721-0115-4749f11`
  with 18,070 files retained.
- Parent integration baseline:
  `86105d555a01ad31b00e1328a011eb0f12828c18`, with `.gitmodules` pointing to the
  public repository and `Loom` stored as mode `160000`.
- Final review pin:
  `724a26f5b2821c951f411bab60de4facb948aa0e` points the parent gitlink to
  `3ebc74f5b713892e0418182cc60f88f6d9bed12b`; an isolated parent clone
  initialized Loom from `https://github.com/aiaimimi0920/Loom.git` and remained
  clean after verification.
- Formal candidate:
  `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260721-standalone-161b8aa`.
  The verifier accepted all 31 checksum entries, and unified, SQLite
  persistence, Gateway, and bounded-concurrency smokes passed with no candidate
  process left running.
- Local validation: Windows `loom_tool_registry` 18/18 and daemon 110/110;
  Linux read-only-source workspace tests passed, with 109 daemon tests passing
  and one expected Windows-only OCR runtime test ignored.
- Hosted validation for `a3b081c`: [CI](https://github.com/aiaimimi0920/Loom/actions/runs/29766615127),
  [Build Windows](https://github.com/aiaimimi0920/Loom/actions/runs/29766615320),
  and [Docker](https://github.com/aiaimimi0920/Loom/actions/runs/29766615150)
  all completed successfully.
- Runtime evidence remains under the isolated clean clone at
  `target/runtime-smoke/runs/20260721-020039-Loom-47172-4e435cca876c406681d11e2d2bd5f891`,
  `target/runtime-smoke/persistence`, `target/runtime-smoke/gateway`, and
  `target/runtime-smoke/daemon-concurrency`.
- Clippy is not a migration completion gate in the current Actions contract;
  existing lint cleanup remains ordinary maintenance and is not hidden by this
  repository extraction.

## Notes

- Loom is an independent Rust workspace published at
  `https://github.com/aiaimimi0920/Loom` and consumed by Neuro as a pinned
  submodule.
- Phase 7 validation was refreshed on 2026-06-04 with fmt, workspace check,
  workspace tests, targeted crate tests, ArtLoom conversion tests, and
  daemon/CLI smoke passing.
- Do not copy Platform, Gateway, or Hook implementation into Loom.
- Use `ArtNexus-GitHub\ArtLoom` as clean ArtLoom baseline.
- Use `ArtNexus\ArtLoom` only for reviewed local deltas.
- Use old internal `NeuroLoom` material only as a Rust runtime architecture
  reference; do not expose NeuroLoom/Neuro prefixes in Loom product names.
- Phase 8 corrects the previous scope decision that treated MCP, registry,
  workflow store, desktop control plane, and 19820 Hook bridge behavior as out
  of scope.
- Phase 9 turns the Phase 8 contracts into executable runtime behavior.
- Phase 9 generated `loom-runtime-bridge-0c703230` with `loom.exe`,
  `loom-daemon.exe`, and `loom-desktop.exe`.
- Phase 10 restores the WebSocket transport layer that old ArtLoom exposed on
  port `19820`.
- Phase 10 generated `loom-hook-ws-6b8410b8` with `loom.exe`,
  `loom-daemon.exe`, and `loom-desktop.exe`, and release smoke proved
  WebSocket `handshake` response evidence.
- Phase 11 generated `loom-hook-broadcast-db2be04f` with `loom.exe`,
  `loom-daemon.exe`, and `loom-desktop.exe`, and release smoke proved subscribed
  WebSocket `art_hook/instantiate` broadcast delivery.
- Phase 12 generated `loom-art-node-565a4966` with `loom.exe`,
  `loom-daemon.exe`, and `loom-desktop.exe`, and release smoke proved
  MCP-backed `art_loom/execute_art_node` execution.
- Phase 13 generated `loom-ahrp-process-74c2a485` with `loom.exe`,
  `loom-daemon.exe`, and `loom-desktop.exe`, and release smoke proved
  MCP-backed AHRP `art/process` base64 output.
- Phase 14 generated `loom-native-image-6e8fb058` with `loom.exe`,
  `loom-daemon.exe`, and `loom-desktop.exe`, and release smoke proved
  packaged native image filter execution with `nativeImageFilter.outputChanged
  = true`.
- Phase 15 generated `loom-script-runtime-fc53e333` with `loom.exe`,
  `loom-daemon.exe`, and `loom-desktop.exe`, and release smoke proved packaged
  script-backed direct tool execution, script Art node base64 output, script
  AHRP base64 output, and script shader text output.
- Phase 16 generated `loom-cloud-api-0b5b3a84` with `loom.exe`,
  `loom-daemon.exe`, and `loom-desktop.exe`, and release smoke proved packaged
  cloud API direct tool execution, cloud Art node base64 output, and cloud AHRP
  base64 output.
- Phase 17 generated `loom-workflow-runtime-efd911d8` with `loom.exe`,
  `loom-daemon.exe`, and `loom-desktop.exe`, and release smoke proved packaged
  workflow-backed direct tool execution, workflow Art node base64 output, and
  workflow AHRP base64 output.
- Phase 18 generated `loom-shared-image-2107b89c` with `loom.exe`,
  `loom-daemon.exe`, and `loom-desktop.exe`, and release smoke proved packaged
  shared image helper APIs plus Hook Bridge AHRP `shared_memory` input/output
  with `sharedImageAhrpProcess.outputRgba = "245,235,225,255"`.
- Phase 19 P19.1 audited old ArtLoom `art_loom/ocr_image`,
  `ocr_service.rs`, `converters.rs`, and packaged OCR model resources. The
  phase intentionally restores OCR protocol/helper parity before real
  PaddleOCR/ONNX engine packaging.
- Phase 19 generated `loom-ocr-image-helper-8ad62b76` with `loom.exe`,
  `loom-daemon.exe`, and `loom-desktop.exe`, and release smoke proved packaged
  `POST /v1/image-helpers/convert` plus Hook Bridge `art_loom/ocr_image` with
  `imageHelperConvert.outputRgba = "10,20,30,255"` and
  `ocrImage.fullText = "release loom ocr"`.
- Phase 20 generated `loom-real-ocr-phase20` with `loom.exe`,
  `loom-daemon.exe`, and `loom-desktop.exe`. Formal release verification passed
  with `gitDirty=false` and 18 checksum entries. Release smoke kept the fixture
  OCR protocol check and added real packaged OCR evidence:
  `realOcrImage.fullTextLength = 63`, `realOcrImage.width = 678`,
  `realOcrImage.height = 108`, and `realOcrImage.blockCount = 2`.
- Phase 21 P21.1 audited old ArtLoom `cloud_engine.rs`, `converters.rs`,
  `AddArtModal.tsx`, and `cliTemplateParser.ts` for multipart/template cloud
  behavior. P21.2 restored old `url`, `contentType`, `headers`, `body`, and
  multipart request execution in `loom_tool_registry`. P21.3 restored Hook
  Bridge cloud Art node temp file injection for `{{inputs.input.path}}`.
- Phase 21 generated `loom-cloud-multipart-phase21` with `loom.exe`,
  `loom-daemon.exe`, and `loom-desktop.exe`. Formal release verification passed
  with `gitDirty=false` and 18 checksum entries. Release smoke proved packaged
  old ArtLoom-style multipart cloud execution with
  `cloudMultipartArtNode.multipartSeen = true`,
  `cloudMultipartArtNode.fileFieldSeen = true`,
  `cloudMultipartArtNode.tempFilenameSeen = true`,
  `cloudMultipartArtNode.promptSeen = true`,
  `cloudMultipartArtNode.traceSeen = true`, and
  `cloudMultipartArtNode.unresolvedTemplateSeen = false`.
- Phase 22 P22.1 audited old ArtLoom `bin/python-embed`,
  `python/Launcher.py`, `python_engine.rs`, `mcp_engine.rs`, and packaged dist
  layout. P22.2 staged a minimal Loom-owned embedded Python runtime and
  `python/Launcher.py`. P22.3 changed `.py` script-backed tools to prefer
  `LOOM_PYTHON`, then package-local `bin/python-embed/python.exe`, then
  development/PATH fallbacks. P22.4 added release smoke proof for actual
  packaged Python execution.
- Phase 22 generated `loom-embedded-python-phase22` with `loom.exe`,
  `loom-daemon.exe`, `loom-desktop.exe`, `bin\python-embed\python.exe`, and
  `python\Launcher.py`. Formal release verification passed with
  `gitDirty=false` and 29 checksum entries. Release smoke proved packaged
  `.py` script execution with
  `pythonToolExecution.text = "python saw release embedded python"`,
  `pythonToolExecution.pythonExecutable` pointing to
  `release\Loom\loom-embedded-python-phase22\bin\python-embed\python.exe`, and
  `pythonToolExecution.packagedPython = true`.
- Phase 23 P23.1 audited old workflow manager/editor UI, `AddArtModal.tsx`,
  `cliTemplateParser.ts`, and `workflowArtInterface.ts`. P23.2 added
  `workflowStudio.ts` with cURL Smart Import, response templating, lightweight
  workflow YAML parsing, and workflow interface inference. P23.3 added desktop
  daemon PUT support through `putJsonViaTauri` and `put_loom_daemon_json`.
  P23.4 added `WorkflowStudioPanel` with editable YAML, Smart Import,
  interface inference, and `Wrap workflow as Loom tool`. P23.5 regenerated the
  release.
- Phase 23 generated `loom-desktop-workflow-studio-phase23` with `loom.exe`,
  `loom-daemon.exe`, and `loom-desktop.exe`. Formal release verification passed
  with `gitDirty=false` and 29 checksum entries. Release smoke proved the
  packaged desktop executable still exists and all prior runtime parity paths
  remain green, including embedded Python, cloud multipart, real OCR, workflow
  direct execution, workflow Art node execution, and workflow AHRP execution.
- Phase 24 P24.1 audited old ArtLoom `python_engine.rs`, `AddArtModal.tsx`,
  `python/Launcher.py`, and packaged `python/Arts` resources. P24.2 staged
  Loom-owned `python/Arts/Art_LoomEcho`. P24.3 restored registry
  `execution.type = "python_art"`. P24.4 exposed daemon `/v1/python-arts` and
  desktop Registry import. P24.5 regenerated the release.
- Phase 24 generated `loom-python-art-catalog-phase24` with `loom.exe`,
  `loom-daemon.exe`, `loom-desktop.exe`,
  `python\Arts\Art_LoomEcho\art.json`, and
  `python\Arts\Art_LoomEcho\main.py`. Formal release verification passed with
  `gitDirty=false` and 31 checksum entries. Release smoke proved packaged
  Python Art discovery and execution:
  `pythonArtCatalog.artId = "loom_echo"`,
  `pythonArtCatalog.count = 1`,
  `pythonArtToolExecution = "python art saw release installed python art"`,
  and `pythonToolExecution.packagedPython = true`.
- Phase 25 P25.1 audited old ArtLoom `mcp_engine.rs`,
  `workflow_store.rs`, `ipc_service.rs`, `runtimeBridge.ts`,
  `useWorkflows.ts`, `WorkflowList.tsx`, `Canvas.tsx`, and
  `MCPSettings.tsx` for management CRUD behavior. P25.2 restored daemon
  `DELETE /v1/mcp/servers/{serverId}`, `DELETE /v1/tools/{toolId}`,
  `GET /v1/workflows/{workflowId}`, and
  `DELETE /v1/workflows/{workflowId}`. P25.3 added desktop DELETE bridge and
  API helpers. P25.4 added desktop `Delete server`, `Delete tool`,
  `Load YAML`, and `Delete workflow` actions. P25.5 regenerated the release.
- Phase 25 generated `loom-management-crud-phase25` with `loom.exe`,
  `loom-daemon.exe`, and `loom-desktop.exe`. Formal release verification
  passed with `gitDirty=false` and 31 checksum entries. Release smoke proved
  packaged management CRUD:
  `managementCrud.mcpServerDeleted = true`,
  `managementCrud.toolDeleted = true`,
  `managementCrud.workflowLoaded = "release-workflow"`, and
  `managementCrud.workflowDeleted = true`.
- Phase 26 P26.1 audited old ArtLoom `mcp_engine.rs`,
  `MCPSettings.tsx`, and `features/mcp/marketplace.ts` for MCP Registry,
  curated marketplace, install/update, and connection-test behavior. P26.2
  restored daemon `GET /v1/mcp/registry`, `POST /v1/mcp/test`, and
  `LOOM_MCP_REGISTRY_ENDPOINT`. P26.3 added desktop marketplace mapping and API
  helpers. P26.4 added desktop `Configured servers`, `MCP Marketplace`,
  `Refresh Registry`, `Install server`, and `Install & Test`. P26.5
  regenerated the release.
- Phase 26 generated `loom-mcp-marketplace-phase26` with `loom.exe`,
  `loom-daemon.exe`, and `loom-desktop.exe`. Formal release verification
  passed with `gitDirty=false` and 31 checksum entries. Release smoke proved
  packaged MCP marketplace parity:
  `mcpMarketplace.registryServerName = "io.modelcontextprotocol/fixture"`,
  `mcpMarketplace.connectionTestSuccess = true`,
  `mcpMarketplace.connectionTestTool = "echo"`, and
  `mcpMarketplace.connectionTestServer = "release-fixture"`.
- Phase 27 P27.1 audited old ArtLoom ReactFlow workflow editor sources,
  including `workflow-editor/index.tsx`, `Canvas.tsx`, `Sidebar.tsx`,
  `PropertiesPanel.tsx`, and `RunSummaryStrip.tsx`. P27.2 added parity
  contract assertions for `Graph view`, `Node properties`, graph node CRUD, and
  graph helper functions. P27.3 added `serializeWorkflowGraphLite`,
  `updateWorkflowGraphNode`, `addWorkflowGraphNode`, and
  `deleteWorkflowGraphNode`. P27.4 added Loom-native graph cards, edge chips,
  and a node properties panel to Workflow Studio. P27.5 regenerated the
  release.
- Phase 27 generated `loom-workflow-graph-ui-phase27` with `loom.exe`,
  `loom-daemon.exe`, and `loom-desktop.exe`. Formal release verification
  passed with `gitDirty=false` and 31 checksum entries. Browser UI validation
  confirmed `Graph view`, `Node properties`, `Apply node changes`, `Add node`,
  `step-2`, `2 nodes`, and YAML `needs: [prompt]`. Release smoke remained
  green across prior restored paths, including MCP marketplace, management
  CRUD, Python Art execution, cloud multipart, real OCR, workflow tool
  execution, workflow Art node execution, and workflow AHRP execution.
- Phase 28 P28.1 audited old ArtLoom `python_engine.rs` and
  `AddArtModal.tsx` for `read_python_file`, `read_art_json`,
  `check_art_json_nearby`, and `inferPortsFromPythonCode`. P28.2 added parity
  contract assertions plus a daemon RED test. P28.3 restored daemon
  `POST /v1/python-arts/source/read`, `read-art-json`, `check-art-json`, and
  `infer-ports` with `.py`/`art.json` and size safety boundaries. P28.4 added
  desktop API helpers, `pythonArtSource.ts`, and Registry UI for `Python source
  import`. P28.5 regenerated the release.
- Phase 28 generated `loom-python-source-import-phase28` with `loom.exe`,
  `loom-daemon.exe`, and `loom-desktop.exe`. Formal release verification
  passed with `gitDirty=false` and 31 checksum entries. Release smoke proved
  packaged source helper parity:
  `pythonArtSourceImport.nearbyArtJsonFound = true`,
  `pythonArtSourceImport.nearbyArtJsonLabel = "Source Import Fixture"`,
  `pythonArtSourceImport.inferredInputs = 2`,
  `pythonArtSourceImport.inferredOutputs = 2`, and
  `pythonArtSourceImport.scriptToolExecution = "source import saw release source helper"`.
- Phase 29 P29.1 final-audit sampled old ArtLoom `settings.rs`,
  `system_settings.rs`, and `ipc_service.rs`, then found that Loom's Hook
  Bridge advertised `get_settings`, `get_shortcuts`, `update_art_param`, and
  `sync_shortcuts` but still returned `Hook bridge method is not implemented`.
  P29.2 added a targeted RED test and parity contract assertions. P29.3
  restored the Hook Bridge handlers with legacy-compatible settings,
  shortcuts, art parameter update acknowledgement, and shortcut sync payloads.
  P29.4 regenerated the release and proved the methods through the packaged
  Hook Bridge WebSocket.
- Phase 29 generated `loom-hook-settings-phase29` with `loom.exe`,
  `loom-daemon.exe`, and `loom-desktop.exe`. Formal release verification
  passed with `gitHead=e0081f7412875cd31848081154530cfe9fdae0ca`,
  `gitDirty=false`, and 31 checksum entries. Release smoke proved packaged
  Hook Bridge settings compatibility:
  `hookBridgeSettings.settingsTheme = "system"`,
  `hookBridgeSettings.shortcutCount = 4`,
  `hookBridgeSettings.updatedArtId = "fixture-art"`, and
  `hookBridgeSettings.synced = true`.
- Phase 30 P30.1 found remaining Loom-specific `Neuro` product-prefix leaks in
  release `BUILD_INFO.txt`, default managed configuration storage, and desktop
  Cargo metadata. P30.2 changed the release heading to
  `Loom Windows release artifact`, moved the default managed configuration root
  to `%APPDATA%\Loom\configuration\apps` and `.runtime\loom\configuration\apps`,
  and changed Loom Cargo metadata authors to `Loom contributors`. P30.3
  regenerated the release and proved prior restored ArtLoom parity paths still
  pass.
- Phase 30 generated `loom-naming-cleanup-phase30` with `loom.exe`,
  `loom-daemon.exe`, and `loom-desktop.exe`. Formal release verification
  passed with `gitHead=b36a19dc13b78d4381c394b4fa66bc8a31ac4194`,
  `gitDirty=false`, and 31 checksum entries. The package zip has sha256
  `33087657aa8e2e1fc8f708b8f557ccea24218e165b7041e6b51796ef04688379`, and
  `BUILD_INFO.txt` starts with `Loom Windows release artifact`. Release smoke
  remained green across Hook Bridge settings, MCP marketplace, management CRUD,
  Python Art catalog/execution, Python source import, shared image AHRP, OCR,
  cloud multipart, and workflow Art/AHRP execution.
- Final ArtLoom parity matrix is recorded in
  `docs/loom/analysis/final-artloom-parity-matrix.md`. It maps every sampled
  old ArtLoom Tauri command group, old feature directory, old settings page,
  old protocol/runtime surface, packaged resource class, and Loom naming
  surface to current Loom evidence. The final register found no remaining
  product-critical ArtLoom parity gaps after Phase 30; remaining old surfaces
  are classified as behaviorally replaced, compatibility-only,
  intentionally replaced, or non-product-critical/obsolete.
- Phase 37 generated `loom-hook-live-release-smoke-phase37` with `loom.exe`,
  `loom-daemon.exe`, and `loom-desktop.exe`. Formal release verification passed
  with `gitHead=1ac1315b4e6a7738560ba7f57aaa9ea49f1f722f`,
  `gitDirty=false`, and 31 checksum entries. The package zip has sha256
  `dd9154f921f98c9b010b66c85991de37b8b3b2186bf2b04130df1b4b35046aa8`.
  Release smoke now proves packaged Hook live workflow persistence:
  `hookLiveWorkflow.workflowId = "hook-live"`,
  `hookLiveWorkflow.listName = "Hook 实时工作流"`,
  `hookLiveWorkflow.nodePersisted = true`,
  `hookLiveWorkflow.targetNodePersisted = true`, and
  `hookLiveWorkflow.edgePersisted = true`. The same phase fixed daemon JSON
  response headers to `application/json; charset=utf-8`.
- Phase 38 generated `loom-desktop-cn-polish-phase38` with `loom.exe`,
  `loom-daemon.exe`, and `loom-desktop.exe`. Formal release verification passed
  with `gitHead=bc569bd70be0e6ce097040319e4aea8a7a3cd736`,
  `gitDirty=false`, and 31 checksum entries. The package zip has sha256
  `b6c5d77189ef9d6e7932ab4546502f8ee499282ef5aa5fc6d029e436eb73daec`.
  Release smoke remained green and retained Hook live workflow evidence:
  `hookLiveWorkflow.workflowId = "hook-live"`,
  `hookLiveWorkflow.listName = "Hook 实时工作流"`, and
  `hookLiveWorkflow.edgePersisted = true`. The phase also added contract guards
  against user-visible daemon jargon, English MCP Marketplace tags, English
  Workflow Studio warnings, and raw English MCP category rendering. This is
  completed migration baseline/package evidence, not the current release
  candidate.
- Phase 39 generated `20260719-051119-dcdc94a8` with `loom.exe`,
  `loom-daemon.exe`, and `loom-desktop.exe`. The candidate was regenerated after
  the scoped provenance work was committed, and formal verification passed
  with `gitHead=dcdc94a899506955d602f1b19ab2eb5a19884a1f`, `gitDirty=true`,
  `sourceGitDirty=false`, approved source paths `Loom` and
  `scripts/build-release-exes.ps1`, and 31 checksum entries. The package ZIP has
  SHA-256
  `23b9a0d7f907d39a1698a44c4438f4056a79e0ffbc53aac74004622d8fd71d07`.
  Evidence is recorded in
  `Loom/target/runtime-smoke/20260719-053219-formal-release-verification/summary.json`,
  `output/smoke/runs/20260719-051738-Loom-36916-4a3cf592712546048aee2ac91cad8e4f/release-local-apps-20260719-051119-dcdc94a8-Loom-summary.json`,
  `Loom/target/runtime-smoke/20260719-051754-e039122a/summary.json`, and
  `Loom/target/runtime-smoke/20260719-053035-8365b4a0/summary.json`.
- Phase 40 is complete at 7/7 tasks. Source validation passed with 245 workspace
  tests, 94 daemon tests, 21 durable tests, 5 CLI tests, desktop
  typecheck/build/Tauri checks, and all three PowerShell contracts. The formal
  candidate is `release/Loom/20260719-082918-923fc5f8`, with
  `gitHead=923fc5f840cbce279496b2b43612a38a9d6e1c91`, `gitDirty=true`,
  `sourceGitDirty=false`, approved source paths `Loom` and
  `scripts/build-release-exes.ps1`, and ZIP SHA-256
  `07aa84be77c860144191c9f77a1c34c1a3d139005b1e7b0d79eafb7e631b542b`.
  Formal verifier evidence is in
  `Loom/target/runtime-smoke/20260719-083825-formal-release-verification/summary.json`;
  unified local smoke evidence is in
  `output/smoke/runs/20260719-083627-Loom-4884-c7dc780f02b94f979a440e46735683c5/release-local-apps-20260719-082918-923fc5f8-Loom-summary.json`;
  persistence evidence is in
  `Loom/target/runtime-smoke/20260719-083651-a92f4f37/summary.json`; Gateway
  evidence is in `Loom/target/runtime-smoke/20260719-083740-3f536340/summary.json`.
- Phase 41 is complete at 7/7 tasks. Source validation passed with 269 workspace
  tests, 110 daemon library tests, 8 daemon CLI contract tests, 21 durable
  tests, 5 CLI tests, desktop typecheck/build/Tauri checks, and all four
  PowerShell contracts. The formal candidate is
  `release/Loom/20260720-163055-8e27b864`, with
  `gitHead=8e27b864aa66f289728dcdbc61790a50d401e5b8`, `gitDirty=true`,
  `sourceGitDirty=false`, approved source paths `Loom` and
  `scripts/build-release-exes.ps1`, and ZIP SHA-256
  `d7ac699a6ae615a6a70a23b108507e91b026941c15cc48bfe08e1db4474acc39`.
  Formal verifier evidence is in
  `Loom/target/runtime-smoke/20260720-164102-formal-release-verification/summary.json`;
  independent artifact/evidence audit is in
  `Loom/target/runtime-smoke/20260720-164102-formal-release-verification/independent-audit.json`;
  unified local smoke evidence is in
  `output/smoke/runs/20260720-163643-Loom-18052-1df32550dddd4d58ae91ed5039d40543/release-local-apps-20260720-163055-8e27b864-Loom-summary.json`;
  persistence evidence is in
  `Loom/target/runtime-smoke/20260720-163704-e6c967e6/summary.json`; Gateway
  evidence is in `Loom/target/runtime-smoke/20260720-163735-7135275c/summary.json`;
  concurrency evidence is in
  `Loom/target/runtime-smoke/daemon-concurrency/20260720-163753-8ef8748a/summary.json`.
