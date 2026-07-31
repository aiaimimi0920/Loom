import assert from "node:assert/strict";
import test from "node:test";

import {
  buildSubWorkflowYaml,
  buildWorkflowArtBundle,
  connectedNodeIds,
  edgeEndpoints,
  edgeWorldEndpoints,
  fitHookCanvas,
  fitViewport,
  getHookCanvasRefreshTrigger,
  getHookCanvasNodePresentation,
  isEdgeHighlighted,
  inferCanvasWorkflowInterface,
  keepNewestHookCanvasSnapshot,
  listCanvasWorkflows,
  readCanvasWorkflowSnapshot,
  retainHookCanvasSelection,
  resolveHookCanvasPreviewUrl,
  scaleToSliderValue,
  sliderValueToScale,
  viewportLayout,
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
      minified: false,
      crop: null,
      opacity: 1,
    },
  ],
  edges: [],
  warnings: [],
};

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

const pipeline: HookCanvasSnapshot = {
  available: true,
  revision: "rev-pipe",
  updatedAt: "1",
  workflowId: "hook-live",
  bounds: { x: 0, y: 0, width: 800, height: 400 },
  nodes: [
    { id: "a", kind: "screenshot", label: "截图", artId: null, x: 0, y: 0, width: 80, height: 80, previewAvailable: false, previewUrl: null, status: "ready", minified: false, crop: null, opacity: 1 },
    { id: "b", kind: "art", label: "Art", artId: "resize", x: 200, y: 0, width: 80, height: 80, previewAvailable: false, previewUrl: null, status: "ready", minified: false, crop: null, opacity: 1 },
    { id: "c", kind: "art", label: "Art", artId: "ocr", x: 400, y: 0, width: 80, height: 80, previewAvailable: false, previewUrl: null, status: "ready", minified: false, crop: null, opacity: 1 },
    { id: "lonely", kind: "screenshot", label: "孤立", artId: null, x: 600, y: 300, width: 80, height: 80, previewAvailable: false, previewUrl: null, status: "ready", minified: false, crop: null, opacity: 1 },
  ],
  edges: [
    { id: "e1", sourceNodeId: "a", sourcePortId: null, targetNodeId: "b", targetPortId: null },
    { id: "e2", sourceNodeId: "b", sourcePortId: null, targetNodeId: "c", targetPortId: null },
  ],
  warnings: [],
};

test("connectedNodeIds returns the whole component reachable in either direction", () => {
  const fromMiddle = connectedNodeIds(pipeline, "b");
  assert.deepEqual([...fromMiddle].sort(), ["a", "b", "c"]);
  const fromEnd = connectedNodeIds(pipeline, "c");
  assert.deepEqual([...fromEnd].sort(), ["a", "b", "c"]);
});

test("connectedNodeIds prefers daemon component ids when present", () => {
  const withComponents: HookCanvasSnapshot = {
    ...pipeline,
    nodes: [
      { ...pipeline.nodes[0], componentId: "group-1" },
      { ...pipeline.nodes[1], componentId: "group-1" },
      { ...pipeline.nodes[2], componentId: "group-1" },
      { ...pipeline.nodes[3], componentId: "group-2" },
    ],
    edges: [],
  };

  assert.deepEqual([...connectedNodeIds(withComponents, "b")].sort(), ["a", "b", "c"]);
  assert.deepEqual([...connectedNodeIds(withComponents, "lonely")], ["lonely"]);
});

test("edgeWorldEndpoints consumes daemon geometry when present", () => {
  const edge = {
    id: "edge-1",
    sourceNodeId: "a",
    sourcePortId: null,
    sourcePoint: { x: 86, y: 40 },
    targetNodeId: "b",
    targetPortId: null,
    targetPoint: { x: 194, y: 40 },
  };

  assert.deepEqual(edgeWorldEndpoints(pipeline, edge), {
    source: { x: 86, y: 40 },
    target: { x: 194, y: 40 },
  });
});

test("connectedNodeIds isolates a node with no edges", () => {
  assert.deepEqual([...connectedNodeIds(pipeline, "lonely")], ["lonely"]);
});

test("connectedNodeIds returns empty for null or unknown node", () => {
  assert.equal(connectedNodeIds(pipeline, null).size, 0);
  assert.equal(connectedNodeIds(pipeline, "missing").size, 0);
});

test("isEdgeHighlighted only when both endpoints are in the set", () => {
  const set = connectedNodeIds(pipeline, "a");
  assert.equal(isEdgeHighlighted(pipeline.edges[0], set), true);
  const partial = new Set(["a"]);
  assert.equal(isEdgeHighlighted(pipeline.edges[0], partial), false);
});

test("buildSubWorkflowYaml emits members with upstream needs and drops outside edges", () => {
  const ids = connectedNodeIds(pipeline, "a");
  const yaml = buildSubWorkflowYaml(pipeline, ids, "my-pipe");
  assert.match(yaml, /name: 'my-pipe'/);
  assert.match(yaml, /uses: 'resize'/);
  assert.match(yaml, /uses: 'ocr'/);
  assert.ok(!yaml.includes("lonely"));
});

