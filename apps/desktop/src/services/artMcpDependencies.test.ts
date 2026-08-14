import assert from "node:assert/strict";
import test from "node:test";

import { artMcpDependencyIds, resolveArtMcpDependencies } from "./artMcpDependencies.ts";
import type { LoomMcpServer, LoomToolDefinition } from "./loomApi.ts";

const imageSearchArt: LoomToolDefinition = {
  id: "custom-image-search",
  name: "Image Search",
  metadata: {
    dependencies: {
      mcpServers: [
        { id: "neuro.official/neuro-image-search", version: "^0.1" },
        { id: "neuro.official/neuro-image-search", version: "^0.1" },
      ],
    },
  },
};

const imageSearchServer = (overrides: Partial<LoomMcpServer> = {}): LoomMcpServer => ({
  id: "neuro-image-search",
  name: "Neuro Image Search",
  transport: "stdio",
  command: "runtime/image-search-mcp.ps1",
  enabled: true,
  package: {
    qualifiedId: "neuro.official/neuro-image-search",
    publisherId: "neuro.official",
    version: "0.1.0",
    digest: "fixture-digest",
    packageDir: "fixture-package",
  },
  credentialRequired: true,
  credentialBound: false,
  credentialRequirements: [{ id: "brave_api_key", label: "Brave API Key", required: true }],
  ...overrides,
});

test("resolves a package-qualified Art dependency to the independent MCP server", () => {
  assert.deepEqual(artMcpDependencyIds(imageSearchArt), ["neuro.official/neuro-image-search"]);
  const [state] = resolveArtMcpDependencies(imageSearchArt, [imageSearchServer()]);
  assert.equal(state.server?.id, "neuro-image-search");
  assert.equal(state.status, "credentials_required");
});

test("reports the independent MCP dependency lifecycle without Art credential fallbacks", () => {
  assert.equal(resolveArtMcpDependencies(imageSearchArt, [
    imageSearchServer({ credentialBound: true }),
  ])[0].status, "ready");
  assert.equal(resolveArtMcpDependencies(imageSearchArt, [
    imageSearchServer({ enabled: false }),
  ])[0].status, "disabled");
  assert.equal(resolveArtMcpDependencies(imageSearchArt, [])[0].status, "missing");
});
