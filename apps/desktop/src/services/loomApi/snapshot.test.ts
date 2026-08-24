// Snapshot retention, readiness timeout, abort, and degraded-service contracts.
import assert from "node:assert/strict";
import test from "node:test";

import {
  readLoomSnapshot,
  retainAvailableSnapshotData,
  waitForLoomOnline,
  type ConnectionState,
  type LoomSnapshot,
} from "../loomApi.ts";

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

async function withTestDeadline<T>(operation: Promise<T>, timeoutMs = 250): Promise<T> {
  let timeoutId: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      operation,
      new Promise<never>((_, reject) => {
        timeoutId = setTimeout(() => reject(new Error("test operation hung")), timeoutMs);
      }),
    ]);
  } finally {
    if (timeoutId !== undefined) clearTimeout(timeoutId);
  }
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

test("readiness timeout bounds a stalled snapshot attempt", async () => {
  let attemptTimeouts = 0;

  const snapshot = await withTestDeadline(waitForLoomOnline(
    async (signal) => await new Promise<LoomSnapshot>((_resolve, reject) => {
      signal.addEventListener("abort", () => {
        reject(new DOMException("Aborted", "AbortError"));
      }, { once: true });
    }),
    {
      timeoutMs: 10,
      attemptTimeoutMs: 5,
      intervalMs: 1,
      onAttemptTimeout: () => {
        attemptTimeouts += 1;
      },
    },
  ));

  assert.equal(snapshot, null);
  assert.ok(attemptTimeouts >= 1);
});

test("readiness timeout aborts only the stalled attempt", async () => {
  let aborted = false;

  await withTestDeadline(waitForLoomOnline(
    async (signal) => await new Promise<LoomSnapshot>((_resolve, reject) => {
      signal.addEventListener("abort", () => {
        aborted = true;
        reject(new DOMException("Aborted", "AbortError"));
      }, { once: true });
    }),
    {
      timeoutMs: 10,
      attemptTimeoutMs: 5,
      intervalMs: 1,
    },
  ));

  assert.equal(aborted, true);
});

test("readiness timeout completes even when its observer throws", async () => {
  const result = await withTestDeadline(
    waitForLoomOnline(
      async () => await new Promise<LoomSnapshot>(() => {}),
      {
        timeoutMs: 10,
        attemptTimeoutMs: 5,
        intervalMs: 1,
        onAttemptTimeout: () => {
          throw new Error("observer failed");
        },
      },
    ),
  );

  assert.equal(result, null);
});

test("browser snapshot requests receive the caller abort signal", async (context) => {
  const originalFetch = globalThis.fetch;
  context.after(() => {
    globalThis.fetch = originalFetch;
  });
  const controller = new AbortController();
  const signals: AbortSignal[] = [];
  let markStarted: (() => void) | undefined;
  const started = new Promise<void>((resolve) => {
    markStarted = resolve;
  });

  globalThis.fetch = (async (_input: string | URL | Request, init?: RequestInit) => {
    const signal = init?.signal;
    assert.ok(signal instanceof AbortSignal);
    signals.push(signal);
    markStarted?.();
    return await new Promise<Response>((_resolve, reject) => {
      const rejectAborted = () => reject(new DOMException("Aborted", "AbortError"));
      if (signal.aborted) {
        rejectAborted();
      } else {
        signal.addEventListener("abort", rejectAborted, { once: true });
      }
    });
  }) as typeof fetch;

  const pending = readLoomSnapshot("http://127.0.0.1:18765", controller.signal);
  await started;
  controller.abort();
  const snapshot = await pending;

  assert.equal(snapshot.connectionState, "offline");
  assert.ok(signals.length >= 2);
  assert.ok(signals.every((signal) => signal === controller.signal));
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
        case "/v1/art-authoring/python/arts":
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
        case "/v1/art-authoring/python/arts":
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
