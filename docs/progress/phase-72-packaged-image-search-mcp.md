# Phase 72: Package-local image-search MCP

Date: 2026-08-14

## Status

Implementation, package contracts, Framework-host integration, Art installation,
image-result execution, a clean-provenance Loom release, and the full release
verifier are complete. The corresponding native Hook/Loom acceptance remains the
final release gate.

## Purpose

The canonical `neuro.official/custom-image-search` Art still existed, and the
generic `neuro.official/mcp` Framework package still existed, but the Art did not
carry its actual image-search MCP server. Its manifest launched:

```text
npx -y @brave/brave-search-mcp-server --transport stdio
```

That made the formal Art package depend on an unpinned npm download, an external
Node/npm/npx installation, and registry availability at execution time. The MCP
Framework host and the Art ZIP could both be present while the image-search node
was still unusable. This was the missing boundary reported after the Phase 71
framework refactor.

Phase 72 restores the capability without restoring the deleted desktop-specific
`mcpImageSearch` helper, per-Art installer wrappers, old ArtLoom routes, or any
other compatibility layer.

## Canonical architecture

The execution path is now:

```text
Hook generic Art node
  -> loom.hook.v1 art.execute
  -> Loom daemon / installed Art registry
  -> neuro.official/mcp Framework package
  -> loom.framework.v1 process host
  -> Art package runtime/image-search-mcp.ps1
  -> Brave image-search HTTP API
  -> stdio MCP structuredContent candidates
  -> Art package runtime/main.ps1
  -> typed image formal result and candidate metadata
  -> Hook generic candidate/result rendering
```

Ownership remains unchanged:

- Hook renders and interacts with a capability-driven Art node; it does not
  start the MCP server or load Art package code.
- Loom installs the Framework and Art packages, resolves the package-local
  command, injects the scoped credential, supervises the process, and owns the
  formal result.
- The generic MCP Framework has no `custom-image-search` Art-ID branch.
- Provider-specific server and result-adaptation code live inside the Art
  package, not inside Hook or the generic host.

## Implementation

### Package-local MCP server

`art-packages/samples/image-search/runtime/image-search-mcp.ps1` implements the
stdio JSON-RPC/MCP boundary used by `loom_mcp`:

- `initialize` with protocol `2024-11-05`;
- `notifications/initialized`;
- `tools/list` exposing exactly `brave_image_search`;
- `tools/call` with canonical `query` and `count` arguments;
- bounded query/count validation and rejection of unknown arguments;
- `BRAVE_API_KEY` credential consumption from the Framework execution context;
- Brave Image Search API requests with strict SafeSearch;
- an 8 MiB response limit and a 45-second HTTP timeout;
- canonical candidate fields: `imageUrl`, `thumbnailUrl`, `title`,
  `sourcePageUrl`, `width`, and `height`;
- JSON-RPC errors that do not echo the credential.

The generic MCP Framework host now also redacts non-empty credential values of
any length from MCP process errors; the previous four-character threshold could
expose unusually short secrets.

The production endpoint is:

```text
https://api.search.brave.com/res/v1/images/search
```

The `-Endpoint` parameter exists for isolated contract tests. The published
package manifest passes no override and therefore uses the production endpoint.

### Art package contract

`art-packages/samples/image-search/manifest.json` is now version `0.3.0` and
declares:

```json
{
  "transport": "stdio",
  "serverId": "neuro-image-search",
  "command": "runtime/image-search-mcp.ps1",
  "args": [],
  "toolName": "brave_image_search",
  "credentialEnv": {
    "BRAVE_API_KEY": "brave_api_key"
  }
}
```

The package is self-contained with respect to its MCP server. It no longer
contains or invokes `npx` and no longer downloads server code at execution time.
It still requires a user-provided Brave API key and network access to Brave and
the returned image URLs; those are current product inputs, not legacy behavior.

### Image formal-result runtime

The existing `runtime/main.ps1` continues to consume
`frameworkData.mcp.result`, preserve candidate metadata, download bounded image
content, and return the formal image result. Its PowerShell 5.1 Content-Length
handling was corrected so real HTTP image downloads do not dereference a
nonexistent nullable `.Value` property.

## Compatibility code intentionally not restored

Phase 72 does not restore:

- `apps/desktop/src/services/mcpImageSearch.ts`;
- an image-search-specific desktop quick-start dispatcher;
- `Install-LoomImageSearchArt.ps1` or any per-Art wrapper;
- an `npx` fallback when the package-local server is absent;
- a Hook-local image-search executor;
- Art-ID-specific host dispatch;
- old ArtLoom/AHRP routes or event aliases.

