import assert from "node:assert/strict";
import test from "node:test";

import {
  buildHookWorkflowInstantiationGraph,
  edgeEndpoints,
  fitHookCanvas,
  getHookCanvasRefreshTrigger,
  getHookCanvasNodePresentation,
  keepNewestHookCanvasSnapshot,
  retainHookCanvasSelection,
  resolveHookCanvasPreviewUrl,
  type HookCanvasSnapshot,
} from "./hookCanvas.ts";
import { snapshot } from "./hookCanvas.test-fixtures.ts";

test("auto refresh waits for the daemon and retries when its online trigger changes", () => {
  assert.equal(
    getHookCanvasRefreshTrigger({
      connectionState: "offline",
      baseUrl: "http://127.0.0.1:8765",
      refreshVersion: 0,
    }),
    null,
  );

  const onlineTrigger = getHookCanvasRefreshTrigger({
    connectionState: "online",
    baseUrl: "http://127.0.0.1:8765",
    refreshVersion: 0,
  });
  assert.ok(onlineTrigger);
  assert.notEqual(
    onlineTrigger,
    getHookCanvasRefreshTrigger({
      connectionState: "online",
      baseUrl: "http://127.0.0.1:8765",
      refreshVersion: 1,
    }),
  );
});

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

test("restores a saved Hook canvas as a desktop instantiation graph", () => {
  const graph = buildHookWorkflowInstantiationGraph({
    ...snapshot,
    nodes: [
      snapshot.nodes[0],
      {
        id: "resize",
        kind: "art",
        label: "调整尺寸",
        artId: "resize-art",
        x: 640,
        y: 220,
        width: 260,
        height: 180,
        previewAvailable: false,
        previewUrl: null,
        status: "ready",
        minified: true,
        crop: null,
        opacity: 0.75,
        params: { width: 512 },
      },
    ],
    edges: [{
      id: "capture-resize",
      sourceNodeId: "capture",
      sourcePortId: "output_image",
      targetNodeId: "resize",
      targetPortId: "image",
    }],
  }, "http://127.0.0.1:8765");

  assert.deepEqual(graph.nodes[0], {
    id: "capture",
    type: "sticker",
    position: { x: 100, y: 200 },
    measured: { width: 500, height: 250 },
    data: {
      label: "截图节点",
      w: 500,
      h: 250,
      params: {},
      src: "http://127.0.0.1:8765/v1/hook-bridge/canvas/nodes/capture/preview",
      previewSrc: "http://127.0.0.1:8765/v1/hook-bridge/canvas/nodes/capture/preview",
      minified: false,
      opacityNormal: 1,
      opacityMini: 1,
    },
  });
  assert.deepEqual(graph.nodes[1], {
    id: "resize",
    type: "artNode",
    position: { x: 640, y: 220 },
    measured: { width: 260, height: 180 },
    data: {
      artId: "resize-art",
      label: "调整尺寸",
      w: 260,
      h: 180,
      params: { width: 512 },
      minified: true,
      opacityNormal: 0.75,
      opacityMini: 0.75,
    },
  });
  assert.deepEqual(graph.edges, [{
    id: "capture-resize",
    source: "capture",
    target: "resize",
    sourceHandle: "output_image",
    targetHandle: "image",
  }]);
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

test("art error nodes surface execution failure instead of reusing the preview image", () => {
  const presentation = getHookCanvasNodePresentation(
    {
      ...snapshot.nodes[0],
      id: "failed-art",
      kind: "art",
      artId: "cloud-upscale",
      status: "error",
      errorMessage: "额度不足（HTTP 402）",
      previewAvailable: true,
      previewUrl: "/v1/hook-bridge/canvas/nodes/failed-art/preview",
    },
    { hasResolvedPreview: true, previewFailed: false },
  );

  assert.deepEqual(presentation, {
    showPreviewImage: false,
    placeholderText: "执行失败",
    detailText: "额度不足（HTTP 402）",
    placeholderTone: "error",
  });
});

test("ready nodes still use preview unavailable when no preview can be rendered", () => {
  const presentation = getHookCanvasNodePresentation(
    {
      ...snapshot.nodes[0],
      id: "ready-art",
      kind: "art",
      artId: "cloud-upscale",
      status: "ready",
      previewAvailable: true,
      previewUrl: "/v1/hook-bridge/canvas/nodes/ready-art/preview",
    },
    { hasResolvedPreview: false, previewFailed: true },
  );

  assert.deepEqual(presentation, {
    showPreviewImage: false,
    placeholderText: "预览不可用",
    detailText: null,
    placeholderTone: "neutral",
  });
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

test("projects daemon-supplied world edge points through the current layout", () => {
  const graph: HookCanvasSnapshot = {
    ...snapshot,
    bounds: { x: 100, y: 200, width: 700, height: 250 },
    nodes: [
      { ...snapshot.nodes[0], id: "capture", x: 100, y: 200, width: 500, height: 250 },
      { ...snapshot.nodes[0], id: "art", kind: "art", artId: "resize", x: 700, y: 200, width: 100, height: 250 },
    ],
    edges: [{
      id: "edge",
      sourceNodeId: "capture",
      sourcePortId: null,
      sourcePoint: { x: 606, y: 325 },
      targetNodeId: "art",
      targetPortId: null,
      targetPoint: { x: 694, y: 325 },
    }],
  };
  const layout = fitHookCanvas(graph, {
    width: 1000,
    height: 620,
    padding: 32,
    minimumNodeSize: 24,
  });
  const endpoints = edgeEndpoints(layout, graph.edges[0]);
  assert.deepEqual(endpoints, {
    source: { x: 708.5942857142858, y: 310 },
    target: { x: 826.2628571428571, y: 310 },
  });
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
