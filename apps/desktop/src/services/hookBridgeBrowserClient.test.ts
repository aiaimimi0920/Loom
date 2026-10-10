import assert from "node:assert/strict";
import test from "node:test";
import { createHookBridgeBrowserClient, type HookBridgeSubscriptionState } from "./hookBridgeBrowserClient.ts";

const WORKFLOW = "loom.hook.workflow.updated";
const CAPS = "loom.hook.capabilities.updated";
const state = (changes: Partial<HookBridgeSubscriptionState> = {}): HookBridgeSubscriptionState => ({
  connected: true, epoch: "1", workflowRevision: "0", capabilitiesRevision: "0", ...changes,
});
const settle = async () => { await Promise.resolve(); await Promise.resolve(); };

function harness() {
  let current = state();
  let reads = 0;
  const timers = new Map<number, () => void>();
  let nextId = 0;
  const client = createHookBridgeBrowserClient({
    readState: async () => { reads += 1; return current; },
    schedule: (callback) => {
      const id = ++nextId;
      timers.set(id, callback);
      return id as unknown as ReturnType<typeof setTimeout>;
    },
    cancel: (handle) => { timers.delete(handle as unknown as number); },
  });
  return {
    client, timers, reads: () => reads,
    async tick(next = current) {
      current = next;
      const pending = [...timers.values()];
      timers.clear();
      pending.forEach((callback) => callback());
      await settle();
    },
  };
}

test("fixed native revisions refresh only changed channels; reconnect refreshes both", async () => {
  const h = harness();
  const workflows: unknown[] = [];
  let caps = 0;
  h.client.subscribe(WORKFLOW, (payload) => workflows.push(payload));
  h.client.subscribe(CAPS, () => { caps += 1; });
  await settle();
  assert.deepEqual(workflows, [{ workflowId: "hook-live" }]);
  assert.equal(caps, 1);
  await h.tick();
  assert.equal(workflows.length, 1);
  await h.tick(state({ workflowRevision: "1" }));
  assert.equal(workflows.length, 2);
  assert.equal(caps, 1);
  await h.tick(state({ connected: false }));
  assert.equal(caps, 1);
  await h.tick(state({ epoch: "2" }));
  assert.equal(workflows.length, 3);
  assert.equal(caps, 2);
  h.client.dispose();
  assert.equal(h.timers.size, 0);
});

test("unsupported channels do not create work; last unsubscribe cancels polling", async () => {
  const h = harness();
  h.client.subscribe("loom.extension.command.invoke", () => assert.fail());
  assert.equal(h.reads(), 0);
  const stop = h.client.subscribe(WORKFLOW, () => {});
  await settle();
  assert.equal(h.timers.size, 1);
  stop();
  assert.equal(h.timers.size, 0);
  h.client.dispose();
});

test("one in-flight IPC read, late response fenced across unsubscribe and resubscribe", async () => {
  let resolve!: (value: HookBridgeSubscriptionState) => void;
  let reads = 0;
  let scheduled: (() => void) | undefined;
  const client = createHookBridgeBrowserClient({
    readState: () => { reads += 1; return new Promise((done) => { resolve = done; }); },
    schedule: (callback) => { scheduled = callback; return 1 as unknown as ReturnType<typeof setTimeout>; },
    cancel: () => { scheduled = undefined; },
  });
  const stop = client.subscribe(WORKFLOW, () => assert.fail("retired listener"));
  client.subscribe(CAPS, () => {} )();
  stop();
  let fresh = 0;
  client.subscribe(WORKFLOW, () => { fresh += 1; });
  assert.equal(reads, 1);
  resolve(state());
  await settle();
  assert.equal(fresh, 0);
  scheduled?.();
  assert.equal(reads, 2);
  resolve(state({ epoch: "2" }));
  await settle();
  assert.equal(fresh, 1);
  client.dispose();
});

test("dispose fences pending IPC completion and never schedules another poll", async () => {
  let resolve!: (value: HookBridgeSubscriptionState) => void;
  const client = createHookBridgeBrowserClient({
    readState: () => new Promise((done) => { resolve = done; }),
    schedule: () => { throw new Error("poll scheduled after dispose"); },
  });
  client.subscribe(WORKFLOW, () => assert.fail("late event"));
  client.dispose();
  resolve(state());
  await settle();
});

test("retired disposer is idempotent and cannot remove a replacement channel", async () => {
  const h = harness();
  const stop = h.client.subscribe(WORKFLOW, () => {});
  await settle();
  stop();
  let fresh = 0;
  h.client.subscribe(WORKFLOW, () => { fresh += 1; });
  stop();
  await settle();
  assert.equal(fresh, 1);
  await h.tick(state({ workflowRevision: "1" }));
  assert.equal(fresh, 2);
  h.client.dispose();
});

test("browser preview opens no WebSocket and schedules no retry", async () => {
  const client = createHookBridgeBrowserClient({
    schedule: () => { throw new Error("preview retry"); },
  });
  client.subscribe(WORKFLOW, () => assert.fail());
  await settle();
  client.dispose();
});

test("transient IPC failure retries, and one throwing listener cannot suppress others", async () => {
  let fail = true;
  let scheduled: (() => void) | undefined;
  const client = createHookBridgeBrowserClient({
    readState: async () => { if (fail) throw new Error("unavailable"); return state(); },
    schedule: (callback) => { scheduled = callback; return 1 as unknown as ReturnType<typeof setTimeout>; },
    cancel: () => { scheduled = undefined; },
  });
  client.subscribe(WORKFLOW, () => { throw new Error("listener"); });
  let received = 0;
  client.subscribe(CAPS, () => { received += 1; });
  await settle();
  fail = false;
  scheduled?.();
  await settle();
  assert.equal(received, 1);
  client.dispose();
});