If the package-local server is missing or invalid, package validation or
execution fails closed.

## Verification

Fresh verification completed on 2026-08-14:

| Gate | Result |
| --- | --- |
| Package-local MCP direct JSON-RPC/API contract | passed |
| MCP Framework runtime-host tests | 6 passed |
| Package-local MCP through real Framework host | passed |
| Image-search `loom_tool_registry` normalization tests | 12 passed |
| Daemon Hook image-search bridge regression | 1 passed |
| Sample Art source/package contract | 6 packages passed |
| Curated Art runtime smoke | 6 execution cases passed |
| Installed Framework + Art execution smoke | 6 packages passed |
| Hook image-search node/candidate UI regressions | 4 files / 19 tests passed |

The installed execution smoke no longer replaces the MCP server with a fake MCP
process. It preserves `runtime/image-search-mcp.ps1`, changes only its test
endpoint to an isolated local fake Brave API, then proves:

- the real packaged server is launched by the installed MCP Framework;
- the Art-scoped secret reaches the server as `BRAVE_API_KEY`;
- the server calls the expected image-search endpoint with canonical arguments;
- the Art runtime downloads the selected image;
- the daemon returns a successful image data URL.

The rebuilt package used by these gates is:

```text
target/image-search-mcp-rebuild/arts/custom-image-search.zip
```

It contains:

```text
manifest.json
art.runtime.json
runtime/common.ps1
runtime/image-search-mcp.ps1
runtime/main.ps1
```

## Formal release

The clean-provenance release is:

```text
release/Loom/20260814-packaged-image-search-mcp-r27
```

Provenance and primary artifacts:

| Item | Value |
| --- | --- |
| source commit | `08fa8a29a000daa0abe677813c72925e0f0d0184` |
| `gitDirty` | `false` |
| `Loom.exe` | `5935dc9cc4e722bc56bc0b946abf948ca105f10e51b387a00dedfb7e61541cb5` |
| `runtime/loom-daemon.exe` | `8157b1086580eaca22b0a1764f367f32c966b956509937a7cebf6b3ec0b07293` |
| `packages/frameworks/mcp.zip` | `2dd4e06059c960fd6da5dd4ef0241d5a25aa349a138af64a1b2fcfc7c164230d` |
| `packages/arts/custom-image-search.zip` | `d6e45b2b6d4e5c4fe90d03eb322b5ac43185740c97aef52746baa59238d5e1c8` |
| image-search ZIP bytes | `10089` |
| desktop ZIP | `78bc746a8aa8085cf15409eae52dcdc9ebfd8a800c5a162ad2af49aafdb4b604` |
| CLI ZIP | `8d41e6fec8a59d7908a7a93ff63619d4c56cf59cb245f3a702e7ca794461eb12` |
| Plugin SDK ZIP | `50a20f849d305c7785e59050b24cef7c48bef4a541c64131965dac49f639f88b` |

`checksums.sha256` contains 49 entries. The formal image-search ZIP was opened
independently after packaging and contains the package-local MCP server. Its
packaged manifest reports version `0.3.0`, qualified ID
`neuro.official/custom-image-search`, Framework `mcp`, server ID
`neuro-image-search`, command `runtime/image-search-mcp.ps1`, zero launcher
arguments, and tool `brave_image_search`.

The release verifier completed with all groups passing:

```text
filesChecked = 49
smoke = passed
hookCanvasSmoke = passed
hookErrorPreviewSmoke = passed
frameworkArtStoreHookSmoke = passed
pluginBoundarySmoke = passed
surfacePrototypeSmoke = passed
authoredArtCreationSmoke = passed
```

## Remaining validation boundary and release gate

The following are intentionally not claimed by source/package tests alone:

1. A real Brave API call was not made because no user credential was read or
   exposed during automated verification. The local API fixture proves the HTTP
   and credential contract without consuming a real secret.
2. The 600-second native Hook/Loom acceptance must run only after the currently
   active user Hook/Loom instances have exited normally.

The latest safe preflight is:

```text
Hook/artifacts/runtime-performance/hook-loom-surface-candidate/
  20260814-124315-hook-loom-surface-c174f9e4814e/summary.json
```

It resolved R17/R27 and validated both hashes, but returned
`blocked_existing_hook`. It did not launch candidates or terminate the active
user Hook processes.
