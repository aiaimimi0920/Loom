import assert from "node:assert/strict";
import test from "node:test";

import { maintainHookBridgeAvailability } from "./hookBridgeAvailability.ts";
import type { LoomHookBridgeStatus } from "./loomApi.ts";

const status = (running: boolean): LoomHookBridgeStatus => ({
  connectedClients: 0,
  methods: [],
  port: 19820,
  protocol: "loom.hook.v1",
  running,
  subscribedClients: 0,
});

const flush = async () => {
  await Promise.resolve();
  await Promise.resolve();
};

test("starts a stopped Hook bridge and keeps monitoring it", async () => {
  const scheduled: Array<{ callback: () => void; delayMs: number }> = [];
  const starts: Array<{ baseUrl: string; port?: number }> = [];
  const handle = maintainHookBridgeAvailability(
    { baseUrl: "http://127.0.0.1:8765", websocketUrl: "ws://127.0.0.1:19820" },
    {
      readStatus: async () => status(false),
      start: async (baseUrl, port) => {
        starts.push({ baseUrl, port });
        return status(true);
      },
      schedule: (callback, delayMs) => {
        scheduled.push({ callback, delayMs });
        return {} as unknown as ReturnType<typeof setTimeout>;
      },
      clear: () => undefined,
    },
  );

  await flush();
  assert.deepEqual(starts, [{ baseUrl: "http://127.0.0.1:8765", port: 19820 }]);
  assert.equal(scheduled.length, 1);
  assert.equal(scheduled[0].delayMs, 5_000);
  handle.dispose();
});

test("retries when status or startup fails", async () => {
  const scheduled: Array<{ callback: () => void; delayMs: number }> = [];
  let attempts = 0;
  const handle = maintainHookBridgeAvailability(
    { baseUrl: "http://127.0.0.1:8765", websocketUrl: "invalid" },
    {
      readStatus: async () => {
        attempts += 1;
        if (attempts === 1) throw new Error("daemon warming up");
        return status(true);
      },
      start: async () => status(true),
      schedule: (callback, delayMs) => {
        scheduled.push({ callback, delayMs });
        return {} as unknown as ReturnType<typeof setTimeout>;
      },
      clear: () => undefined,
    },
  );

  await flush();
  assert.equal(scheduled[0].delayMs, 1_000);
  scheduled.shift()?.callback();
  await flush();
  assert.equal(attempts, 2);
  assert.equal(scheduled[0].delayMs, 5_000);
  handle.dispose();
});

test("dispose prevents a late status response from starting the bridge", async () => {
  let resolveStatus: ((value: LoomHookBridgeStatus) => void) | undefined;
  let starts = 0;
  const handle = maintainHookBridgeAvailability(
    { baseUrl: "http://127.0.0.1:8765", websocketUrl: "ws://127.0.0.1:19820" },
    {
      readStatus: async () => await new Promise((resolve) => {
        resolveStatus = resolve;
      }),
      start: async () => {
        starts += 1;
        return status(true);
      },
      schedule: () => ({} as unknown as ReturnType<typeof setTimeout>),
      clear: () => undefined,
    },
  );

  handle.dispose();
  resolveStatus?.(status(false));
  await flush();
  assert.equal(starts, 0);
});
