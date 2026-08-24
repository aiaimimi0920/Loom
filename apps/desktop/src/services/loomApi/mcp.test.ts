// MCP credential, registry, and remote call transport contracts.
import assert from "node:assert/strict";
import test from "node:test";

import { callMcpTool, fetchMcpRegistry, updateMcpServerCredentials } from "../loomApi.ts";

test("updates credentials through the independent MCP server endpoint", async (context) => {
  const originalFetch = globalThis.fetch;
  context.after(() => {
    globalThis.fetch = originalFetch;
  });
  let requestBody: Record<string, unknown> | null = null;
  globalThis.fetch = (async (input: string | URL | Request, init?: RequestInit) => {
    const value = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    assert.equal(new URL(value).pathname, "/v1/mcp/servers/neuro-image-search/credentials");
    assert.equal(init?.method, "PUT");
    requestBody = JSON.parse(String(init?.body)) as Record<string, unknown>;
    return Response.json({
      server: {
        id: "neuro-image-search",
        name: "Neuro Image Search",
        transport: "stdio",
        command: "runtime/image-search-mcp.ps1",
        credentialRequired: true,
        credentialBound: true,
      },
    });
  }) as typeof fetch;

  const server = await updateMcpServerCredentials(
    "http://127.0.0.1:18765",
    "neuro-image-search",
    { brave_api_key: "write-only-fixture" },
  );
  assert.deepEqual(requestBody, {
    values: { brave_api_key: "write-only-fixture" },
    clear: [],
  });
  assert.equal(server.credentialBound, true);
  assert.equal("credentialBindings" in server, false);
});

test("official MCP Registry and remote tool helpers preserve standard transport fields", async (context) => {
  const originalFetch = globalThis.fetch;
  context.after(() => {
    globalThis.fetch = originalFetch;
  });
  const seen: Array<{ url: URL; body: unknown }> = [];
  globalThis.fetch = (async (input: string | URL | Request, init?: RequestInit) => {
    const value = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    seen.push({
      url: new URL(value),
      body: init?.body ? JSON.parse(String(init.body)) : undefined,
    });
    return new Response(JSON.stringify({ servers: [], metadata: {} }), {
      status: 200,
      headers: { "Content-Type": "application/json" },
    });
  }) as typeof fetch;

  await fetchMcpRegistry("http://127.0.0.1:18765", {
    search: "remote docs",
    limit: 80,
    updatedSince: "2026-08-01T00:00:00Z",
    includeDeleted: true,
    refresh: true,
  });
  await callMcpTool(
    "http://127.0.0.1:18765",
    {
      transport: "streamable-http",
      command: "",
      url: "https://example.test/mcp",
      headers: { Authorization: "Bearer secret" },
    },
    "search",
    { query: "loom" },
  );

  assert.equal(seen[0].url.pathname, "/v1/mcp/registry");
  assert.equal(seen[0].url.searchParams.get("version"), "latest");
  assert.equal(seen[0].url.searchParams.get("search"), "remote docs");
  assert.equal(seen[0].url.searchParams.get("updated_since"), "2026-08-01T00:00:00Z");
  assert.equal(seen[0].url.searchParams.get("include_deleted"), "true");
  assert.equal(seen[0].url.searchParams.get("refresh"), "true");
  assert.deepEqual(seen[1].body, {
    transport: "streamable-http",
    command: "",
    args: [],
    env: {},
    url: "https://example.test/mcp",
    headers: { Authorization: "Bearer secret" },
    toolName: "search",
    toolArgs: { query: "loom" },
  });
});
