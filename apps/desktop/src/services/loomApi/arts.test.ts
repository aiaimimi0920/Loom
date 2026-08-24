// Art management, uninstall, catalog, installation, and publication contracts.
import assert from "node:assert/strict";
import test from "node:test";

import {
  autoUpdateArts,
  fetchArtStoreCatalog,
  getArtManagement,
  installArtFromStore,
  publishArt,
  saveArtManagementSettings,
  uninstallArtPackage,
  updateArtToVersion,
} from "../loomApi.ts";

test("Art management helpers use dedicated settings and version routes", async (context) => {
  const originalFetch = globalThis.fetch;
  context.after(() => {
    globalThis.fetch = originalFetch;
  });
  const seen: Array<{ method: string; pathname: string; body: string }> = [];
  const management = {
    artId: "neuro.official/sample",
    name: "Sample",
    description: "",
    locallyAuthored: false,
    canEditIdentity: false,
    currentVersion: "1.0.0",
    highestVersion: "1.1.0",
    autoUpdate: true,
    installedVersions: [],
    availableVersions: ["1.0.0", "1.1.0"],
    parameters: [],
    defaults: {},
    valueBindings: {},
    credentialBindings: {},
    availableCredentials: [],
    updateAvailable: true,
  };
  globalThis.fetch = (async (input: string | URL | Request, init?: RequestInit) => {
    const value = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    const url = new URL(value);
    seen.push({
      method: String(init?.method ?? "GET").toUpperCase(),
      pathname: url.pathname,
      body: String(init?.body ?? ""),
    });
    return new Response(JSON.stringify(url.pathname === "/v1/arts/auto-update"
      ? { updated: [], errors: [] }
      : management), {
      status: 200,
      headers: { "Content-Type": "application/json" },
    });
  }) as typeof fetch;

  const artId = "neuro.official/sample";
  await getArtManagement("http://127.0.0.1:18772", artId);
  await saveArtManagementSettings("http://127.0.0.1:18772", artId, {
    autoUpdate: false,
    defaults: { strength: 0.8 },
    valueBindings: { count: "image_search_count" },
    credentialBindings: { apiKey: "cloudflare_key" },
    secretValues: { privateToken: "write-only-value" },
  });
  await updateArtToVersion("http://127.0.0.1:18772", artId, "1.1.0");
  await autoUpdateArts("http://127.0.0.1:18772");

  assert.deepEqual(seen.map(({ method, pathname }) => `${method} ${pathname}`), [
    "GET /v1/arts/neuro.official%2Fsample/management",
    "PUT /v1/arts/neuro.official%2Fsample/settings",
    "POST /v1/arts/neuro.official%2Fsample/update",
    "POST /v1/arts/auto-update",
  ]);
  assert.deepEqual(JSON.parse(seen[1]?.body ?? "{}"), {
    autoUpdate: false,
    defaults: { strength: 0.8 },
    valueBindings: { count: "image_search_count" },
    credentialBindings: { apiKey: "cloudflare_key" },
    secretValues: { privateToken: "write-only-value" },
  });
  assert.equal("privateToken" in JSON.parse(seen[1]?.body ?? "{}").defaults, false);
  assert.equal(seen[2]?.body, JSON.stringify({ version: "1.1.0" }));
});


test("Art package uninstall uses the publisher-qualified package route", async (context) => {
  const originalFetch = globalThis.fetch;
  context.after(() => {
    globalThis.fetch = originalFetch;
  });

  let seenMethod = "";
  let seenPath = "";
  let seenBody = "";
  globalThis.fetch = (async (input: string | URL | Request, init?: RequestInit) => {
    const value = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    seenMethod = String(init?.method ?? "GET").toUpperCase();
    seenPath = new URL(value).pathname;
    seenBody = String(init?.body ?? "");
    return new Response(JSON.stringify({ artId: "publisher.test/sample-art", uninstalled: true }), {
      status: 200,
      headers: { "Content-Type": "application/json" },
    });
  }) as typeof fetch;

  await uninstallArtPackage("http://127.0.0.1:18770", "publisher.test/sample-art", {
    removeUnusedMcpServers: true,
  });

  assert.equal(seenMethod, "POST");
  assert.equal(seenPath, "/v1/arts/publisher.test%2Fsample-art/uninstall");
  assert.match(seenBody, /"removeUnusedMcpServers":true/);
});


test("art store helpers preserve certification and call install and publish routes", async (context) => {
  const originalFetch = globalThis.fetch;
  context.after(() => {
    globalThis.fetch = originalFetch;
  });

  const seen: Array<{ method: string; pathWithQuery: string; body: string }> = [];

  globalThis.fetch = (async (input: string | URL | Request, init?: RequestInit) => {
    const value = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    const url = new URL(value);
    const method = String(init?.method ?? "GET").toUpperCase();
    const body = String(init?.body ?? "");
    seen.push({ method, pathWithQuery: `${url.pathname}${url.search}`, body });

    if (method === "GET" && url.pathname === "/v1/arts/store/catalog") {
      return new Response(JSON.stringify({
        arts: [
          {
            id: "loom_echo",
            name: "Loom Echo",
            description: "Fixture art",
            framework: "process",
            official: true,
          },
        ],
      }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      });
    }

    if (method === "POST" && url.pathname === "/v1/arts/store/install") {
      return new Response(JSON.stringify({
        reports: [{ toolId: "loom_echo", framework: "process" }],
      }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      });
    }

    if (method === "POST" && url.pathname === "/v1/arts/store/publish") {
      return new Response(JSON.stringify({
        artId: "local-art",
        globalId: "NA40000000001",
        sha256: "a".repeat(64),
        published: true,
      }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      });
    }

    throw new Error(`Unexpected art store path: ${method} ${url.pathname}${url.search}`);
  }) as typeof fetch;

  const catalog = await fetchArtStoreCatalog("http://127.0.0.1:18772");
  await installArtFromStore(
    "http://127.0.0.1:18772",
    "loom_echo",
  );
  const published = await publishArt(
    "http://127.0.0.1:18772",
    "local-art",
  );

  assert.equal(catalog.length, 1);
  assert.equal(catalog[0]?.framework, "process");
  assert.equal(catalog[0]?.official, true);
  assert.equal(published.globalId, "NA40000000001");
  assert.deepEqual(
    seen.map((entry) => `${entry.method} ${entry.pathWithQuery}`),
    [
      "GET /v1/arts/store/catalog",
      "POST /v1/arts/store/install",
      "POST /v1/arts/store/publish",
    ],
  );
  assert.equal(
    seen[1]?.body,
    JSON.stringify({ artId: "loom_echo" }),
  );
  assert.equal(
    seen[2]?.body,
    JSON.stringify({ artId: "local-art" }),
  );
});
