# Phase 26 MCP Marketplace Audit

## Scope

Phase 26 restores the old ArtLoom MCP marketplace and connection-test behavior
at the Loom layer.

Restored capabilities:

- daemon MCP Registry discovery API
- daemon MCP stdio connection test API
- daemon support for `LOOM_MCP_REGISTRY_ENDPOINT` so tests and release smoke can
  use a local registry fixture instead of the public network
- desktop curated MCP marketplace templates
- desktop mapping from official MCP Registry responses to installable stdio
  marketplace cards
- desktop install/update flow that saves MCP server configs through the daemon
- desktop install-and-test flow that validates a server through `tools/list`
- release smoke evidence for registry discovery and connection test in the
  generated package

Visible product names remain:

- `loom.exe`
- `loom-daemon.exe`
- `loom-desktop.exe`

The old ArtLoom source remains read-only:
`Z:\project\project\ArtNexus\ArtLoom`.

## Old source evidence

Reviewed old ArtLoom MCP marketplace sources:

- `Z:\project\project\ArtNexus\ArtLoom\src-tauri\src\lib.rs`
- `Z:\project\project\ArtNexus\ArtLoom\src-tauri\src\mcp_engine.rs`
- `Z:\project\project\ArtNexus\ArtLoom\src\pages\settings\MCPSettings.tsx`
- `Z:\project\project\ArtNexus\ArtLoom\src\features\mcp\marketplace.ts`

Old Tauri command registration in `src-tauri/src/lib.rs` included:

```text
mcp_engine::install_mcp_package
mcp_engine::check_mcp_package_installed
mcp_engine::fetch_mcp_registry
mcp_engine::test_mcp_connection
mcp_engine::call_mcp_tool
mcp_engine::get_mcp_servers
mcp_engine::save_mcp_server
mcp_engine::delete_mcp_server
```

Old `mcp_engine.rs` included:

- `fetch_mcp_registry(search, limit, cursor)` against
  `https://registry.modelcontextprotocol.io/v0/servers`
- `test_mcp_connection(command, args, env)` that starts a stdio MCP server,
  sends `initialize`, sends `notifications/initialized`, then sends
  `tools/list`
- `install_mcp_package(package_name)` and
  `check_mcp_package_installed(module_name)` for Python package manager checks
- persistent `get_mcp_servers`, `save_mcp_server`, and `delete_mcp_server`

Old `features/mcp/marketplace.ts` provided the product behavior that mattered:

- curated MCP templates
- category filtering
- official registry response mapping
- stdio package selection
- npm/pypi/oci command construction
- required env detection
- install config generation
- health tags from configured/tested state

Old `MCPSettings.tsx` exposed:

- `Configured` server tab
- `Marketplace` tab
- search/category filters
- `Refresh Registry`
- `Load More`
- `Install`
- `Install & Test`
- configured server enable/delete/configure actions
- connection test evidence with discovered tool count

## Loom state before Phase 26

Before this phase, Loom had the lower-level MCP server CRUD from Phase 25:

```text
GET /v1/mcp/servers
PUT /v1/mcp/servers/{serverId}
DELETE /v1/mcp/servers/{serverId}
```

It also had MCP-backed tool execution and old Hook bridge MCP execution paths
from earlier phases.

Missing relative to old ArtLoom:

- no daemon `GET /v1/mcp/registry`
- no daemon `POST /v1/mcp/test`
- no release smoke proof for registry discovery or `tools/list` connection
  testing
- no desktop curated MCP marketplace templates
- no desktop official registry mapping
- no desktop `Install server` or `Install & Test` flow
- no desktop marketplace categories/search/source status

## Phase 26 implementation

### Daemon MCP Registry and connection test

Updated:

```text
Loom/apps/daemon/src/lib.rs
Loom/apps/daemon/Cargo.toml
Loom/Cargo.lock
```

New daemon help/API entries:

```text
GET  /v1/mcp/registry
POST /v1/mcp/test
```

New environment override:

```text
LOOM_MCP_REGISTRY_ENDPOINT
```

`GET /v1/mcp/registry`:

- parses `search`, `limit`, and `cursor`
- clamps `limit` to the old ArtLoom-compatible range `1..=100`
- percent-encodes query values
- fetches the configured registry endpoint with a Loom user-agent
- returns the registry JSON unchanged so desktop mapping stays close to the old
  UI contract

`POST /v1/mcp/test`:

- accepts a `McpServerConfig`
- starts the configured stdio MCP server with `StdioMcpClient`
- sends `initialize`
- sends `notifications/initialized`
- sends `tools/list`
- returns:

```json
{
  "success": true,
  "tools": [],
  "server_info": {},
  "serverInfo": {}
}
```

Both snake-case and camel-case server info keys are returned to keep old-style
and current-style consumers simple.

Updated `Loom/crates/loom_mcp/src/lib.rs` so saved MCP server configs preserve
the old ArtLoom `description` field.

### Desktop marketplace service

Added:

```text
Loom/apps/desktop/src/services/mcpMarketplace.ts
```

Restored old marketplace behavior in Loom-native TypeScript:

- `MCP_MARKET_CATEGORIES`
- `MCP_MARKET_SERVERS`
- `mapRegistryResponseToMarketplace`
- `mergeRegistryAndCuratedMarketplace`
- `buildMarketplaceServerConfig`
- `getMarketplaceHealth`

