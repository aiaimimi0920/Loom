import assert from "node:assert/strict";
import test from "node:test";
import { createLatestRequestGate, createSingleFlightGate } from "./latestRequest.ts";
import { refreshSharedSnapshot } from "./snapshotRefresh.ts";
import { waitForLoomOnline } from "./loomApi/snapshot.ts";
import type { LoomSnapshot } from "./loomApi/snapshotTypes.ts";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((complete) => { resolve = complete; });
  return { promise, resolve };
}

function fixture<T>(read: () => Promise<T>) {
  const applied: T[] = [];
  const loading: boolean[] = [];
  const latest = createLatestRequestGate();
  const flight = createSingleFlightGate();
  return {
    applied, loading, latest, flight,
    refresh: (signal?: AbortSignal) => refreshSharedSnapshot({
      latest, flight, read, signal,
      apply: (value) => applied.push(value),
      loading: (value) => loading.push(value),
    }),
  };
}

test("a retry applies the shared native snapshot after the first readiness attempt expires", async () => {
  const native = deferred<LoomSnapshot>();
  const online = { connectionState: "online" } as LoomSnapshot;
  let reads = 0;
  let attempts = 0;
  const state = fixture(() => { reads++; return native.promise; });
  const result = await waitForLoomOnline((signal) => {
    const pending = state.refresh(signal);
    if (++attempts === 2) native.resolve(online);
    return pending;
  }, { timeoutMs: 1_000, attemptTimeoutMs: 5, intervalMs: 1 });

  assert.equal(result, online);
  assert.equal(reads, 1);
  assert.equal(attempts, 2);
  assert.deepEqual(state.applied, [online]);
  assert.equal(state.loading.at(-1), false);
});

test("cancelling the only caller prevents a late native result from updating the UI", async () => {
  const native = deferred<number>();
  const state = fixture(() => native.promise);
  const controller = new AbortController();
  const pending = state.refresh(controller.signal);
  controller.abort();
  native.resolve(7);
  assert.equal(await pending, 7);
  assert.deepEqual(state.applied, []);
  assert.deepEqual(state.loading, [true, false]);
});

test("an old caller abort cannot cancel the current subscriber or clear its loading state", async () => {
  const native = deferred<number>();
  const state = fixture(() => native.promise);
  const firstController = new AbortController();
  const first = state.refresh(firstController.signal);
  const second = state.refresh();
  firstController.abort();
  assert.deepEqual(state.loading, [true, true]);
  native.resolve(9);
  await Promise.all([first, second]);
  assert.deepEqual(state.applied, [9]);
  assert.equal(state.loading.at(-1), false);
});

test("unmount invalidation and a replacement flight reject the previous native result", async () => {
  const oldRead = deferred<number>();
  const newRead = deferred<number>();
  let reads = 0;
  const state = fixture(() => ++reads === 1 ? oldRead.promise : newRead.promise);
  const old = state.refresh();
  await Promise.resolve();
  state.latest.invalidate();
  state.flight.invalidate();
  const current = state.refresh();
  await Promise.resolve();
  newRead.resolve(2);
  await current;
  oldRead.resolve(1);
  await old;
  assert.deepEqual(state.applied, [2]);
});

test("an already aborted caller starts no native work", async () => {
  let reads = 0;
  const state = fixture(async () => ++reads);
  const controller = new AbortController();
  controller.abort();
  await assert.rejects(state.refresh(controller.signal), { name: "AbortError" });
  assert.equal(reads, 0);
  assert.deepEqual(state.loading, []);
});

test("failed reads release the shared flight for a later refresh", async () => {
  let reads = 0;
  const state = fixture(async () => {
    if (++reads === 1) throw new Error("native unavailable");
    return 4;
  });
  await assert.rejects(state.refresh(), /native unavailable/);
  await state.refresh();
  assert.deepEqual(state.applied, [4]);
  assert.equal(state.loading.at(-1), false);
});
