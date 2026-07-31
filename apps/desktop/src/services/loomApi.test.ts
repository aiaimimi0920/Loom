import assert from "node:assert/strict";
import test from "node:test";

import {
  artLoomExecuteArtNodeErrorMessage,
  fetchArtStoreCatalog,
  installArtFromStore,
  installFramework,
  listFrameworks,
  readLoomSnapshot,
  saveHookCanvasWorkflow,
  updateArtLoomWorkflowNode,
  uninstallFramework,
  waitForLoomOnline,
  type ConnectionState,
  type LoomSnapshot,
} from "./loomApi.ts";

function readinessSnapshot(connectionState: ConnectionState): LoomSnapshot {
  return {
    baseUrl: "http://127.0.0.1:18768",
    connectionState,
    checkedAt: new Date().toISOString(),
    health: connectionState === "online" ? { status: "ok" } : null,
    status: connectionState === "online" ? { status: "ready" } : null,
    capabilities: [],
    mcpServers: [],
    tools: [],
    pythonArts: [],
    workflows: [],
    hookBridge: null,
    settings: {
      root: "http://127.0.0.1:18768/settings",
      tea: "http://127.0.0.1:18768/settings/tea",
      hook: "http://127.0.0.1:18768/settings/hook",
      talk: "http://127.0.0.1:18768/settings/talk",
    },
    error: connectionState === "online" ? null : "offline",
  };
}

test("waits through transient offline snapshots until the daemon is online", async () => {
  const states: ConnectionState[] = ["offline", "offline", "online"];
  const sleeps: number[] = [];
  let elapsed = 0;

  const snapshot = await waitForLoomOnline(
    async () => readinessSnapshot(states.shift() ?? "online"),
    {
      timeoutMs: 1_000,
      intervalMs: 100,
      sleep: async (delayMs) => {
        sleeps.push(delayMs);
        elapsed += delayMs;
      },
      now: () => elapsed,
    },
  );

  assert.ok(snapshot);
  assert.equal(snapshot.connectionState, "online");
  assert.deepEqual(sleeps, [100, 100]);
});

test("readiness timeout bounds a stalled snapshot attempt", async () => {
  const startedAt = Date.now();
  let attemptTimeouts = 0;

  const snapshot = await waitForLoomOnline(
    async () => await new Promise<LoomSnapshot>((resolve) => {
      setTimeout(() => resolve(readinessSnapshot("offline")), 200);
    }),
    {
      timeoutMs: 10,
      attemptTimeoutMs: 5,
      intervalMs: 1,
      onAttemptTimeout: () => {
        attemptTimeouts += 1;
      },
    },
  );

  assert.equal(snapshot, null);
  assert.ok(Date.now() - startedAt < 150);
  assert.ok(attemptTimeouts >= 1);
});

test("readiness timeout aborts only the stalled attempt", async () => {
  let aborted = false;

  await waitForLoomOnline(
    async (signal) => await new Promise<LoomSnapshot>((resolve) => {
      signal.addEventListener("abort", () => {
        aborted = true;
      }, { once: true });
      setTimeout(() => resolve(readinessSnapshot("offline")), 200);
    }),
    {
      timeoutMs: 10,
      attemptTimeoutMs: 5,
      intervalMs: 1,
    },
  );

  assert.equal(aborted, true);
});

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

test("saveHookCanvasWorkflow sends the selected node export request to the daemon", async (context) => {
  const originalFetch = globalThis.fetch;
  context.after(() => {
    globalThis.fetch = originalFetch;
  });

  let seenPath = "";
  let seenBody = "";

  globalThis.fetch = (async (input: string | URL | Request, init?: RequestInit) => {
    const value = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    seenPath = new URL(value).pathname;
    seenBody = String(init?.body ?? "");
    return new Response(JSON.stringify({ workflow: { id: "wf-hook", name: "Hook Export" } }), {
      status: 200,
      headers: { "Content-Type": "application/json" },
    });
  }) as typeof fetch;

  await saveHookCanvasWorkflow("http://127.0.0.1:18770", {
    workflowId: "wf-hook",
    selectedNodeId: "capture",
    workflowName: "Hook Export",
  });

  assert.equal(seenPath, "/v1/hook-bridge/canvas/workflows/wf-hook");
  assert.match(seenBody, /"selectedNodeId":"capture"/);
  assert.match(seenBody, /"workflowName":"Hook Export"/);
});

test("updateArtLoomWorkflowNode sends the compat node-param persistence request", async (context) => {
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
    return new Response(JSON.stringify({ type: "success" }), {
      status: 200,
      headers: { "Content-Type": "application/json" },
    });
  }) as typeof fetch;

  await updateArtLoomWorkflowNode("http://127.0.0.1:18770", {
    workflowId: "hook-live",
    nodeId: "image-search-node",
    param: "result_index",
    value: 1,
  });

  assert.equal(seenMethod, "POST");
  assert.equal(seenPath, "/v1/artloom-compat/ipc/update-workflow-node");
  assert.match(seenBody, /"workflowId":"hook-live"/);
  assert.match(seenBody, /"nodeId":"image-search-node"/);
  assert.match(seenBody, /"param":"result_index"/);
  assert.match(seenBody, /"value":1/);
});

