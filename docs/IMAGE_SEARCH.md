# Image Search

## Packages, cross-platform entrypoints, and credentials

The `neuro.official/custom-image-search` Art uses the generic `mcp` framework and declares the independent `neuro.official/neuro-image-search` MCP server package as a dependency (`art-packages/samples/image-search` and `mcp-server-packages/image-search`). Otter neither launches the MCP server nor embeds image-search logic.

`mcp-server-packages/image-search/mcp.server.json` declares cross-platform stdio entrypoints so the same MCP package operates on both Windows and Linux server hosts without runtime `npm install` or `npx`:

- Windows default entry: `runtime/image-search-mcp.ps1`
- Linux platform entry (`entry.platforms.linux`): `runtime/image-search-mcp.mjs`

Configure **Brave API Key** from either the Art dependency status view or the MCP service page. Both write the server-scoped credential through `PUT /v1/mcp/servers/neuro-image-search/credentials`. The `mcp` framework injects the `brave_api_key` credential as `BRAVE_API_KEY` into the managed MCP child process, which exposes `brave_image_search` over stdio. Credential storage guarantees and platform boundaries are documented in [plugin permissions](plugin-permissions.md). Never place `BRAVE_API_KEY` in Art manifests, Art user settings, or Otter session snapshots.

## Inputs, parameters, and results

- The Art manifest declares `inputs: []`: image search is a generator node with no upstream screenshot input. Explicit empty capability input lists are authoritative and ignore stale graph edges.
- Parameters with `type: "secret"` or `data_type: "secret"` cannot be downgraded by `secret: false` and are excluded from Otter node state, session persistence, workflow exports, and result metadata.
- The Art exposes the search query and candidate count, converting MCP responses into candidate previews and a formal image output. Brave's `source` maps to the source-page URL and `url` maps to the downloadable image URL; if both strings are identical, the adapter omits the source-page field rather than inventing a page/Referer.
- Package-local PowerShell modules split candidate normalization (`image-search-candidates.ps1`), outbound address/redirect/size-bounded fetching (`image-search-download.ps1`), shared I/O helpers (`common.ps1`), and bitmap thumbnail generation (`image-runtime-bitmap.ps1`).
- The `mcp` framework validates tool arguments against `inputSchema` from `tools/list`, normalizing scalar types and enums, checking required fields, stripping undeclared fields when `additionalProperties` is `false`, and rejecting unsupported composed schemas (`$ref`, `allOf`, `anyOf`, `oneOf`) or nested object/array properties.

## Art settings and verification

General non-secret Art preferences persist in `<control-plane>/art-user-settings.json` and project into `metadata.artUserSettings` on registry reads; read failures clear stale projections rather than exposing raw file errors, and never substitute for MCP `CredentialStore` bindings.

Verification contracts:

- `scripts/tests/Test-LoomImageSearchMcpServer.ps1` validates the stdio MCP server contract.
- `scripts/tests/Test-LoomSampleArtInstallExecution.ps1` validates isolated installation, scoped credential injection, local fixture execution, and cleanup.
