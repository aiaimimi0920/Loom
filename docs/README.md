# Loom Documentation Index

Use the documents below as the authoritative reference for operating, integrating with, and extending Loom. Live Rust/TypeScript source, JSON Schemas, and contract test suites remain the ultimate source of truth.

## Core architecture and development

- [Root README](../README.md): overview, build editions (`desktop`, `web`, `api`), client/server modes, CLI, outbound/inbound MCP, AI Gateway relay, container deployment, and release smoke suites.
- [Architecture](ARCHITECTURE.md): system boundaries, workspace crates and apps, bounded HTTP request execution, package storage layout, and runtime subsystems.
- [Development manual](DEVELOPMENT.md) and [contributing guide](../CONTRIBUTING.md): effective-code-line thresholds (`150` target / `500` acceptable / `700` max soft exception), module splitting, security/lifetime/performance review, and CI quality reports.
- [Runtime security boundaries](RUNTIME_SECURITY.md): header-only HTTP upload admission, `loom_native_image` per-job resource caps, and Linux Bubblewrap + `pidfd` process-tree containment.
- [Dependency security](DEPENDENCY_SECURITY.md): lockfile inventory contract, OSV vulnerability scanning, exception rules, and incident response.

## Companion integration, store, and workflows

- [Otter and companion integration](OTTER_INTEGRATION.md): global Otter connection control, two-level Host -> App Sessions hierarchy, Otter canvas projection, multi-device workflow sync and history archive, intra-workflow hybrid terminal relay, Otter display language, and Companion App Settings Hub.
- [Workflow contract](WORKFLOW_CONTRACT.md): `loom.workflow.v1` graph synchronization, dynamic execution placement (`local`/`server`/`auto`), child Art dependencies, and SemVer compatibility.
- [Gateway integration](GATEWAY_INTEGRATION.md): global AI Gateway configuration (`/v1/ai/*`), LAN/remote origin rules, and durable `ai.chat.v1` invocation.
- [AssetLibrary, NAccount, and Unified Store](ASSET_LIBRARY_INTEGRATION.md): NAccount PKCE login (`/v1/naccount/*`), AssetLibrary catalog/download/install (`/v1/asset-library/*`), unified multi-platform catalog (`GET /v1/store/catalog`), forward-slash ZIP rule, and 3-tier Cloud Relay (`/v1/cloud-relay/*`).
- [Download engine](DOWNLOAD_ENGINE.md): Windows in-process `libaria2` (`loom_aria2.dll`), Linux portable fallback, loopback ticket relay, download-control API, and GPL corresponding-source bundle rules.
- [Neuro UI integration](NEURO_UI.md): `@neuro/ui` local linking, pinned clean-CI checkout layout, and UI-versus-controller ownership boundaries.

## Plugins, capabilities, and Tile Walls

- [Public protocols and JSON Schemas](../protocol/README.md): normative wire contracts (`loom.framework.v1`, `loom.hook.v1`, `loom.surface.v1`, `loom.capability.runtime.v1`, `loom.live.v1`, `loom.wall.v1`, Local Art v3, Device Sessions, LAN Connection, and QR Projection).
- [Plugin development](plugin-development.md): authoring Frameworks, Arts, and Capability Plugins with `loom-plugin`.
- [Plugin security](plugin-security.md), [plugin permissions](plugin-permissions.md), [signing and trust](plugin-signing-and-trust.md), and [historical plugin migration](plugin-migration.md).
- [OCR contract and golden dataset](OCR_CONTRACT.md) and [translation hybrid mode](TRANSLATION_HYBRID_MODE.md): `neuro.official/ocr` text/span/QR attachments, golden quality metrics, and `text-translation` `gateway`/`local`/`auto` broker modes.
- [Image Search](IMAGE_SEARCH.md) and [Stock Monitor](STOCK_MONITOR.md): sample MCP-backed Arts, cross-platform MCP server runtimes, scoped credentials, and freshness boundaries.
- [Tile Wall guide](TILE_WALL.md): multi-display terminal ownership, LAN wall routes, protocol index, and the 19 open physical multi-machine acceptance checks.

## Release and provenance

- [Release provenance](release-provenance.md): clean-source vs. `-DevelopmentOnly` builds, manifest/checksum verification, bounded smoke diagnostics, GitHub OIDC attestations, and draft-first publication.
- [GitHub release notes template](GITHUB_RELEASE_BODY.md): public release body used by `.github/workflows/release-tag.yml`.
