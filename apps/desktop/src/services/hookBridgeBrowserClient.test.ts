import assert from "node:assert/strict";
import test from "node:test";

import { createHookBridgeBrowserClient } from "./hookBridgeBrowserClient.ts";

class FakeWebSocket {
  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
  static readonly CLOSING = 2;
  static readonly CLOSED = 3;

  readonly sent: string[] = [];
  readyState = FakeWebSocket.CONNECTING;
  onopen: ((event?: unknown) => void) | null = null;
  onmessage: ((event: { data: string }) => void) | null = null;
  onclose: ((event?: unknown) => void) | null = null;
  onerror: ((event?: unknown) => void) | null = null;

  readonly url: string;

  constructor(url: string) {
    this.url = url;
  }

  send(payload: string) {
    this.sent.push(payload);
  }

  close() {
    this.readyState = FakeWebSocket.CLOSED;
    this.onclose?.();
  }

  open() {
    this.readyState = FakeWebSocket.OPEN;
    this.onopen?.();
  }

  emitMessage(payload: unknown) {
    this.onmessage?.({ data: JSON.stringify(payload) });
  }
}

test("subscribes to requested hook bridge channels when the socket opens", () => {
  const sockets: FakeWebSocket[] = [];
  const client = createHookBridgeBrowserClient({
    websocketFactory: (url) => {
      const socket = new FakeWebSocket(url);
      sockets.push(socket);
      return socket;
    },
  });

  const stop = client.subscribe("art_loom/workflow_updated", () => {});

  assert.equal(sockets.length, 1);
  assert.deepEqual(sockets[0].sent, []);

  sockets[0].open();

  assert.equal(sockets[0].sent.length, 1);
  assert.deepEqual(JSON.parse(sockets[0].sent[0]), {
    method: "subscribe",
    params: {
      channels: ["art_loom/workflow_updated"],
    },
  });

  stop();
  client.dispose();
});

test("uses an isolated Hook Bridge URL when one is configured", () => {
  const sockets: FakeWebSocket[] = [];
  const client = createHookBridgeBrowserClient({
    url: "ws://127.0.0.1:43127",
    websocketFactory: (url) => {
      const socket = new FakeWebSocket(url);
      sockets.push(socket);
      return socket;
    },
  });

  client.subscribe("art_hook/instantiate", () => {});

  assert.equal(sockets[0].url, "ws://127.0.0.1:43127");
  client.dispose();
});

test("dispatches hook bridge payloads to matching desktop listeners", () => {
  const sockets: FakeWebSocket[] = [];
  const client = createHookBridgeBrowserClient({
    websocketFactory: (url) => {
      const socket = new FakeWebSocket(url);
      sockets.push(socket);
      return socket;
    },
  });

  const instantiatePayloads: unknown[] = [];
  const workflowUpdatedPayloads: unknown[] = [];

  client.subscribe("art_hook/instantiate", (payload) => {
    instantiatePayloads.push(payload);
  });
  client.subscribe("art_loom/workflow_updated", (payload) => {
    workflowUpdatedPayloads.push(payload);
  });

  assert.equal(sockets.length, 1);
  sockets[0].open();

  sockets[0].emitMessage({
    method: "art_hook/instantiate",
    params: {
      workflow_id: "hook-live",
      mode: "reference",
    },
  });
  sockets[0].emitMessage({
    method: "art_loom/workflow_updated",
    params: {
      workflowId: "hook-live",
      nodeId: "prompt",
    },
  });
  sockets[0].emitMessage({
    type: "success",
    data: {
      subscribed: true,
    },
  });

  assert.deepEqual(instantiatePayloads, [
    {
      workflow_id: "hook-live",
      mode: "reference",
    },
  ]);
  assert.deepEqual(workflowUpdatedPayloads, [
    {
      workflowId: "hook-live",
      nodeId: "prompt",
    },
  ]);

  client.dispose();
});
