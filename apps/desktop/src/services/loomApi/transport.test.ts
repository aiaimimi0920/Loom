// Transport response limits, preview URL confinement, and Tauri failure contracts.
import assert from "node:assert/strict";
import test from "node:test";

import { getLoomDaemonJson, loadHookCanvasPreview } from "../loomApi.ts";

test("browser Hook previews stay on the configured daemon preview route", async () => {
  const preview = await loadHookCanvasPreview(
    "http://127.0.0.1:18765",
    "/v1/hook-bridge/canvas/nodes/node-1/preview",
  );

  assert.equal(
    preview,
    "http://127.0.0.1:18765/v1/hook-bridge/canvas/nodes/node-1/preview",
  );
  await assert.rejects(
    loadHookCanvasPreview("http://127.0.0.1:18765", "https://attacker.test/preview"),
    /outside the preview route/,
  );
  await assert.rejects(
    loadHookCanvasPreview(
      "http://127.0.0.1:18765",
      "/v1/hook-bridge/canvas/nodes/../../../admin/preview",
    ),
    /outside the preview route/,
  );
});

test("Tauri Hook preview failures do not fall back to an external URL", async (context) => {
  const globals = globalThis;
  const hadWindow = Reflect.has(globals, "window");
  const originalWindow: unknown = Reflect.get(globals, "window");
  const hadIsTauri = Reflect.has(globals, "isTauri");
  const originalIsTauri: unknown = Reflect.get(globals, "isTauri");
  const hadInternals = Reflect.has(globals, "__TAURI_INTERNALS__");
  const originalInternals: unknown = Reflect.get(globals, "__TAURI_INTERNALS__");
  context.after(() => {
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

  Reflect.set(globals, "window", globals);
  Reflect.set(globals, "isTauri", true);
  Reflect.set(globals, "__TAURI_INTERNALS__", {
    invoke: async () => {
      throw new Error("preview denied");
    },
  });

  await assert.rejects(
    loadHookCanvasPreview(
      "http://127.0.0.1:18765",
      "/v1/hook-bridge/canvas/nodes/node-1/preview",
    ),
    /preview denied/,
  );
});

test("daemon JSON rejects a declared response larger than the transport limit", async (context) => {
  const originalFetch = globalThis.fetch;
  context.after(() => {
    globalThis.fetch = originalFetch;
  });
  globalThis.fetch = (async () => new Response("{}", {
    headers: { "Content-Length": String(16 * 1024 * 1024 + 1) },
  })) as typeof fetch;

  await assert.rejects(
    getLoomDaemonJson("http://127.0.0.1:18765", "/v1/oversized"),
    /超过 16777216 字节限制/,
  );
});

test("daemon JSON rejects a streamed response larger than the transport limit", async (context) => {
  const originalFetch = globalThis.fetch;
  context.after(() => {
    globalThis.fetch = originalFetch;
  });
  globalThis.fetch = (async () => new Response(new ReadableStream<Uint8Array>({
    start(controller) {
      controller.enqueue(new Uint8Array(8 * 1024 * 1024));
      controller.enqueue(new Uint8Array(8 * 1024 * 1024));
      controller.enqueue(new Uint8Array(1));
      controller.close();
    },
  }))) as typeof fetch;

  await assert.rejects(
    getLoomDaemonJson("http://127.0.0.1:18765", "/v1/streamed-oversized"),
    /超过 16777216 字节限制/,
  );
});

test("daemon HTTP errors truncate untrusted response details", async (context) => {
  const originalFetch = globalThis.fetch;
  context.after(() => {
    globalThis.fetch = originalFetch;
  });
  const detail = `${"x".repeat(3_000)}-untrusted-tail`;
  globalThis.fetch = (async () => Response.json({
    error: { message: detail },
  }, { status: 500 })) as typeof fetch;

  await assert.rejects(
    getLoomDaemonJson("http://127.0.0.1:18765", "/v1/error"),
    (error: unknown) => {
      assert.ok(error instanceof Error);
      assert.match(error.message, /^Loom 本地服务请求 \/v1\/error 返回 HTTP 500：x+/);
      assert.ok(error.message.length < 2_200);
      assert.doesNotMatch(error.message, /untrusted-tail/);
      return true;
    },
  );
});
