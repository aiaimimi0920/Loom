import assert from "node:assert/strict";
import test from "node:test";
import { createHookCanvasPreviewLoader } from "./hookCanvasPreviewLoader.ts";

const tick = () => new Promise<void>((resolve) => setImmediate(resolve));

test("1000 previews keep at most four actual reads active", async () => {
  let active = 0, peak = 0, reads = 0;
  const loader = createHookCanvasPreviewLoader(async (_base, path) => {
    reads++; peak = Math.max(peak, ++active);
    await tick(); active--; return path;
  });
  const results = await Promise.all(Array.from({ length: 1000 }, (_, id) => loader.load("local", String(id))));
  assert.equal(reads, 1000);
  assert.equal(peak, 4);
  assert.equal(results[999], "999");
});

test("aborting active and queued subscriptions preserves the native concurrency bound", async () => {
  const releases: Array<() => void> = [];
  const reads: string[] = [];
  const loader = createHookCanvasPreviewLoader(async (_base, path) => {
    reads.push(path);
    await new Promise<void>((resolve) => releases.push(resolve));
    return path;
  }, { concurrency: 1 });
  const active = new AbortController(), queued = new AbortController();
  const first = loader.load("local", "first", active.signal);
  const skipped = loader.load("local", "skipped", queued.signal);
  const final = loader.load("local", "final");
  active.abort(); queued.abort();
  await assert.rejects(first, { name: "AbortError" });
  await assert.rejects(skipped, { name: "AbortError" });
  assert.deepEqual(reads, ["first"]);
  releases.shift()!(); await tick();
  assert.deepEqual(reads, ["first", "final"]);
  releases.shift()!(); assert.equal(await final, "final");
});

test("temporary 503 failures retry within a fixed attempt budget", async () => {
  let calls = 0;
  const delays: number[] = [];
  const loader = createHookCanvasPreviewLoader(async () => {
    if (++calls < 3) throw new Error("/preview returned HTTP/1.1 503 Service Unavailable");
    return "image";
  }, { sleep: async (ms) => { delays.push(ms); } });
  assert.equal(await loader.load("local", "preview"), "image");
  assert.equal(calls, 3); assert.deepEqual(delays, [150, 300]);
});

test("permanent failure and repeated transient failure do not retry forever", async () => {
  for (const [message, expected] of [["HTTP/1.1 403 Forbidden", 1], ["/nodes/503/preview HTTP/1.1 403 Forbidden", 1], ["HTTP/1.1 503 Busy", 3]] as const) {
    let reads = 0;
    const loader = createHookCanvasPreviewLoader(async () => { reads++; throw new Error(message); }, { sleep: async () => {} });
    await assert.rejects(loader.load("local", "preview"), new RegExp(message));
    assert.equal(reads, expected);
  }
});

test("queued work is bounded and pre-aborted work never starts", async () => {
  let release!: () => void;
  const loader = createHookCanvasPreviewLoader(async () => { await new Promise<void>((resolve) => { release = resolve; }); return "ok"; }, { concurrency: 1, maxPending: 1 });
  const first = loader.load("local", "1");
  const controller = new AbortController();
  const second = loader.load("local", "2", controller.signal);
  await assert.rejects(loader.load("local", "3"), /queue is full/);
  controller.abort();
  await assert.rejects(second, { name: "AbortError" });
  await assert.rejects(loader.load("local", "4", controller.signal), { name: "AbortError" });
  release(); await first;
});

test("aborting retry backoff releases the slot without another read", async () => {
  let calls = 0;
  const controller = new AbortController();
  const loader = createHookCanvasPreviewLoader(async () => { calls++; throw new Error("HTTP/1.1 503 Busy"); });
  const pending = loader.load("local", "preview", controller.signal);
  await tick(); controller.abort();
  await assert.rejects(pending, { name: "AbortError" });
  await tick(); assert.equal(calls, 1);
});