test("buildSubWorkflowYaml prefers daemon workflow export metadata when present", () => {
  const daemonSnapshot: HookCanvasSnapshot = {
    ...pipeline,
    nodes: [
      {
        ...pipeline.nodes[0],
        componentId: "pipe-1",
        workflowNodeId: "capture",
        upstreamWorkflowNodeIds: [],
      },
      {
        ...pipeline.nodes[1],
        componentId: "pipe-1",
        workflowNodeId: "resize",
        upstreamWorkflowNodeIds: ["capture"],
      },
      {
        ...pipeline.nodes[2],
        componentId: "pipe-1",
        workflowNodeId: "resize-2",
        upstreamWorkflowNodeIds: ["resize"],
      },
      {
        ...pipeline.nodes[3],
        componentId: "pipe-2",
        workflowNodeId: "lonely",
        upstreamWorkflowNodeIds: [],
      },
    ],
  };

  const yaml = buildSubWorkflowYaml(daemonSnapshot, connectedNodeIds(daemonSnapshot, "a"), "pipe");
  assert.match(yaml, /name: 'pipe'/);
  assert.match(yaml, /- id: capture/);
  assert.match(yaml, /- id: resize/);
  assert.match(yaml, /- id: resize-2/);
  assert.match(yaml, /needs: \[capture\]/);
  assert.match(yaml, /needs: \[resize\]/);
  assert.ok(!yaml.includes("lonely"));
});

test("buildSubWorkflowYaml quotes YAML-sensitive names and tool ids", () => {
  const tricky: HookCanvasSnapshot = {
    ...pipeline,
    nodes: [
      { ...pipeline.nodes[0], workflowNodeId: "capture", upstreamWorkflowNodeIds: [] },
      {
        ...pipeline.nodes[1],
        artId: "resize:smart's",
        workflowNodeId: "resize",
        upstreamWorkflowNodeIds: ["capture"],
      },
      { ...pipeline.nodes[2], workflowNodeId: "ocr", upstreamWorkflowNodeIds: ["resize"] },
      { ...pipeline.nodes[3], workflowNodeId: "lonely", upstreamWorkflowNodeIds: [] },
    ],
  };

  const yaml = buildSubWorkflowYaml(
    tricky,
    connectedNodeIds(tricky, "a"),
    "Hook: Export's",
  );
  assert.match(yaml, /name: 'Hook: Export''s'/);
  assert.match(yaml, /uses: 'resize:smart''s'/);
});

test("fitViewport centers the whole snapshot into the surface", () => {
  const vp = fitViewport(pipeline, 1000, 620, 40);
  assert.ok(vp.scale > 0);
  assert.ok(Number.isFinite(vp.offsetX));
  assert.ok(Number.isFinite(vp.offsetY));
});

test("viewportLayout maps world coordinates through scale and pan offset", () => {
  const vp = { scale: 2, offsetX: 100, offsetY: 50 };
  const layout = viewportLayout(pipeline, vp);
  const first = layout.nodes[0];
  assert.equal(first.x, (0 - 100) * 2);
  assert.equal(first.y, (0 - 50) * 2);
  assert.equal(first.width, 80 * 2);
});

test("slider mapping round-trips scale within the zoom range", () => {
  const value = scaleToSliderValue(1, 0.05, 6);
  const scale = sliderValueToScale(value, 0.05, 6);
  assert.ok(Math.abs(scale - 1) < 0.01);
  assert.equal(scaleToSliderValue(0.05, 0.05, 6), 0);
  assert.equal(scaleToSliderValue(6, 0.05, 6), 1000);
});

const ioNode = (id: string): HookCanvasSnapshot["nodes"][number] => ({
  id,
  kind: "screenshot",
  label: id,
  artId: null,
  x: 0,
  y: 0,
  width: 80,
  height: 80,
  previewAvailable: false,
  previewUrl: null,
  status: "ready",
  minified: false,
  crop: null,
  opacity: 1,
});

const ioSnapshot = (
  nodes: string[],
  edges: Array<[string, string]>,
): HookCanvasSnapshot => ({
  available: true,
  revision: "io",
  updatedAt: null,
  workflowId: null,
  bounds: { x: 0, y: 0, width: 0, height: 0 },
  nodes: nodes.map(ioNode),
  edges: edges.map(([s, t]) => ({
    id: `${s}-${t}`,
    sourceNodeId: s,
    sourcePortId: "output_image",
    targetNodeId: t,
    targetPortId: "image",
  })),
  warnings: [],
});

test("infers linear-chain workflow interface as one input and one output", () => {
  const io = inferCanvasWorkflowInterface(ioSnapshot(["a", "b", "c"], [["a", "b"], ["b", "c"]]));
  assert.deepEqual(io.inputs.map((p) => p.nodeId), ["a"]);
  assert.deepEqual(io.outputs.map((p) => p.nodeId), ["c"]);
  assert.equal(io.inputs[0].portId, "image");
  assert.equal(io.outputs[0].portId, "output_image");
});

