import assert from "node:assert/strict";
import test from "node:test";

import {
  artLoomExecuteArtNodeErrorMessage,
  autoUpdateArts,
  bootstrapPackagedArts,
  fetchArtStoreCatalog,
  fetchMcpRegistry,
  getArtManagement,
  getPublisherIdentity,
  installArtFromStore,
  installFramework,
  callMcpTool,
  listPluginCredentials,
  listPluginTrust,
  listFrameworks,
  publishArt,
  readLoomSnapshot,
  retainAvailableSnapshotData,
  revokePluginPublisher,
  revealPluginCredential,
  revealPublisherPrivateKey,
  rotatePublisherIdentity,
  savePluginCredential,
  saveArtManagementSettings,
  saveHookCanvasWorkflow,
  setPluginTrustPolicy,
  trustPluginUser,
  trustPluginPublisher,
  untrustPluginUser,
  updateArtToVersion,
  updateArtLoomWorkflowNode,
  deletePluginCredential,
  uninstallFramework,
  uninstallArtPackage,
  registerPublisherIdentity,
  waitForLoomOnline,
  type ConnectionState,
  type LoomSnapshot,
} from "./loomApi.ts";

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

test("retains the last available Art data while the daemon is temporarily offline", () => {
  const previous = readinessSnapshot("online");
  previous.tools = [{ id: "color-transfer", name: "颜色迁移" } as LoomSnapshot["tools"][number]];
  previous.workflows = [{ id: "workflow-art", name: "颜色迁移-压缩" } as LoomSnapshot["workflows"][number]];
  const offline = readinessSnapshot("offline");

  const retained = retainAvailableSnapshotData(previous, offline);

  assert.equal(retained.connectionState, "offline");
  assert.equal(retained.error, "offline");
  assert.deepEqual(retained.tools, previous.tools);
  assert.deepEqual(retained.workflows, previous.workflows);
});

test("retains only modules that failed in an otherwise online snapshot", () => {
  const previous = readinessSnapshot("online");
  previous.tools = [{ id: "image-compression", name: "图片压缩" } as LoomSnapshot["tools"][number]];
  previous.mcpServers = [{ id: "old-mcp", name: "旧服务" } as LoomSnapshot["mcpServers"][number]];
  const degraded = readinessSnapshot("online");
  degraded.error = "Loom 本地服务在线，但部分模块暂不可用：/v1/tools returned HTTP 500";

  const retained = retainAvailableSnapshotData(previous, degraded);

  assert.deepEqual(retained.tools, previous.tools);
  assert.deepEqual(retained.mcpServers, []);
});

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

