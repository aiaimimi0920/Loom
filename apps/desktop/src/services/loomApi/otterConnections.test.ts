import assert from "node:assert/strict";
import test from "node:test";
import { decideOtterConnection, loadOtterConnections, saveOtterConnectionPolicy } from "./otterConnections.ts";

test("connection policy preserves password unless explicitly changed or cleared", async (context) => {
  const sent: Record<string, unknown>[] = [];
  context.mock.method(globalThis, "fetch", async (_url: string, options: RequestInit) => {
    assert.equal(options.method, "PUT");
    sent.push(JSON.parse(String(options.body)) as Record<string, unknown>);
    return Response.json({});
  });
  const draft = { name: "Loom", discoverable: false, endpoint: null, approval: "manual" as const };
  await saveOtterConnectionPolicy("http://localhost:8765", draft);
  await saveOtterConnectionPolicy("http://localhost:8765", { ...draft, password: "" });
  assert.equal(Object.hasOwn(sent[0], "password"), false);
  assert.equal(sent[1].password, "");
  assert.equal(Object.hasOwn(sent[0], "loomId"), false);
});

test("connection decisions use the administrator API rather than a client receipt", async (context) => {
  context.mock.method(globalThis, "fetch", async (url: string, options: RequestInit) => {
    assert.equal(new URL(url).pathname, "/v1/otter/connections/decision");
    assert.deepEqual(JSON.parse(String(options.body)), { requestId: "connection-test", approve: false });
    return Response.json({ phase: "rejected" });
  });
  assert.equal((await decideOtterConnection("http://localhost:8765", "connection-test", false)).phase, "rejected");
});

test("connection list surfaces authentication errors instead of fabricating an empty list", async (context) => {
  context.mock.method(globalThis, "fetch", async () => new Response("denied", { status: 401 }));
  await assert.rejects(loadOtterConnections("http://localhost:8765"));
});
