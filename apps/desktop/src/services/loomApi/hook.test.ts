// Hook canvas workflow export and node-parameter persistence contracts.
import assert from "node:assert/strict";
import test from "node:test";

import { saveHookCanvasWorkflow, updateHookWorkflowNode } from "../loomApi.ts";

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

test("updateHookWorkflowNode sends the formal Hook node-param persistence request", async (context) => {
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

  await updateHookWorkflowNode("http://127.0.0.1:18770", {
    workflowId: "hook-live",
    nodeId: "image-search-node",
    param: "result_index",
    value: 1,
  });

  assert.equal(seenMethod, "POST");
  assert.equal(seenPath, "/v1/hook-bridge/workflows/nodes/update");
  assert.match(seenBody, /"workflowId":"hook-live"/);
  assert.match(seenBody, /"nodeId":"image-search-node"/);
  assert.match(seenBody, /"param":"result_index"/);
  assert.match(seenBody, /"value":1/);
});
