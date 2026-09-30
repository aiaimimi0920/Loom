# Image Search

## Packages and credentials

The `neuro.official/custom-image-search` Art uses the generic `mcp` framework
and declares the independent `neuro.official/neuro-image-search` MCP package
as a dependency. Sources live in `art-packages/samples/image-search` and
`mcp-server-packages/image-search`. Hook does not launch the server or contain
an image-search execution branch.

Configure **Brave API Key** through the Art dependency status or MCP service
page. The service credential endpoint is
`PUT /v1/mcp/servers/neuro-image-search/credentials`. The MCP package declares
`brave_api_key`, which the framework maps to `BRAVE_API_KEY` for its
`runtime/image-search-mcp.ps1` process. The server exposes `brave_image_search`
over stdio and calls Brave without runtime npm installation or `npx`.
Credential storage and platform limitations are described in
[plugin permissions](plugin-permissions.md).

The older Art-local server payload and read-only `art-mcp:` service projection
belong to retired implementations. Current service management uses the
independent MCP package and its credential lifecycle. Do not restore the old
projection or put the Brave key into Art defaults, Hook state or manifests.

## Inputs, parameters and results

The Art manifest declares `inputs: []`: image search is a generator and has no
screenshot input. A resolved capability's explicit input list is authoritative,
including an empty list; stale graph links do not create execution inputs.
Only an absent capability input declaration permits the current unit-port fallback.

Canonical `type: "secret"` or `data_type: "secret"` cannot be downgraded by
`secret: false`. Secret defaults and known secret parameters must be excluded
from Hook node state, session persistence, workflow snapshots and execution
parameters. Credentials are resolved inside Loom's protected execution boundary.

The manifest exposes the search query and candidate count. The Art converts MCP
results into image candidates and formal image output. Source-page URLs and
downloadable image URLs have distinct meanings: Brave's `source` supplies the
source page, while its `url` supplies the image URL. The adapter discards a
source-page URL equal to the image URL rather than inventing a page/Referer.

The MCP framework obtains the selected tool's `inputSchema` from `tools/list`.
It normalizes supported scalar types, aliases and enum values, checks required
arguments, and omits undeclared properties when `additionalProperties` is false.
Unsupported composed schemas such as `$ref`, `allOf`, `anyOf` and `oneOf`, and
nested object/array property types, are rejected by the current host. There is
no general complex-schema passthrough guarantee. Art/Hook control fields must
not become accidental tool arguments, and secret-bearing call arguments must
not be echoed into result metadata.

## Art settings and package updates

For general Art preferences, `<control-plane>/art-user-settings.json` is the
persistent authority. `metadata.artUserSettings` is a registry projection:
registry reads and saves remove the previous projection and reconstruct it
from settings for the qualified Art ID. Current registry lookup treats settings
read/validation failures as absent preferences, clearing stale projected values;
it does not promise the historical `art_settings_error` failure on every read.
This Art-preference mechanism does not replace MCP service credential storage.

When shipping a changed framework, update its version and reinstall dependent
bundled Arts so dependency locks resolve to the intended active framework.
Verify both a fresh control plane and upgrade from an existing installation.

## Verification boundary

`scripts/tests/Test-LoomImageSearchMcpServer.ps1` exercises the server contract;
`scripts/tests/Test-LoomSampleArtInstallExecution.ps1` exercises independent
package installation and execution with isolated fixtures. Fixture evidence
does not establish current public Brave availability or native Hook acceptance.
Use [release provenance](release-provenance.md) for exact-package joint gates.
Historical phase 72-74 provider runs, candidate hashes and old-process blockers
remain recoverable from Git tag `cleanup-base-20260928`.