The service maps official registry entries into stdio install configs:

- npm packages -> `npx`
- pypi packages -> `uvx`
- oci packages -> `docker run -i --rm`

It preserves required env detection and manual package-argument warnings.

### Desktop daemon API helpers

Updated:

```text
Loom/apps/desktop/src/services/loomApi.ts
```

Added:

```text
fetchMcpRegistry
testMcpConnection
saveMcpServer
LoomMcpTestResult
```

The helpers use the existing Tauri HTTP bridge where available and browser
`fetch` fallback in local previews.

### Desktop MCP UI

Updated:

```text
Loom/apps/desktop/src/App.tsx
Loom/apps/desktop/src/styles.css
```

The MCP panel now has:

- search box
- category filter
- `Refresh Registry`
- `Load More`
- source status showing registry vs curated fallback
- `Configured servers`
- configured server `Test connection`
- configured server `Delete server`
- `MCP Marketplace`
- marketplace cards
- `Install server`
- `Install & Test`
- health tags for configured/enabled/manual-config/key/test state

This is intentionally not an AntD clone of old `MCPSettings.tsx`; it restores
the old behavior inside the existing Loom desktop glass UI.

### Release smoke

Updated:

```text
scripts/smoke-release-local-apps.ps1
scripts/tests/test-loom-artloom-parity-contract.ps1
```

Release smoke now starts a local MCP Registry fixture, sets
`LOOM_MCP_REGISTRY_ENDPOINT`, and proves:

```text
GET /v1/mcp/registry?search=fixture
POST /v1/mcp/test
```

The existing release MCP fixture now responds to `tools/list`, so the packaged
daemon can prove stdio connection testing without relying on external package
installers or network services.

Smoke summary evidence:

```json
{
  "mcpMarketplace": {
    "registryServerCount": 1,
    "registryServerName": "io.modelcontextprotocol/fixture",
    "connectionTestSuccess": true,
    "connectionTestTool": "echo",
    "connectionTestServer": "release-fixture"
  }
}
```

## Validation

Fresh local validation:

```text
cargo test --manifest-path Loom/Cargo.toml -p loom_mcp --offline -- --nocapture
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon daemon_reads_and_writes_mcp_servers --offline -- --nocapture --test-threads=1
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon daemon_exposes_mcp_registry_and_connection_test_contracts --offline -- --nocapture --test-threads=1
cargo check --manifest-path Loom/Cargo.toml -p loom-daemon --offline
cargo check --manifest-path Loom/apps/desktop/src-tauri/Cargo.toml --offline
npm --prefix Loom/apps/desktop run typecheck
npm --prefix Loom/apps/desktop run build
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1
cargo fmt --manifest-path Loom/Cargo.toml --all -- --check
rg -n "NeuroLoom|Neuro" Loom/apps/desktop/src Loom/apps/desktop/src-tauri/src/lib.rs Loom/resources/python scripts/tests/test-loom-artloom-parity-contract.ps1
```

All commands passed. The prefix regression check returned no matches.

Generated release:

```text
release\Loom\loom-mcp-marketplace-phase26
```

Formal verification:

```text
status = passed
gitHead = 3cbfa07a6bcc049e61c2ba6b1770bbba65c04b35
gitDirty = false
checksumEntries = 31
```

Package:

```text
packages\Loom-loom-mcp-marketplace-phase26-windows-x64.zip
size = 50068556 bytes
sha256 = e5a897d202cd35fd24f520690d0458a2448c8973d1942d937554f39167bc6747
```

Release smoke:

```text
output\smoke\runs\20260612-211033-Loom-20760-e6381cbc777949ee9e680cbab197d856\release-local-apps-loom-mcp-marketplace-phase26-Loom-summary.json
output\smoke\latest\release-local-apps-loom-mcp-marketplace-phase26-Loom-summary.json
```

Smoke evidence includes:

```text
controlPlane.mcpMarketplace.registryServerCount = 1
controlPlane.mcpMarketplace.registryServerName = "io.modelcontextprotocol/fixture"
controlPlane.mcpMarketplace.connectionTestSuccess = true
controlPlane.mcpMarketplace.connectionTestTool = "echo"
controlPlane.mcpMarketplace.connectionTestServer = "release-fixture"
controlPlane.managementCrud.mcpServerDeleted = true
controlPlane.pythonArtCatalog.artId = "loom_echo"
controlPlane.pythonArtToolExecution = "python art saw release installed python art"
controlPlane.pythonToolExecution.packagedPython = true
controlPlane.cloudMultipartArtNode.multipartSeen = true
controlPlane.realOcrImage.fullTextLength = 63
```

## Non-goals

Phase 26 does not restore every old package-manager side effect.

Intentional boundaries:

- No direct `pip install` clone of old `install_mcp_package`; marketplace
  install saves stdio server config, while `npx`, `uvx`, or `docker` resolve
  packages when the server runs.
- No AntD UI clone; the behavior is restored in the Loom desktop UI system.
- No full old visual graph editor parity; that remains a separate final-audit
  decision.
- No Python source-edit/import UI beyond the installed catalog restored in
  Phase 24.
