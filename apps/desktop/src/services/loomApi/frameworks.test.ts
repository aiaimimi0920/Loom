// Framework route, deduplication, browser fallback, error, and Tauri contracts.
import assert from "node:assert/strict";
import test from "node:test";

import {
  bootstrapPackagedArts,
  installFramework,
  listFrameworks,
  uninstallFramework,
} from "../loomApi.ts";

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