test("infers multi-source and multi-sink workflow interface", () => {
  // a,b -> c -> d,e (two sources, two sinks)
  const io = inferCanvasWorkflowInterface(
    ioSnapshot(["a", "b", "c", "d", "e"], [["a", "c"], ["b", "c"], ["c", "d"], ["c", "e"]]),
  );
  assert.deepEqual(io.inputs.map((p) => p.nodeId).sort(), ["a", "b"]);
  assert.deepEqual(io.outputs.map((p) => p.nodeId).sort(), ["d", "e"]);
});

test("treats an isolated single node as both input and output", () => {
  const io = inferCanvasWorkflowInterface(ioSnapshot(["solo"], []));
  assert.deepEqual(io.inputs.map((p) => p.nodeId), ["solo"]);
  assert.deepEqual(io.outputs.map((p) => p.nodeId), ["solo"]);
});

const artNode = (
  id: string,
  workflowNodeId: string,
  artId: string,
  needs: string[],
  params: Record<string, unknown>,
): HookCanvasSnapshot["nodes"][number] => ({
  id,
  componentId: "c",
  workflowNodeId,
  upstreamWorkflowNodeIds: needs,
  kind: "art",
  label: id,
  artId,
  x: 0,
  y: 0,
  width: 80,
  height: 80,
  previewAvailable: false,
  previewUrl: null,
  status: "ready",
  minified: false,
  crop: null,
  opacity: 1,
  params,
});

test("buildWorkflowArtBundle exposes selected params and bakes the rest into with", () => {
  const snapshot: HookCanvasSnapshot = {
    available: true,
    revision: "art",
    updatedAt: null,
    workflowId: null,
    bounds: { x: 0, y: 0, width: 0, height: 0 },
    nodes: [
      artNode("raw-a", "a", "resize", [], { width: 512, mode: "fit" }),
      artNode("raw-b", "b", "ocr", ["a"], { lang: "en" }),
    ],
    edges: [
      { id: "e1", sourceNodeId: "raw-a", sourcePortId: "output_image", targetNodeId: "raw-b", targetPortId: "image" },
    ],
    warnings: [],
  };
  const params = [
    { workflowNodeId: "a", target: "width", label: "A / 宽度", uiType: "number", executionType: "int" },
    { workflowNodeId: "a", target: "mode", label: "A / 模式", uiType: "string", executionType: "text" },
    { workflowNodeId: "b", target: "lang", label: "B / 语言", uiType: "string", executionType: "text" },
  ];
  const exposed = new Set(["a::width", "b::lang"]);
  const values = { "a::width": "512", "a::mode": "fit", "b::lang": "en" };

  const { yaml, tool } = buildWorkflowArtBundle({
    snapshot,
    workflowId: "wf-1",
    workflowName: "链",
    params,
    exposed,
    values,
  });

  // Unexposed param (a.mode) is baked into with; exposed ones are not.
  assert.match(yaml, /mode: fit/);
  assert.ok(!yaml.includes("width: 512"), "exposed width must not be baked as constant");
  assert.ok(!/lang: en/.test(yaml), "exposed lang must not be baked as constant");

  const bindings = tool.execution?.workflowBindings as {
    inputs: Array<{ nodeId: string; target: string; kind: string }>;
    primaryOutput?: { nodeId: string; output: string };
  };
  // Image input binding uses the workflowNodeId of the source node ("a").
  const imageBinding = bindings.inputs.find((b) => b.kind === "input_image");
  assert.equal(imageBinding?.nodeId, "a");
  assert.equal(imageBinding?.target, "image");
  // Param bindings for the two exposed params, keyed by workflowNodeId.
  const paramBindings = bindings.inputs.filter((b) => b.kind === "param");
  assert.deepEqual(
    paramBindings.map((b) => `${b.nodeId}::${b.target}`).sort(),
    ["a::width", "b::lang"],
  );
  // primaryOutput points at the terminal node (b) via workflowNodeId.
  assert.equal(bindings.primaryOutput?.nodeId, "b");
  // Image input goes in `inputs` (1), exposed params go in `params` (2) — Loom's
  // artloom_compat_art_json maps tool.params → Hook's ArtParameter list.
  assert.equal(tool.inputs?.length, 1);
  assert.equal(tool.params?.length, 2);
  assert.equal(tool.id, "hook-wf-wf-1");
  // ArtLoom-compat metadata so it surfaces in Hook's Art node list.
  const metadata = tool.metadata as { artloomCompat?: { source?: string } };
  assert.equal(metadata.artloomCompat?.source, "artloom-compat");
  // Every param carries id + widget so Hook's ArtParameter can deserialize it.
  for (const param of tool.params as Array<{ id?: string; widget?: string }>) {
    assert.ok(param.id, "param must have id");
    assert.ok(param.widget, "param must have widget");
  }
});
