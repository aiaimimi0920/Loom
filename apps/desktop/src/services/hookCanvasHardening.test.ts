import assert from "node:assert/strict";
import test from "node:test";

import {
  buildSubWorkflowYaml,
  buildWorkflowArtBundle,
  connectedNodeIds,
  edgeWorldEndpoints,
  fitViewport,
  scaleToSliderValue,
  sliderValueToScale,
  type HookCanvasNode,
  type HookCanvasSnapshot,
} from "./hookCanvas.ts";

function canvasNode(id: string, workflowNodeId = id): HookCanvasNode {
  return {
    id,
    workflowNodeId,
    upstreamWorkflowNodeIds: [],
    kind: "art",
    label: id,
    artId: `art-${id}`,
    x: 0,
    y: 0,
    width: 80,
    height: 60,
    previewAvailable: false,
    previewUrl: null,
    status: "ready",
    minified: false,
    crop: null,
    opacity: 1,
  };
}

function canvasSnapshot(
  nodes: HookCanvasNode[],
  edges: HookCanvasSnapshot["edges"] = [],
): HookCanvasSnapshot {
  return {
    available: true,
    revision: "hardening",
    updatedAt: null,
    workflowId: "hardening",
    bounds: { x: 0, y: 0, width: 100, height: 100 },
    nodes,
    edges,
    warnings: [],
  };
}

test("workflow Art bundles never persist secret parameter values as defaults or YAML", () => {
  const secret = "plaintext-secret-sentinel";
  const snapshot = canvasSnapshot([canvasNode("raw-secret", "secret-node")]);
  const { yaml, tool } = buildWorkflowArtBundle({
    snapshot,
    workflowId: "secret-workflow",
    workflowName: "Secret workflow",
    params: [
      {
        workflowNodeId: "secret-node",
        target: "api_key",
        label: "API key",
        uiType: "string",
        executionType: "text",
        defaultValue: "default-secret-sentinel",
        secret: true,
      },
      {
        workflowNodeId: "secret-node",
        target: "hidden_token",
        label: "Hidden token",
        uiType: "string",
        executionType: "text",
        secret: true,
      },
    ],
    exposed: new Set(["secret-node::api_key"]),
    values: {
      "secret-node::api_key": secret,
      "secret-node::hidden_token": secret,
    },
  });

  assert.doesNotMatch(yaml, /plaintext-secret-sentinel|default-secret-sentinel/);
  assert.doesNotMatch(JSON.stringify(tool), /plaintext-secret-sentinel|default-secret-sentinel/);
  const secretParam = (tool.params as Array<Record<string, unknown>>).find(
    (param) => param.name === "api_key",
  );
  assert.equal(secretParam?.secret, true);
  assert.equal(Object.hasOwn(secretParam ?? {}, "default"), false);
});

test("workflow exports reject unsafe expression identifiers before serialization", () => {
  const unsafeMetadata = canvasSnapshot([canvasNode("raw", "bad: injected")]);
  assert.throws(
    () => buildSubWorkflowYaml(unsafeMetadata, new Set(["raw"]), "unsafe"),
    /Invalid workflow node id/,
  );

  const source = canvasNode("raw-source", "source");
  const target = { ...canvasNode("raw-target", "target"), upstreamWorkflowNodeIds: ["source"] };
  const unsafePort = canvasSnapshot([source, target], [{
    id: "unsafe-port",
    sourceNodeId: source.id,
    sourcePortId: "output }} malicious",
    targetNodeId: target.id,
    targetPortId: "image",
  }]);
  assert.throws(
    () => buildWorkflowArtBundle({
      snapshot: unsafePort,
      workflowId: "unsafe-port",
      workflowName: "unsafe",
      params: [],
      exposed: new Set(),
      values: {},
    }),
    /Invalid source port id/,
  );
});

test("workflow exports reject duplicate daemon node identifiers", () => {
  const duplicateMetadata = canvasSnapshot([
    canvasNode("raw-a", "duplicate-node"),
    canvasNode("raw-b", "duplicate-node"),
  ]);

  assert.throws(
    () => buildSubWorkflowYaml(duplicateMetadata, new Set(["raw-a", "raw-b"]), "duplicate"),
    /Duplicate workflow node id: duplicate-node/,
  );
  assert.throws(
    () => buildWorkflowArtBundle({
      snapshot: duplicateMetadata,
      workflowId: "duplicate",
      workflowName: "duplicate",
      params: [],
      exposed: new Set(),
      values: {},
    }),
    /Duplicate workflow node id: duplicate-node/,
  );
});

test("connected component traversal remains correct for a large linear canvas", () => {
  const nodeCount = 10_000;
  const nodes = Array.from({ length: nodeCount }, (_, index) => canvasNode(`node-${index}`));
  const edges = Array.from({ length: nodeCount - 1 }, (_, index) => ({
    id: `edge-${index}`,
    sourceNodeId: `node-${index}`,
    sourcePortId: "output_image",
    targetNodeId: `node-${index + 1}`,
    targetPortId: "image",
  }));

  const connected = connectedNodeIds(canvasSnapshot(nodes, edges), "node-0");
  assert.equal(connected.size, nodeCount);
  assert.equal(connected.has(`node-${nodeCount - 1}`), true);
});

test("world edge fallback preserves Hook link-gap geometry", () => {
  const source = { ...canvasNode("source"), x: 10, y: 20, width: 80, height: 60 };
  const target = { ...canvasNode("target"), x: 200, y: 40, width: 100, height: 80, minified: true };
  const edge = {
    id: "fallback",
    sourceNodeId: source.id,
    sourcePortId: null,
    targetNodeId: target.id,
    targetPortId: null,
  };
  assert.deepEqual(edgeWorldEndpoints(canvasSnapshot([source, target], [edge]), edge), {
    source: { x: 96, y: 50 },
    target: { x: 196, y: 80 },
  });
});

test("viewport and slider helpers return finite values for degenerate inputs", () => {
  const viewport = fitViewport(canvasSnapshot([]), Number.NaN, -1, Number.POSITIVE_INFINITY);
  assert.equal(viewport.scale > 0, true);
  assert.equal(Number.isFinite(viewport.offsetX), true);
  assert.equal(Number.isFinite(viewport.offsetY), true);
  assert.equal(scaleToSliderValue(1, 1, 1), 0);
  assert.equal(sliderValueToScale(500, 1, 1), 1);
  assert.equal(Number.isFinite(scaleToSliderValue(Number.NaN, -1, 6, 0)), true);
  assert.equal(Number.isFinite(sliderValueToScale(Number.NaN, -1, 6, 0)), true);
});
