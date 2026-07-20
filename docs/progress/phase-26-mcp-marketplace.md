# Phase 26: MCP Marketplace Parity

## Goal

Restore old ArtLoom MCP Registry marketplace, curated server templates,
install/update, and connection-test behavior in Loom, including desktop UI and
packaged release smoke proof.

## Tasks

- [x] P26.1 Source audit and parity boundary
  - Acceptance: source-backed audit identifies old MCP registry fetch,
    marketplace mapping, install/update, connection testing, current Loom gaps,
    and the Phase 26 recovery boundary.
  - Evidence:
    - `docs/loom/analysis/phase-26-mcp-marketplace-audit.md` records old
      `mcp_engine.rs`, `MCPSettings.tsx`, `features/mcp/marketplace.ts`,
      current Loom gaps, implementation design, release evidence, and
      non-goals.

- [x] P26.2 Daemon registry and connection-test APIs
  - Acceptance: Loom daemon exposes old-like MCP Registry discovery and stdio
    server testing.
  - Evidence:
    - `Loom/apps/daemon/src/lib.rs` exposes:
      - `GET /v1/mcp/registry`
      - `POST /v1/mcp/test`
    - `LOOM_MCP_REGISTRY_ENDPOINT` supports local fixture override.
    - `Loom/apps/daemon/Cargo.toml` adds `reqwest` for registry HTTP fetch.
    - `Loom/crates/loom_mcp/src/lib.rs` preserves MCP server `description`.
    - `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon daemon_exposes_mcp_registry_and_connection_test_contracts --offline -- --nocapture --test-threads=1`
      passed with 1 test.
    - `cargo check --manifest-path Loom/Cargo.toml -p loom-daemon --offline`
      passed.

- [x] P26.3 Desktop marketplace mapping and API helpers
  - Acceptance: desktop can map official registry responses, provide curated
    templates, save marketplace servers, and call daemon connection tests.
  - Evidence:
    - `Loom/apps/desktop/src/services/mcpMarketplace.ts` adds
      `MCP_MARKET_SERVERS`, `MCP_MARKET_CATEGORIES`,
      `mapRegistryResponseToMarketplace`,
      `mergeRegistryAndCuratedMarketplace`, `buildMarketplaceServerConfig`,
      and `getMarketplaceHealth`.
    - `Loom/apps/desktop/src/services/loomApi.ts` adds `fetchMcpRegistry`,
      `testMcpConnection`, `saveMcpServer`, and `LoomMcpTestResult`.
    - `npm --prefix Loom/apps/desktop run typecheck` passed.

- [x] P26.4 Desktop MCP marketplace UI
  - Acceptance: desktop MCP panel exposes configured servers and marketplace
    install/test flows.
  - Evidence:
    - `Loom/apps/desktop/src/App.tsx` adds:
      - `Configured servers`
      - `MCP Marketplace`
      - `Refresh Registry`
      - category/search filtering
      - `Install server`
      - `Install & Test`
      - configured server `Test connection`
      - existing `Delete server`
    - `Loom/apps/desktop/src/styles.css` adds marketplace toolbar/tag layout.
    - `npm --prefix Loom/apps/desktop run build` passed.

- [x] P26.5 Contract, release, and smoke
  - Acceptance: parity contract passes; regenerated release proves registry
    discovery, connection test, and all previously restored runtime paths.
  - Evidence:
    - `scripts/tests/test-loom-artloom-parity-contract.ps1` asserts daemon
      registry/test routes, release-smoke evidence, desktop API helpers,
      marketplace mapping service, and desktop UI labels.
    - `scripts/smoke-release-local-apps.ps1` exercises:
      - `GET /v1/mcp/registry?search=fixture`
      - `POST /v1/mcp/test`
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1`
      passed.
    - `cargo fmt --manifest-path Loom/Cargo.toml --all -- --check` passed.
    - `rg -n "NeuroLoom|Neuro" Loom/apps/desktop/src Loom/apps/desktop/src-tauri/src/lib.rs Loom/resources/python scripts/tests/test-loom-artloom-parity-contract.ps1`
      returned no matches.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release-exes.ps1 -Apps Loom -VersionId loom-mcp-marketplace-phase26 -Force`
      generated `release\Loom\loom-mcp-marketplace-phase26` with `loom.exe`,
      `loom-daemon.exe`, and `loom-desktop.exe`.
    - `packages\Loom-loom-mcp-marketplace-phase26-windows-x64.zip`
      was generated with size `50068556` bytes.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-release.ps1 -VersionId loom-mcp-marketplace-phase26 -Apps Loom`
      passed formal verification with
      `gitHead = 3cbfa07a6bcc049e61c2ba6b1770bbba65c04b35`,
      `gitDirty = false`, and 31 checksum entries.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\smoke-release-local-apps.ps1 -VersionId loom-mcp-marketplace-phase26 -Apps Loom`
      passed. The smoke summary is:
      `output\smoke\runs\20260612-211033-Loom-20760-e6381cbc777949ee9e680cbab197d856\release-local-apps-loom-mcp-marketplace-phase26-Loom-summary.json`.
    - Smoke evidence includes:
      - `mcpMarketplace.registryServerCount = 1`
      - `mcpMarketplace.registryServerName = "io.modelcontextprotocol/fixture"`
      - `mcpMarketplace.connectionTestSuccess = true`
      - `mcpMarketplace.connectionTestTool = "echo"`
      - `mcpMarketplace.connectionTestServer = "release-fixture"`
      - `managementCrud.mcpServerDeleted = true`
      - `pythonArtCatalog.artId = "loom_echo"`
      - `pythonArtToolExecution = "python art saw release installed python art"`
      - `pythonToolExecution.packagedPython = true`
      - `cloudMultipartArtNode.multipartSeen = true`
      - `realOcrImage.fullTextLength = 63`

## Notes

- Old source reference is read-only:
  `Z:\project\project\ArtNexus\ArtLoom`.
- Phase 26 restores marketplace behavior, not the exact old AntD UI.
- Marketplace install saves stdio server configuration. It does not run old
  Python `pip install`; `npx`, `uvx`, and `docker` remain the execution-time
  package mechanisms for their respective server templates.
- Remaining likely follow-up decisions are the full old ReactFlow visual graph
  editor and old Python source-edit/import surfaces beyond the installed Python
  Art catalog restored in Phase 24.
