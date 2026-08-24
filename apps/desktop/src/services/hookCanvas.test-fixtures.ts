import type { HookCanvasSnapshot } from "./hookCanvas.ts";

export const snapshot: HookCanvasSnapshot = {
  available: true,
  revision: "rev-1",
  updatedAt: "1",
  workflowId: "hook-live",
  bounds: { x: 100, y: 200, width: 500, height: 250 },
  nodes: [{
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
  }],
  edges: [],
  warnings: [],
};

export const pipeline: HookCanvasSnapshot = {
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

export const ioNode = (id: string): HookCanvasSnapshot["nodes"][number] => ({
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

export const ioSnapshot = (
  nodes: string[],
  edges: Array<[string, string]>,
): HookCanvasSnapshot => ({
  available: true,
  revision: "io",
  updatedAt: null,
  workflowId: null,
  bounds: { x: 0, y: 0, width: 0, height: 0 },
  nodes: nodes.map(ioNode),
  edges: edges.map(([source, target]) => ({
    id: `${source}-${target}`,
    sourceNodeId: source,
    sourcePortId: "output_image",
    targetNodeId: target,
    targetPortId: "image",
  })),
  warnings: [],
});

export const artNode = (
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
