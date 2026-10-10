import assert from "node:assert/strict";
import test from "node:test";
import { createAuthenticatedHookBridgeSocket, setHookBridgePreviewToken } from "./hookBridgeAuthentication.ts";

test("browser preview requires explicit authority and never puts it in the URL", async () => {
  const original = globalThis.WebSocket;
  const calls: unknown[][] = [];
  globalThis.WebSocket = class {
    constructor(...args: unknown[]) { calls.push(args); }
  } as unknown as typeof WebSocket;
  try {
    setHookBridgePreviewToken(null);
    await assert.rejects(createAuthenticatedHookBridgeSocket("ws://127.0.0.1:19820"), /authentication is required/);
    assert.equal(calls.length, 0);
    setHookBridgePreviewToken("fixture-token");
    await createAuthenticatedHookBridgeSocket("ws://127.0.0.1:19820");
    assert.deepEqual(calls, [["ws://127.0.0.1:19820", ["loom.hook.v1", "loom.auth.Zml4dHVyZS10b2tlbg"]]]);
    for (const url of ["ws://evil.example", "ws://user@localhost", "ws://127.0.0.1/?token=x"]) {
      await assert.rejects(createAuthenticatedHookBridgeSocket(url), /loopback endpoint/);
    }
    assert.equal(calls.length, 1);
  } finally {
    globalThis.WebSocket = original;
    setHookBridgePreviewToken(null);
  }
});
