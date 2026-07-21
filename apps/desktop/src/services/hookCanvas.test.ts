import assert from "node:assert/strict";
import test from "node:test";

import {
  edgeEndpoints,
  fitHookCanvas,
  keepNewestHookCanvasSnapshot,
  retainHookCanvasSelection,
  resolveHookCanvasPreviewUrl,
  type HookCanvasSnapshot,
} from "./hookCanvas.ts";

const snapshot: HookCanvasSnapshot = {
  available: true,
  revision: "rev-1",
  updatedAt: "1",
  workflowId: "hook-live",
  bounds: { x: 100, y: 200, width: 500, height: 250 },
  nodes: [
    {
      id: "capture",
      kind: "screenshot",
      label: "截图节点",
      artId: null,
      x: 100,
      y: 200,
      width: 500,
      height: 250,
      previewAvailable: true,
      previewUrl: "/v1/hook-bridge/canvas/nodes/capture/preview",
      status: "ready",
    },
  ],
  edges: [],
  warnings: [],
};

test("fits Hook nodes into a stable virtual viewport", () => {
  const layout = fitHookCanvas(snapshot, {
    width: 1000,
    height: 620,
    padding: 32,
    minimumNodeSize: 24,
  });
  assert.equal(layout.nodes[0].x, 32);
  assert.equal(layout.nodes[0].y, 76);
  assert.equal(layout.nodes[0].width, 936);
  assert.equal(layout.nodes[0].height, 468);
});

test("keeps the previous object when revision is unchanged", () => {
  assert.equal(keepNewestHookCanvasSnapshot(snapshot, { ...snapshot }), snapshot);
});

test("replaces the previous object when revision changes", () => {
  const next = { ...snapshot, revision: "rev-2" };
  assert.equal(keepNewestHookCanvasSnapshot(snapshot, next), next);
});

test("retains the last valid snapshot when the daemon reports an unavailable session", () => {
  const unavailable = {
    ...snapshot,
    available: false,
    revision: "missing",
    nodes: [],
    edges: [],
  };
  assert.equal(keepNewestHookCanvasSnapshot(snapshot, unavailable), snapshot);
  assert.equal(keepNewestHookCanvasSnapshot(null, unavailable), unavailable);
});

test("resolves preview paths against the daemon origin", () => {
  assert.equal(
    resolveHookCanvasPreviewUrl("http://127.0.0.1:8765/", snapshot.nodes[0]),
    "http://127.0.0.1:8765/v1/hook-bridge/canvas/nodes/capture/preview",
  );
});

test("does not resolve unavailable previews", () => {
  assert.equal(
    resolveHookCanvasPreviewUrl("http://127.0.0.1:8765/", {
      ...snapshot.nodes[0],
      previewAvailable: false,
    }),
    null,
  );
});

test("resolves edge endpoints from fitted node centers", () => {
  const target = {
    ...snapshot.nodes[0],
    id: "art",
    x: 700,
    width: 100,
  };
  const graph = {
    ...snapshot,
    bounds: { x: 100, y: 200, width: 700, height: 250 },
    nodes: [snapshot.nodes[0], target],
    edges: [{
      id: "edge",
      sourceNodeId: "capture",
      sourcePortId: null,
      targetNodeId: "art",
      targetPortId: null,
    }],
  };
  const layout = fitHookCanvas(graph, {
    width: 1000,
    height: 620,
    padding: 32,
    minimumNodeSize: 24,
  });
  const endpoints = edgeEndpoints(layout, graph.edges[0]);
  assert.ok(endpoints);
  assert.equal(Number.isFinite(endpoints.source.x), true);
  assert.equal(Number.isFinite(endpoints.target.x), true);
  assert.equal(endpoints.source.x < endpoints.target.x, true);
});

test("missing edge nodes and stale selections degrade to null", () => {
  const layout = fitHookCanvas(snapshot, {
    width: 1000,
    height: 620,
    padding: 32,
    minimumNodeSize: 24,
  });
  assert.equal(
    edgeEndpoints(layout, {
      id: "missing",
      sourceNodeId: "capture",
      sourcePortId: null,
      targetNodeId: "missing",
      targetPortId: null,
    }),
    null,
  );
  assert.equal(retainHookCanvasSelection("missing", snapshot), null);
  assert.equal(retainHookCanvasSelection("capture", snapshot), "capture");
});

test("empty and degenerate bounds never produce NaN", () => {
  const empty = fitHookCanvas(
    { ...snapshot, bounds: { x: 0, y: 0, width: 0, height: 0 }, nodes: [] },
    { width: 1000, height: 620, padding: 32, minimumNodeSize: 24 },
  );
  assert.deepEqual(empty.nodes, []);
  assert.equal(Number.isFinite(empty.scale), true);
});

test("negative coordinates preserve relative placement and minimum size", () => {
  const graph = {
    ...snapshot,
    bounds: { x: -200, y: -100, width: 400, height: 200 },
    nodes: [{ ...snapshot.nodes[0], x: -200, y: -100, width: 1, height: 1 }],
  };
  const layout = fitHookCanvas(graph, {
    width: 1000,
    height: 620,
    padding: 32,
    minimumNodeSize: 24,
  });
  assert.equal(layout.nodes[0].x, 32);
  assert.equal(layout.nodes[0].y, 76);
  assert.equal(layout.nodes[0].width, 24);
  assert.equal(layout.nodes[0].height, 24);
});
