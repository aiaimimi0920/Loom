import assert from "node:assert/strict";
import test from "node:test";
import { loadWallSourceInventory, type WallSourceInventory } from "./wallSourceInventory.ts";

const busy: WallSourceInventory = { sources: [], errors: ["Art / 图片：HTTP 503 daemon_busy"] };
const loaded: WallSourceInventory = { sources: [{ source: { kind: "surface", id: "form-1" }, label: "Form", status: "form-1" }], errors: [] };
const flush = () => new Promise<void>((resolve) => setImmediate(resolve));

test("a transient source-list failure recovers without polling again after success", async (context) => {
  context.mock.timers.enable({ apis: ["setTimeout"] });
  let reads = 0;
  const published: { inventory: WallSourceInventory; pending: boolean }[] = [];
  const stop = loadWallSourceInventory("http://127.0.0.1:1234", (inventory, pending) => published.push({ inventory, pending }),
    async () => ++reads === 1 ? busy : loaded);
  context.after(stop);
  await flush();
  assert.deepEqual(published, [{ inventory: busy, pending: true }]);
  context.mock.timers.tick(1999); await flush(); assert.equal(reads, 1);
  context.mock.timers.tick(1); await flush();
  assert.deepEqual(published[1], { inventory: loaded, pending: false });
  context.mock.timers.tick(10000); await flush(); assert.equal(reads, 2);
});

test("persistent failures have a finite retry budget and remain visible", async (context) => {
  context.mock.timers.enable({ apis: ["setTimeout"] });
  let reads = 0;
  const pending: boolean[] = [];
  const stop = loadWallSourceInventory("http://127.0.0.1:1234", (inventory, waiting) => {
    assert.equal(inventory.errors.length, 1); pending.push(waiting);
  }, async () => { reads++; throw new Error("service unavailable"); });
  context.after(stop);
  await flush();
  for (let i = 0; i < 2; i++) { context.mock.timers.tick(2000); await flush(); }
  assert.deepEqual(pending, [true, true, false]);
  context.mock.timers.tick(10000); await flush(); assert.equal(reads, 3);
});

test("disposing an old load ignores its late result and its retry timer", async (context) => {
  context.mock.timers.enable({ apis: ["setTimeout"] });
  let resolve!: (inventory: WallSourceInventory) => void;
  const oldRead = new Promise<WallSourceInventory>((done) => { resolve = done; });
  const seen: WallSourceInventory[] = [];
  const stopOld = loadWallSourceInventory("http://127.0.0.1:1234", (value) => seen.push(value), () => oldRead);
  stopOld();
  const stopNew = loadWallSourceInventory("http://127.0.0.1:1235", (value) => seen.push(value), async () => loaded);
  context.after(stopNew);
  await flush(); resolve(busy); await flush();
  context.mock.timers.tick(10000); await flush(); assert.deepEqual(seen, [loaded]);
  let stoppedReads = 0;
  const stopTimer = loadWallSourceInventory("http://127.0.0.1:1235", () => {}, async () => { stoppedReads++; return busy; });
  await flush(); stopTimer();
  context.mock.timers.tick(2000); await flush();
  assert.equal(stoppedReads, 1);
});
