import assert from "node:assert/strict";
import test from "node:test";

import { readLoomSnapshot } from "./loomApi.ts";

test("browser fallback keeps the daemon online when an optional module fails", async (context) => {
  const originalFetch = globalThis.fetch;
  context.after(() => {
    globalThis.fetch = originalFetch;
  });

  globalThis.fetch = (async (input: string | URL | Request) => {
    const value = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    const path = new URL(value).pathname;
    const response = (() => {
      switch (path) {
        case "/health":
          return [200, { status: "ok" }] as const;
        case "/status":
          return [200, { status: "ready" }] as const;
        case "/v1/capabilities":
          return [200, { capabilities: [] }] as const;
        case "/v1/mcp/servers":
          return [200, { servers: [] }] as const;
        case "/v1/tools":
          return [500, { error: { code: "tool_registry_error" } }] as const;
        case "/v1/python-arts":
          return [200, { arts: [] }] as const;
        case "/v1/workflows":
          return [200, { workflows: [] }] as const;
        case "/v1/hook-bridge/status":
          return [200, { running: false }] as const;
        default:
          throw new Error(`Unexpected snapshot path: ${path}`);
      }
    })();
    return new Response(JSON.stringify(response[1]), {
      status: response[0],
      headers: { "Content-Type": "application/json" },
    });
  }) as typeof fetch;

  const snapshot = await readLoomSnapshot("http://127.0.0.1:18765");

  assert.equal(snapshot.connectionState, "online");
  assert.deepEqual(snapshot.tools, []);
  assert.match(snapshot.error ?? "", /\/v1\/tools/);
});

test("browser fallback reports malformed optional module contracts as degraded", async (context) => {
  const originalFetch = globalThis.fetch;
  context.after(() => {
    globalThis.fetch = originalFetch;
  });

  globalThis.fetch = (async (input: string | URL | Request) => {
    const value = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    const path = new URL(value).pathname;
    const body = (() => {
      switch (path) {
        case "/health":
          return { status: "ok" };
        case "/status":
          return { status: "ready" };
        case "/v1/capabilities":
          return { capabilities: [] };
        case "/v1/mcp/servers":
          return { servers: [] };
        case "/v1/tools":
          return {};
        case "/v1/python-arts":
          return { arts: [] };
        case "/v1/workflows":
          return { workflows: [] };
        case "/v1/hook-bridge/status":
          return { running: false };
        default:
          throw new Error(`Unexpected snapshot path: ${path}`);
      }
    })();
    return new Response(JSON.stringify(body), {
      status: 200,
      headers: { "Content-Type": "application/json" },
    });
  }) as typeof fetch;

  const snapshot = await readLoomSnapshot("http://127.0.0.1:18766");

  assert.equal(snapshot.connectionState, "online");
  assert.deepEqual(snapshot.tools, []);
  assert.match(snapshot.error ?? "", /\/v1\/tools/);
});

test("browser fallback reports offline when the daemon disappears after core probes", async (context) => {
  const originalFetch = globalThis.fetch;
  context.after(() => {
    globalThis.fetch = originalFetch;
  });
  let healthRequests = 0;

  globalThis.fetch = (async (input: string | URL | Request) => {
    const value = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    const path = new URL(value).pathname;
    if (path === "/health") {
      healthRequests += 1;
      if (healthRequests === 1) {
        return new Response(JSON.stringify({ status: "ok" }), {
          status: 200,
          headers: { "Content-Type": "application/json" },
        });
      }
      throw new TypeError("fetch failed");
    }
    if (path === "/status") {
      return new Response(JSON.stringify({ status: "ready" }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      });
    }
    throw new TypeError("fetch failed");
  }) as typeof fetch;

  const snapshot = await readLoomSnapshot("http://127.0.0.1:18767");

  assert.equal(snapshot.connectionState, "offline");
  assert.equal(snapshot.health, null);
  assert.match(snapshot.error ?? "", /读取模块状态期间离线/);
});
