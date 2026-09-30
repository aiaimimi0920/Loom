import assert from "node:assert/strict";
import test from "node:test";
import { listProjectionPeers, parseProjectionPeerState, probeProjectionPeer, removeProjectionPeer, saveProjectionPeer } from "./projectionPeers.ts";
import { deleteJson } from "./transport.ts";

const peer = { peerId: "loom-" + "a".repeat(64), publicKey: "a".repeat(43) + "=", name: "PC3", origin: "https://pc3.example.test", enabled: true };
const state = { revision: 4, identity: { peerId: "loom-" + "b".repeat(64), publicKey: peer.publicKey }, peers: [peer] };

test("peer mutations preserve revision and identity; delete transmits JSON body", async (context) => {
  const requests: { path: string; method: string; body: unknown }[] = [];
  context.mock.method(globalThis, "fetch", async (url: string, options: RequestInit = {}) => {
    requests.push({ path: new URL(url).pathname, method: options.method ?? "GET", body: options.body ? JSON.parse(String(options.body)) : null });
    return Response.json(state);
  });
  assert.deepEqual(await listProjectionPeers("http://localhost:8765"), state);
  await saveProjectionPeer("http://localhost:8765", 4, peer);
  await removeProjectionPeer("http://localhost:8765", 4, peer.peerId);
  await deleteJson("http://localhost:8765", "/v1/old-delete");
  assert.deepEqual(requests, [
    { path: "/v1/projection-peers", method: "GET", body: null },
    { path: "/v1/projection-peers", method: "PUT", body: { expectedRevision: 4, peer } },
    { path: "/v1/projection-peers", method: "DELETE", body: { expectedRevision: 4, peerId: peer.peerId } },
    { path: "/v1/old-delete", method: "DELETE", body: null },
  ]);
});

test("a stale or different probe cannot be presented as verified", async (context) => {
  let response = { verified: true, peerId: peer.peerId, revision: 4 };
  context.mock.method(globalThis, "fetch", async () => Response.json(response));
  await probeProjectionPeer("http://localhost:8765", 4, peer.peerId);
  response = { ...response, revision: 5 };
  await assert.rejects(probeProjectionPeer("http://localhost:8765", 4, peer.peerId));
  response = { ...response, revision: 4, peerId: "other" };
  await assert.rejects(probeProjectionPeer("http://localhost:8765", 4, peer.peerId));
});

test("malformed, oversized and duplicate peer settings fail closed", () => {
  for (const input of [null, { ...state, revision: -1 }, { ...state, peers: Array(17).fill(peer) },
    { ...state, peers: [peer, peer] }, { ...state, peers: [{ ...peer, enabled: "yes" }] },
    { ...state, peers: [{ ...peer, name: "测".repeat(43) }] }]) {
    assert.throws(() => parseProjectionPeerState(input));
  }
});

test("revision conflicts propagate without automatic retry", async (context) => {
  let calls = 0;
  context.mock.method(globalThis, "fetch", async () => {
    calls++;
    return Response.json({ error: { message: "peer_revision_conflict" } }, { status: 409 });
  });
  await assert.rejects(saveProjectionPeer("http://localhost:8765", 4, peer), /peer_revision_conflict/);
  assert.equal(calls, 1);
});
