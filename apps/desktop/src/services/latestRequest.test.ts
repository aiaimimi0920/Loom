import assert from "node:assert/strict";
import test from "node:test";

import { createLatestRequestGate, createSingleFlightGate } from "./latestRequest.ts";

test("only the latest request token remains current", () => {
  const gate = createLatestRequestGate();
  const first = gate.begin();
  assert.equal(gate.isCurrent(first), true);

  const second = gate.begin();
  assert.equal(gate.isCurrent(first), false);
  assert.equal(gate.isCurrent(second), true);
});

test("invalidating the gate rejects an in-flight request", () => {
  const gate = createLatestRequestGate();
  const request = gate.begin();

  gate.invalidate();

  assert.equal(gate.isCurrent(request), false);
});

test("single-flight gate shares one active operation and allows the next after completion", async () => {
  const gate = createSingleFlightGate();
  let resolveFirst!: (value: number) => void;
  let calls = 0;
  const first = gate.run(() => {
    calls += 1;
    return new Promise<number>((resolve) => {
      resolveFirst = resolve;
    });
  });
  const overlapping = gate.run(async () => {
    calls += 1;
    return 99;
  });

  assert.equal(first, overlapping);
  assert.equal(gate.isRunning(), true);
  await Promise.resolve();
  assert.equal(calls, 1);
  resolveFirst(7);
  assert.equal(await first, 7);
  assert.equal(gate.isRunning(), false);

  const next = await gate.run(async () => {
    calls += 1;
    return 11;
  });
  assert.equal(next, 11);
  assert.equal(calls, 2);
});

test("invalidating a single-flight gate permits a replacement without letting the old request clear it", async () => {
  const gate = createSingleFlightGate();
  let resolveFirst!: (value: number) => void;
  let resolveSecond!: (value: number) => void;
  let calls = 0;
  const first = gate.run(() => {
    calls += 1;
    return new Promise<number>((resolve) => {
      resolveFirst = resolve;
    });
  });
  await Promise.resolve();

  gate.invalidate();
  const second = gate.run(() => {
    calls += 1;
    return new Promise<number>((resolve) => {
      resolveSecond = resolve;
    });
  });
  await Promise.resolve();

  assert.equal(calls, 2);
  resolveFirst(7);
  assert.equal(await first, 7);
  assert.equal(gate.isRunning(), true);

  resolveSecond(11);
  assert.equal(await second, 11);
  assert.equal(gate.isRunning(), false);
});
