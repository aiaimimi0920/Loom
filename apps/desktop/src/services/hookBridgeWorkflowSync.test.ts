import assert from "node:assert/strict";
import test from "node:test";

import {
  HOOK_LIVE_WORKFLOW_ID,
  startHookBridgeWorkflowSync,
} from "./hookBridgeWorkflowSync.ts";

type Handler = (payload: unknown) => void;

class FakeHookBridgeClient {
  readonly handlers = new Map<string, Set<Handler>>();
  disposed = false;

  subscribe(channel: string, handler: Handler): () => void {
    const bucket = this.handlers.get(channel) ?? new Set<Handler>();
    bucket.add(handler);
    this.handlers.set(channel, bucket);
    return () => {
      const current = this.handlers.get(channel);
      if (!current) return;
      current.delete(handler);
      if (current.size === 0) this.handlers.delete(channel);
    };
  }

  emit(channel: string, payload: unknown) {
    const bucket = this.handlers.get(channel);
    if (!bucket) return;
    for (const handler of bucket) handler(payload);
  }

  dispose() {
    this.disposed = true;
    this.handlers.clear();
  }
}

async function waitForDebounce() {
  await new Promise((resolve) => setTimeout(resolve, 10));
  await Promise.resolve();
  await Promise.resolve();
}

function createSync(
  client: FakeHookBridgeClient,
  events: string[],
) {
  return startHookBridgeWorkflowSync({
    client,
    refresh: async () => {
      events.push("refresh");
    },
    invalidateHookCanvas: () => {
      events.push("invalidate");
    },
    openHookWorkflow: () => {
      events.push("open");
    },
    debounceMs: 1,
  });
}

test("subscribes to instantiate, workflow_updated, and arts_updated channels", () => {
  const client = new FakeHookBridgeClient();

  const handle = createSync(client, []);

  assert.deepEqual(Array.from(client.handlers.keys()).sort(), [
    "art_hook/instantiate",
    "art_loom/arts_updated",
    "art_loom/workflow_updated",
  ]);

  handle.dispose();
  assert.equal(client.disposed, true);
});

test("instantiate refreshes, invalidates, and opens the Hook workflow", async () => {
  const client = new FakeHookBridgeClient();
  const events: string[] = [];
  const handle = createSync(client, events);

  client.emit("art_hook/instantiate", { workflow_id: HOOK_LIVE_WORKFLOW_ID });
  await waitForDebounce();

  assert.deepEqual(events, ["refresh", "invalidate", "open"]);
  handle.dispose();
});

test("workflow and Art updates refresh without forced navigation", async () => {
  const client = new FakeHookBridgeClient();
  const events: string[] = [];
  const handle = createSync(client, events);

  client.emit("art_loom/workflow_updated", { workflowId: HOOK_LIVE_WORKFLOW_ID });
  await waitForDebounce();
  client.emit("art_loom/arts_updated", {});
  await waitForDebounce();

  assert.deepEqual(events, ["refresh", "invalidate", "refresh", "invalidate"]);
  handle.dispose();
});

test("events inside one debounce window produce one refresh and invalidation", async () => {
  const client = new FakeHookBridgeClient();
  const events: string[] = [];
  const handle = createSync(client, events);

  client.emit("art_loom/arts_updated", {});
  client.emit("art_loom/workflow_updated", { workflowId: HOOK_LIVE_WORKFLOW_ID });
  client.emit("art_hook/instantiate", { workflow_id: HOOK_LIVE_WORKFLOW_ID });
  await waitForDebounce();

  assert.deepEqual(events, ["refresh", "invalidate", "open"]);
  handle.dispose();
});

test("unrelated workflow updates do nothing", async () => {
  const client = new FakeHookBridgeClient();
  const events: string[] = [];
  const handle = createSync(client, events);

  client.emit("art_loom/workflow_updated", { workflowId: "other-workflow" });
  await waitForDebounce();

  assert.deepEqual(events, []);
  handle.dispose();
});

test("dispose cancels pending debounce work", async () => {
  const client = new FakeHookBridgeClient();
  const events: string[] = [];
  const handle = createSync(client, events);

  client.emit("art_loom/arts_updated", {});
  handle.dispose();
  await waitForDebounce();

  assert.deepEqual(events, []);
  assert.equal(client.disposed, true);
});
