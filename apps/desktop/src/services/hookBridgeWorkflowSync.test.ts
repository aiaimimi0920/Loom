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
      if (current.size === 0) {
        this.handlers.delete(channel);
      }
    };
  }

  emit(channel: string, payload: unknown) {
    const bucket = this.handlers.get(channel);
    if (!bucket) return;
    for (const handler of bucket) {
      handler(payload);
    }
  }

  dispose() {
    this.disposed = true;
    this.handlers.clear();
  }
}

async function flushMicrotasks() {
  await Promise.resolve();
  await Promise.resolve();
}

test("subscribes to instantiate, workflow_updated, and arts_updated channels", () => {
  const client = new FakeHookBridgeClient();

  const handle = startHookBridgeWorkflowSync({
    client,
    refresh: () => undefined,
    openHookWorkflow: () => undefined,
  });

  assert.deepEqual(Array.from(client.handlers.keys()).sort(), [
    "art_hook/instantiate",
    "art_loom/arts_updated",
    "art_loom/workflow_updated",
  ]);

  handle.dispose();
  assert.equal(client.disposed, true);
});

test("refreshes and opens hook workflow for instantiate and matching workflow updates", async () => {
  const client = new FakeHookBridgeClient();
  const events: string[] = [];

  const handle = startHookBridgeWorkflowSync({
    client,
    refresh: async () => {
      events.push("refresh");
    },
    openHookWorkflow: () => {
      events.push("open");
    },
  });

  client.emit("art_hook/instantiate", { workflow_id: HOOK_LIVE_WORKFLOW_ID });
  await flushMicrotasks();
  client.emit("art_loom/workflow_updated", { workflowId: HOOK_LIVE_WORKFLOW_ID });
  await flushMicrotasks();
  client.emit("art_loom/workflow_updated", { workflowId: "other-workflow" });
  await flushMicrotasks();
  client.emit("art_loom/workflow_updated", {});
  await flushMicrotasks();
  client.emit("art_loom/arts_updated", {});
  await flushMicrotasks();

  assert.deepEqual(events, [
    "refresh",
    "open",
    "refresh",
    "open",
    "refresh",
    "open",
    "refresh",
    "open",
  ]);

  handle.dispose();
});

test("dispose unsubscribes handlers from the client", () => {
  const client = new FakeHookBridgeClient();

  const handle = startHookBridgeWorkflowSync({
    client,
    refresh: () => undefined,
    openHookWorkflow: () => undefined,
  });

  handle.dispose();

  assert.equal(client.handlers.size, 0);
  assert.equal(client.disposed, true);
});