test("Art package uninstall uses the publisher-qualified package route", async (context) => {
  const originalFetch = globalThis.fetch;
  context.after(() => {
    globalThis.fetch = originalFetch;
  });

  let seenMethod = "";
  let seenPath = "";
  globalThis.fetch = (async (input: string | URL | Request, init?: RequestInit) => {
    const value = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    seenMethod = String(init?.method ?? "GET").toUpperCase();
    seenPath = new URL(value).pathname;
    return new Response(JSON.stringify({ artId: "publisher.test/sample-art", uninstalled: true }), {
      status: 200,
      headers: { "Content-Type": "application/json" },
    });
  }) as typeof fetch;

  await uninstallArtPackage("http://127.0.0.1:18770", "publisher.test/sample-art");

  assert.equal(seenMethod, "POST");
  assert.equal(seenPath, "/v1/arts/publisher.test%2Fsample-art/uninstall");
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
            id: "process",
            qualifiedId: "neuro.official/process",
            name: "Python Art 框架",
            description: "运行 Python Art，需要 Python 运行时。",
            installed: false,
            enabled: false,
            ready: false,
            readyDetail: "未安装",
          },
          {
            id: "process",
            qualifiedId: "neuro.official/process",
            name: "Python Art 框架",
            description: "运行 Python Art，需要 Python 运行时。",
            installed: true,
            enabled: true,
            ready: true,
            readyDetail: "已安装",
            version: "0.1.0",
          },
        ],
      }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      });
    }

    if (method === "POST" && path === "/v1/frameworks/process/install") {
      return new Response(JSON.stringify({
        framework: {
          id: "process",
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

    if (method === "POST" && path === "/v1/frameworks/process/uninstall") {
      return new Response(JSON.stringify({
        framework: {
          id: "process",
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

    if (method === "POST" && path === "/v1/frameworks/publisher.alpha%2Fshared/install") {
      return Response.json({
        framework: {
          id: "shared",
          qualifiedId: "publisher.alpha/shared",
          name: "Shared Framework",
          description: "Publisher qualified fixture",
          installed: true,
          ready: true,
          readyDetail: "ready",
        },
      });
    }

    throw new Error(`Unexpected framework path: ${method} ${path}`);
  }) as typeof fetch;

  const frameworks = await listFrameworks("http://127.0.0.1:18771");
  const installed = await installFramework("http://127.0.0.1:18771", "process");
  const uninstalled = await uninstallFramework("http://127.0.0.1:18771", "process");
  const qualified = await installFramework(
    "http://127.0.0.1:18771",
    "publisher.alpha/shared",
  );

  assert.equal(frameworks.length, 1);
  assert.equal(frameworks[0]?.id, "process");
  assert.equal(frameworks[0]?.installed, true);
  assert.equal(installed?.installed, true);
  assert.equal(installed?.ready, true);
  assert.equal(uninstalled?.installed, false);
  assert.equal(qualified?.qualifiedId, "publisher.alpha/shared");
  assert.deepEqual(
    seen.map((entry) => `${entry.method} ${entry.path}`),
    [
      "GET /v1/frameworks",
      "POST /v1/frameworks/process/install",
      "POST /v1/frameworks/process/uninstall",
      "POST /v1/frameworks/publisher.alpha%2Fshared/install",
    ],
  );
  assert.equal(seen[1]?.body, "{}");
  assert.equal(seen[2]?.body, "{}");
  assert.equal(seen[3]?.body, "{}");
});

test("browser mode leaves packaged Art bootstrap to the desktop host", async () => {
  assert.deepEqual(await bootstrapPackagedArts("http://127.0.0.1:18770"), {
    available: false,
    applied: false,
    catalogHash: null,
    frameworkIds: [],
    artIds: [],
  });
});

test("framework install surfaces the daemon error message in browser mode", async (context) => {
  const originalFetch = globalThis.fetch;
  context.after(() => {
    globalThis.fetch = originalFetch;
  });

  globalThis.fetch = (async () => Response.json({
    error: {
      code: "framework_install_failed",
      message: "framework `process` has no available package source",
    },
  }, { status: 500 })) as typeof fetch;

  await assert.rejects(
    installFramework("http://127.0.0.1:18771", "process"),
    /HTTP 500.*no available package source/,
  );
});

test("framework listing deduplicates canonical identities without merging publishers", async (context) => {
  const originalFetch = globalThis.fetch;
  context.after(() => {
    globalThis.fetch = originalFetch;
  });

  const framework = (qualifiedId: string) => ({
    id: "shared",
    qualifiedId,
    name: "Shared",
    description: "",
    installed: true,
    enabled: true,
    ready: true,
    readyDetail: "ready",
  });
  globalThis.fetch = (async () => Response.json({
    frameworks: [
      framework("publisher.alpha/shared"),
      framework("publisher.alpha/shared"),
      framework("publisher.beta/shared"),
    ],
  })) as typeof fetch;

  const frameworks = await listFrameworks("http://127.0.0.1:18771");

  assert.deepEqual(
    frameworks.map((entry) => entry.qualifiedId),
    ["publisher.alpha/shared", "publisher.beta/shared"],
  );
});

test("Tauri framework installs use the packaged fallback command without direct HTTP", async (context) => {
  const globals = globalThis;
  const originalFetch = globals.fetch;
  const hadWindow = Reflect.has(globals, "window");
  const originalWindow: unknown = Reflect.get(globals, "window");
  const hadIsTauri = Reflect.has(globals, "isTauri");
  const originalIsTauri: unknown = Reflect.get(globals, "isTauri");
  const hadInternals = Reflect.has(globals, "__TAURI_INTERNALS__");
  const originalInternals: unknown = Reflect.get(globals, "__TAURI_INTERNALS__");
  context.after(() => {
    globals.fetch = originalFetch;
    for (const [name, existed, value] of [
      ["window", hadWindow, originalWindow],
      ["isTauri", hadIsTauri, originalIsTauri],
      ["__TAURI_INTERNALS__", hadInternals, originalInternals],
    ] as const) {
      if (existed) {
        Reflect.set(globals, name, value);
      } else {
        Reflect.deleteProperty(globals, name);
      }
    }
  });

  let fetchCalls = 0;
  globals.fetch = (async () => {
    fetchCalls += 1;
    return Response.json({ framework: { id: "process", installed: true } });
  }) as typeof fetch;
  Reflect.set(globals, "window", globals);
  Reflect.set(globals, "isTauri", true);
  let invokeCalls = 0;
  Reflect.set(globals, "__TAURI_INTERNALS__", {
    invoke: async (command: string, args: Record<string, unknown>) => {
      invokeCalls += 1;
      assert.equal(command, "install_packaged_framework");
      assert.equal(args.baseUrl, "http://127.0.0.1:18771");
      if (args.id === "cloud_api") {
        return { framework: { id: "cloud_api", installed: true, ready: true } };
      }
      assert.equal(args.id, "process");
      throw "framework `process` package checksum mismatch";
    },
  });

  const installed = await installFramework("http://127.0.0.1:18771", "cloud_api");
  assert.equal(installed?.id, "cloud_api");
  assert.equal(installed?.ready, true);

  await assert.rejects(
    installFramework("http://127.0.0.1:18771", "process"),
    (error: unknown) => {
      assert.ok(error instanceof Error);
      assert.match(error.message, /checksum mismatch/);
      return true;
    },
  );
  assert.equal(invokeCalls, 2);
  assert.equal(fetchCalls, 0);
});

test("plugin trust credentials and publisher identity helpers preserve their contracts", async (context) => {
  const originalFetch = globalThis.fetch;
  context.after(() => {
    globalThis.fetch = originalFetch;
  });
  const seen: Array<{ method: string; path: string; body: Record<string, unknown> | null }> = [];
  globalThis.fetch = (async (input: string | URL | Request, init?: RequestInit) => {
    const value = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    const path = new URL(value).pathname;
    const method = String(init?.method ?? "GET").toUpperCase();
    const body = typeof init?.body === "string"
      ? JSON.parse(init.body) as Record<string, unknown>
      : null;
    seen.push({ method, path, body });
    if (path === "/v1/plugin-trust") {
      return Response.json({ publishers: [] });
    }
    if (path === "/v1/plugin-trust/publishers") {
      return Response.json({ publishers: [{ ...body, revoked: false }] });
    }
    if (path === "/v1/plugin-trust/revoke") {
      return Response.json({
        publishers: [{ publisherId: body?.publisherId, keyId: body?.keyId, publicKey: "key", revoked: true }],
      });
    }
    if (path === "/v1/plugin-trust/policy") {
      return Response.json({ publishers: [], policy: body?.policy, trustedPublishers: [] });
    }
    if (path === "/v1/plugin-trust/users") {
      return Response.json({ publishers: [], policy: "require_trusted", trustedPublishers: [body?.userId] });
    }
    if (path === "/v1/plugin-trust/users/remove") {
      return Response.json({ publishers: [], policy: "require_trusted", trustedPublishers: [] });
    }
    if (method === "GET" && path === "/v1/plugin-credentials") {
      return Response.json({ credentials: [] });
    }
    if (method === "POST" && path === "/v1/plugin-credentials") {
      return Response.json({
        credential: { name: body?.name, valueType: body?.valueType, scope: body?.scope, protection: "dpapi" },
      });
    }
    if (path === "/v1/plugin-credentials/delete") {
      return Response.json({ deleted: true });
    }
    if (path === "/v1/plugin-credentials/reveal") {
      return Response.json({
        credential: {
          name: body?.name,
          value: "revealed-secret",
          valueType: "string",
          scope: body?.scope,
          protection: "dpapi",
        },
      });
    }
    if (method === "GET" && path === "/v1/publisher-identity") {
      return Response.json({
        identity: {
          schemaVersion: 1,
          userId: "L0000000000",
          currentKeyId: "key-1",
          publicKey: "public-key",
        },
        hasPrivateKey: true,
      });
    }
    if (path === "/v1/publisher-identity/register" || path === "/v1/publisher-identity/rotate") {
      return Response.json({
        identity: {
          schemaVersion: 1,
          userId: "L0000000000",
          currentKeyId: path.endsWith("rotate") ? "key-2" : "key-1",
          publicKey: "public-key",
        },
        hasPrivateKey: true,
      });
    }
    if (path === "/v1/publisher-identity/private-key") {
      return Response.json({ keyId: "key-1", privateKey: "private-key", publicKey: "public-key" });
    }
    throw new Error(`Unexpected plugin security route: ${method} ${path}`);
  }) as typeof fetch;

  assert.deepEqual(await listPluginTrust("http://127.0.0.1:18773"), {
    schemaVersion: undefined,
    publishers: [],
    policy: "allow_unsigned",
    trustedPublishers: [],
  });
  const trusted = await trustPluginPublisher("http://127.0.0.1:18773", {
    publisherId: "publisher.alpha",
    keyId: "release-key",
    publicKey: "base64-key",
  });
  assert.equal(trusted.publishers[0]?.publisherId, "publisher.alpha");
  const revoked = await revokePluginPublisher(
    "http://127.0.0.1:18773",
    "publisher.alpha",
    "release-key",
  );
  assert.equal(revoked.publishers[0]?.revoked, true);
  assert.equal((await setPluginTrustPolicy("http://127.0.0.1:18773", "require_signed")).policy, "require_signed");
  assert.deepEqual(
    (await trustPluginUser("http://127.0.0.1:18773", "L0000000000")).trustedPublishers,
    ["L0000000000"],
  );
  assert.deepEqual(
    (await untrustPluginUser("http://127.0.0.1:18773", "L0000000000")).trustedPublishers,
    [],
  );
  assert.deepEqual(await listPluginCredentials("http://127.0.0.1:18773"), []);
  const credential = await savePluginCredential("http://127.0.0.1:18773", {
    name: "api_key",
    value: "write-only-secret",
    valueType: "string",
    scope: { frameworkId: "publisher.alpha/shared-framework", artId: "publisher.alpha/shared-art" },
  });
  assert.equal(credential?.name, "api_key");
  assert.equal("value" in (credential ?? {}), false);
  assert.equal(
    (await revealPluginCredential("http://127.0.0.1:18773", "api_key", {
      frameworkId: "publisher.alpha/shared-framework",
      artId: "publisher.alpha/shared-art",
    }))?.value,
    "revealed-secret",
  );
  await deletePluginCredential(
    "http://127.0.0.1:18773",
    "api_key",
    { frameworkId: "publisher.alpha/shared-framework", artId: "publisher.alpha/shared-art" },
  );

  const saveRequest = seen.find((entry) => entry.path === "/v1/plugin-credentials" && entry.method === "POST");
  assert.equal(saveRequest?.body?.value, "write-only-secret");
  assert.equal(saveRequest?.body?.valueType, "string");
  assert.deepEqual(saveRequest?.body?.scope, {
    frameworkId: "publisher.alpha/shared-framework",
    artId: "publisher.alpha/shared-art",
  });
  assert.deepEqual(await getPublisherIdentity("http://127.0.0.1:18773"), {
    identity: {
      schemaVersion: 1,
      userId: "L0000000000",
      currentKeyId: "key-1",
      publicKey: "public-key",
    },
    hasPrivateKey: true,
  });
  assert.equal((await registerPublisherIdentity("http://127.0.0.1:18773")).identity?.currentKeyId, "key-1");
  assert.equal((await rotatePublisherIdentity("http://127.0.0.1:18773")).identity?.currentKeyId, "key-2");
  assert.equal((await revealPublisherPrivateKey("http://127.0.0.1:18773")).privateKey, "private-key");
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
