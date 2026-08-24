import assert from "node:assert/strict";
import test from "node:test";

import type { HookCanvasNode, HookCanvasSnapshot } from "./hookCanvas.ts";
import type { LoomToolDefinition } from "./loomApi.ts";
import {
  buildExposableParams,
  buildNodeParamGroups,
} from "../components/hook/hookCanvasThumbnailModel.ts";
import {
  createMinimapProjection,
  viewportFromSurfaceDrag,
  worldPointFromMinimap,
} from "../components/hook/useHookCanvasViewport.ts";

function node(id: string): HookCanvasNode {
  return {
    id,
    workflowNodeId: id,
    upstreamWorkflowNodeIds: [],
    kind: "art",
    label: id,
    artId: "local/test-art",
    x: 100,
    y: 80,
    width: 120,
    height: 90,
    previewAvailable: false,
    previewUrl: null,
    status: "ready",
    minified: false,
    crop: null,
    opacity: 1,
    params: { strength: 0.75, enabled: true },
  };
}

function snapshot(nodes: HookCanvasNode[]): HookCanvasSnapshot {
  return {
    available: true,
    revision: "thumbnail-hardening",
    updatedAt: null,
    workflowId: "thumbnail-hardening",
    bounds: { x: 0, y: 0, width: 600, height: 400 },
    nodes,
    edges: [],
    warnings: [],
  };
}

test("surface drag ignores pointer jitter until the drag threshold is crossed", () => {
  const drag = {
    startClientX: 10,
    startClientY: 10,
    origin: { scale: 1, offsetX: 20, offsetY: 30 },
    rectWidth: 1000,
    rectHeight: 620,
  };
  assert.equal(viewportFromSurfaceDrag(drag, 12, 12), null);
  assert.deepEqual(viewportFromSurfaceDrag(drag, 20, 10), {
    scale: 1,
    offsetX: 10,
    offsetY: 30,
  });
});

test("minimap projection round-trips world points and stays finite for empty bounds", () => {
  const projected = createMinimapProjection(snapshot([node("one")]), {
    scale: 1.5,
    offsetX: 50,
    offsetY: 25,
  });
  const world = { x: 210, y: 145 };
  const mapped = projected.toMap(world.x, world.y);
  const restored = worldPointFromMinimap(
    projected,
    { left: 0, top: 0, width: 200, height: 130 },
    mapped.x,
    mapped.y,
  );
  assert.ok(restored);
  assert.ok(Math.abs(restored.worldX - world.x) < 0.001);
  assert.ok(Math.abs(restored.worldY - world.y) < 0.001);

  const empty = createMinimapProjection(snapshot([]), {
    scale: 1,
    offsetX: 0,
    offsetY: 0,
  });
  assert.equal(Number.isFinite(empty.scale), true);
  assert.equal(Number.isFinite(empty.viewRect.x), true);
  assert.equal(Number.isFinite(empty.viewRect.w), true);
});

test("thumbnail parameter model preserves live values and Art exposure metadata", () => {
  const tools: LoomToolDefinition[] = [{
    id: "local/test-art",
    name: "Test Art",
    params: [
      { id: "strength", name: "strength", label: "Strength", uiType: "float", default: 0.25, step: 0.05 },
      {
        id: "enabled",
        name: "enabled",
        label: "Enabled",
        widget: "checkbox",
        default: false,
        secret: true,
      },
    ],
  }];
  const groups = buildNodeParamGroups(snapshot([node("one")]), tools);
  assert.equal(groups.length, 1);
  assert.equal(groups[0].rows[0].currentValue, "0.75");
  assert.equal(groups[0].rows[1].currentValue, "true");
  const params = buildExposableParams(groups);
  assert.equal(params[0].workflowNodeId, "one");
  assert.equal(params[1].secret, true);
  assert.equal(params[1].executionType, "bool");
});
