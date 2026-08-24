import assert from "node:assert/strict";
import test from "node:test";

import {
  buildSubWorkflowYaml,
  connectedNodeIds,
  edgeWorldEndpoints,
  isEdgeHighlighted,
  type HookCanvasSnapshot,
} from "./hookCanvas.ts";
import { pipeline } from "./hookCanvas.test-fixtures.ts";

test("connectedNodeIds returns the whole component reachable in either direction", () => {
  assert.deepEqual([...connectedNodeIds(pipeline, "b")].sort(), ["a", "b", "c"]);
  assert.deepEqual([...connectedNodeIds(pipeline, "c")].sort(), ["a", "b", "c"]);
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

test("isEdgeHighlighted requires both endpoints", () => {
  assert.equal(isEdgeHighlighted(pipeline.edges[0], connectedNodeIds(pipeline, "a")), true);
  assert.equal(isEdgeHighlighted(pipeline.edges[0], new Set(["a"])), false);
});

test("buildSubWorkflowYaml emits members with upstream needs and drops outside edges", () => {
  const yaml = buildSubWorkflowYaml(pipeline, connectedNodeIds(pipeline, "a"), "my-pipe");
  assert.match(yaml, /name: 'my-pipe'/);
  assert.match(yaml, /uses: 'sticker'/);
  assert.ok(!yaml.includes("uses: 'screenshot'"));
  assert.match(yaml, /uses: 'resize'/);
  assert.match(yaml, /uses: 'ocr'/);
  assert.ok(!yaml.includes("lonely"));
});

test("buildSubWorkflowYaml prefers daemon workflow export metadata", () => {
  const daemonSnapshot: HookCanvasSnapshot = {
    ...pipeline,
    nodes: [
      { ...pipeline.nodes[0], componentId: "pipe-1", workflowNodeId: "capture", upstreamWorkflowNodeIds: [] },
      { ...pipeline.nodes[1], componentId: "pipe-1", workflowNodeId: "resize", upstreamWorkflowNodeIds: ["capture"] },
      { ...pipeline.nodes[2], componentId: "pipe-1", workflowNodeId: "resize-2", upstreamWorkflowNodeIds: ["resize"] },
      { ...pipeline.nodes[3], componentId: "pipe-2", workflowNodeId: "lonely", upstreamWorkflowNodeIds: [] },
    ],
  };
  const yaml = buildSubWorkflowYaml(daemonSnapshot, connectedNodeIds(daemonSnapshot, "a"), "pipe");
  assert.match(yaml, /name: 'pipe'/);
  assert.match(yaml, /- id: capture/);
  assert.match(yaml, /uses: 'sticker'/);
  assert.ok(!yaml.includes("uses: 'screenshot'"));
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
      { ...pipeline.nodes[1], artId: "resize:smart's", workflowNodeId: "resize", upstreamWorkflowNodeIds: ["capture"] },
      { ...pipeline.nodes[2], workflowNodeId: "ocr", upstreamWorkflowNodeIds: ["resize"] },
      { ...pipeline.nodes[3], workflowNodeId: "lonely", upstreamWorkflowNodeIds: [] },
    ],
  };
  const yaml = buildSubWorkflowYaml(tricky, connectedNodeIds(tricky, "a"), "Hook: Export's");
  assert.match(yaml, /name: 'Hook: Export''s'/);
  assert.match(yaml, /uses: 'resize:smart''s'/);
});
