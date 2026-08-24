import assert from "node:assert/strict";
import test from "node:test";

import {
  fitViewport,
  inferCanvasWorkflowInterface,
  scaleToSliderValue,
  sliderValueToScale,
  viewportLayout,
} from "./hookCanvas.ts";
import { ioSnapshot, pipeline } from "./hookCanvas.test-fixtures.ts";

test("fitViewport centers the whole snapshot into the surface", () => {
  const viewport = fitViewport(pipeline, 1000, 620, 40);
  assert.ok(viewport.scale > 0);
  assert.ok(Number.isFinite(viewport.offsetX));
  assert.ok(Number.isFinite(viewport.offsetY));
});

test("viewportLayout maps world coordinates through scale and pan offset", () => {
  const layout = viewportLayout(pipeline, { scale: 2, offsetX: 100, offsetY: 50 });
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

test("infers linear-chain workflow interface as one input and one output", () => {
  const io = inferCanvasWorkflowInterface(ioSnapshot(["a", "b", "c"], [["a", "b"], ["b", "c"]]));
  assert.deepEqual(io.inputs.map((port) => port.nodeId), ["a"]);
  assert.deepEqual(io.outputs.map((port) => port.nodeId), ["c"]);
  assert.equal(io.inputs[0].portId, "image");
  assert.equal(io.outputs[0].portId, "output_image");
});

test("infers multi-source and multi-sink workflow interface", () => {
  const io = inferCanvasWorkflowInterface(
    ioSnapshot(["a", "b", "c", "d", "e"], [["a", "c"], ["b", "c"], ["c", "d"], ["c", "e"]]),
  );
  assert.deepEqual(io.inputs.map((port) => port.nodeId).sort(), ["a", "b"]);
  assert.deepEqual(io.outputs.map((port) => port.nodeId).sort(), ["d", "e"]);
});

test("treats an isolated single node as both input and output", () => {
  const io = inferCanvasWorkflowInterface(ioSnapshot(["solo"], []));
  assert.deepEqual(io.inputs.map((port) => port.nodeId), ["solo"]);
  assert.deepEqual(io.outputs.map((port) => port.nodeId), ["solo"]);
});