test("artLoomExecuteArtNodeErrorMessage surfaces compat payload details", () => {
  assert.equal(
    artLoomExecuteArtNodeErrorMessage({
      type: "error",
      data: { message: "MCP tool response contained no usable image data" },
    }),
    "MCP tool response contained no usable image data",
  );
  assert.equal(
    artLoomExecuteArtNodeErrorMessage({
      type: "error",
      data: { error: "额度不足（HTTP 402）" },
    }),
    "额度不足（HTTP 402）",
  );
});

test("artLoomExecuteArtNodeErrorMessage ignores success responses", () => {
  assert.equal(
    artLoomExecuteArtNodeErrorMessage({
      type: "success",
      data: { message: "should not surface" },
    }),
    null,
  );
});

test("framework helpers call the framework management routes", async (context) => {
  const originalFetch = globalThis.fetch;
  context.after(() => {
    globalThis.fetch = originalFetch;
  });

  const seen: Array<{ method: string; path: string; body: string }> = [];

  globalThis.fetch = (async (input: string | URL | Request, init?: RequestInit) => {
    const value = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    const path = new URL(value).pathname;
    const method = String(init?.method ?? "GET").toUpperCase();
    const body = String(init?.body ?? "");
    seen.push({ method, path, body });

    if (method === "GET" && path === "/v1/frameworks") {
      return new Response(JSON.stringify({
        frameworks: [
          {
            id: "python_art",
            name: "Python Art 框架",
            description: "运行 Python Art，需要 Python 运行时。",
            installed: false,
            ready: false,
            readyDetail: "未安装",
          },
        ],
      }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      });
    }

    if (method === "POST" && path === "/v1/frameworks/python_art/install") {
      return new Response(JSON.stringify({
        framework: {
          id: "python_art",
          name: "Python Art 框架",
          description: "运行 Python Art，需要 Python 运行时。",
          installed: true,
          ready: true,
          readyDetail: "已安装运行时：C:\\runtime\\python.exe",
        },
      }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      });
    }

    if (method === "POST" && path === "/v1/frameworks/python_art/uninstall") {
      return new Response(JSON.stringify({
        framework: {
          id: "python_art",
          name: "Python Art 框架",
          description: "运行 Python Art，需要 Python 运行时。",
          installed: false,
          ready: false,
          readyDetail: "未安装",
        },
      }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      });
    }

    throw new Error(`Unexpected framework path: ${method} ${path}`);
  }) as typeof fetch;

  const frameworks = await listFrameworks("http://127.0.0.1:18771");
  const installed = await installFramework("http://127.0.0.1:18771", "python_art");
  const uninstalled = await uninstallFramework("http://127.0.0.1:18771", "python_art");

  assert.equal(frameworks.length, 1);
  assert.equal(frameworks[0]?.id, "python_art");
  assert.equal(installed?.installed, true);
  assert.equal(installed?.ready, true);
  assert.equal(uninstalled?.installed, false);
  assert.deepEqual(
    seen.map((entry) => `${entry.method} ${entry.path}`),
    [
      "GET /v1/frameworks",
      "POST /v1/frameworks/python_art/install",
      "POST /v1/frameworks/python_art/uninstall",
    ],
  );
  assert.equal(seen[1]?.body, "{}");
  assert.equal(seen[2]?.body, "{}");
});

test("art store helpers call catalog and install routes", async (context) => {
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
            framework: "python_art",
          },
        ],
      }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      });
    }

    if (method === "POST" && url.pathname === "/v1/arts/store/install") {
      return new Response(JSON.stringify({
        reports: [{ toolId: "loom_echo", framework: "python_art" }],
      }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      });
    }

    throw new Error(`Unexpected art store path: ${method} ${url.pathname}${url.search}`);
  }) as typeof fetch;

  const catalog = await fetchArtStoreCatalog(
    "http://127.0.0.1:18772",
    "http://127.0.0.1:8790",
  );
  await installArtFromStore(
    "http://127.0.0.1:18772",
    "loom_echo",
    "http://127.0.0.1:8790",
  );

  assert.equal(catalog.length, 1);
  assert.equal(catalog[0]?.framework, "python_art");
  assert.deepEqual(
    seen.map((entry) => `${entry.method} ${entry.pathWithQuery}`),
    [
      "GET /v1/arts/store/catalog?store=http%3A%2F%2F127.0.0.1%3A8790",
      "POST /v1/arts/store/install",
    ],
  );
  assert.equal(
    seen[1]?.body,
    JSON.stringify({ artId: "loom_echo", store: "http://127.0.0.1:8790" }),
  );
});
