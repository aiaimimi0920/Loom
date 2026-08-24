import assert from "node:assert/strict";
import test from "node:test";

import { buildSubWorkflowYaml, buildWorkflowArtBundle, type HookCanvasSnapshot } from "./hookCanvas.ts";
import { artNode, ioNode, pipeline } from "./hookCanvas.test-fixtures.ts";
import { parseWorkflowYamlLite } from "./workflowStudio.ts";

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
    {
      workflowNodeId: "a",
      target: "width",
      label: "A / 宽度",
      uiType: "int",
      executionType: "number",
      widget: "slider",
      dataType: "number",
      defaultValue: 512,
      min: 64,
      max: 2048,
      step: 64,
      group: "尺寸",
    },
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

  assert.match(yaml, /mode: fit/);
  const workflow = parseWorkflowYamlLite(yaml);
  assert.equal(workflow.nodes.find((node) => node.id === "b")?.with.image, "${{ nodes.a.outputs.output_image }}");
  assert.ok(!yaml.includes("width: 512"), "exposed width must not be baked as constant");
  assert.ok(!/lang: en/.test(yaml), "exposed lang must not be baked as constant");

  const bindings = tool.execution?.workflowBindings as {
    inputs: Array<{ nodeId: string; target: string; kind: string }>;
    primaryOutput?: { nodeId: string; output: string };
  };
  const imageBinding = bindings.inputs.find((binding) => binding.kind === "input_image");
  assert.equal(imageBinding?.nodeId, "a");
  assert.equal(imageBinding?.target, "image");
  assert.deepEqual(
    bindings.inputs.filter((binding) => binding.kind === "param").map((binding) => `${binding.nodeId}::${binding.target}`).sort(),
    ["a::width", "b::lang"],
  );
  assert.equal(bindings.primaryOutput?.nodeId, "b");
  assert.equal(tool.inputs?.length, 1);
  assert.equal(tool.params?.length, 2);
  assert.equal(tool.id, "hook-wf-wf-1");
  const metadata = tool.metadata as { hookWorkflow?: { managedBy?: string } };
  assert.equal(metadata.hookWorkflow?.managedBy, "hook-workflow");
  for (const param of tool.params as Array<{ id?: string; widget?: string }>) {
    assert.ok(param.id, "param must have id");
    assert.ok(param.widget, "param must have widget");
  }
  assert.deepEqual((tool.params as Array<Record<string, unknown>>)[0], {
    id: "width",
    name: "width",
    label: "A / 宽度",
    widget: "slider",
    type: "int",
    executionType: "number",
    default: "512",
    data_type: "number",
    min: 64,
    max: 2048,
    step: 64,
    group: "尺寸",
  });
});

test("buildWorkflowArtBundle preserves target ports for multi-image edges", () => {
  const snapshot: HookCanvasSnapshot = {
    available: true,
    revision: "multi-image",
    updatedAt: null,
    workflowId: null,
    bounds: { x: 0, y: 0, width: 0, height: 0 },
    nodes: [
      { ...ioNode("raw-reference"), workflowNodeId: "reference-source", upstreamWorkflowNodeIds: [] },
      { ...ioNode("raw-input"), workflowNodeId: "input-source", upstreamWorkflowNodeIds: [] },
      artNode("raw-color", "color", "neuro.official/custom-1770131241684", ["input-source", "reference-source"], {}),
      artNode("raw-compress", "compress", "neuro.official/custom-1770146354922", ["color"], {}),
    ],
    edges: [
      {
        id: "input-color",
        sourceNodeId: "raw-input",
        sourcePortId: "output",
        targetNodeId: "raw-color",
        targetPortId: "input",
      },
      {
        id: "reference-color",
        sourceNodeId: "raw-reference",
        sourcePortId: "output_image",
        targetNodeId: "raw-color",
        targetPortId: "reference",
      },
      {
        id: "color-compress",
        sourceNodeId: "raw-color",
        sourcePortId: "output",
        targetNodeId: "raw-compress",
        targetPortId: "input",
      },
    ],
    warnings: [],
  };

  const { yaml, tool } = buildWorkflowArtBundle({
    snapshot,
    workflowId: "color-compress",
    workflowName: "颜色迁移+压缩",
    params: [],
    exposed: new Set(),
    values: {},
  });
  assert.equal(tool.name, "颜色迁移+压缩");
  assert.equal(tool.description, "由 Hook 工作流创建的 Art。");
  const workflow = parseWorkflowYamlLite(yaml);
  const color = workflow.nodes.find((node) => node.id === "color");
  const compress = workflow.nodes.find((node) => node.id === "compress");

  assert.deepEqual(color?.needs, ["input-source", "reference-source"]);
  assert.deepEqual(color?.with, {
    input: "${{ nodes.input-source.outputs.output }}",
    reference: "${{ nodes.reference-source.outputs.output_image }}",
  });
  assert.deepEqual(compress?.needs, ["color"]);
  assert.deepEqual(compress?.with, { input: "${{ nodes.color.outputs.output }}" });

  const bindings = tool.execution?.workflowBindings as {
    inputs: Array<{ workflowParam: string; nodeId: string; target: string; kind: string }>;
  };
  assert.deepEqual(bindings.inputs.slice(0, 2), [
    { workflowParam: "input", nodeId: "input-source", target: "image", kind: "input_image" },
    { workflowParam: "input_2", nodeId: "reference-source", target: "image", kind: "input_image" },
  ]);
  assert.deepEqual(
    (tool.inputs as Array<{ name: string; label: string }>).map((input) => [input.name, input.label]),
    [["input", "输入图像"], ["input_2", "参考图像"]],
  );

  const fallbackYaml = buildSubWorkflowYaml(
    snapshot,
    new Set(snapshot.nodes.map((node) => node.id)),
    "color-compress",
  );
  assert.match(fallbackYaml, /reference: '\$\{\{ nodes\.reference-source\.outputs\.output_image \}\}'/);
  assert.match(fallbackYaml, /input: '\$\{\{ nodes\.color\.outputs\.output \}\}'/);
});

test("buildWorkflowArtBundle emits Hook screenshot sources as executable sticker nodes", () => {
  const workflowSnapshot: HookCanvasSnapshot = {
    ...pipeline,
    nodes: [
      { ...pipeline.nodes[0], workflowNodeId: "capture", upstreamWorkflowNodeIds: [] },
      { ...pipeline.nodes[1], workflowNodeId: "resize", upstreamWorkflowNodeIds: ["capture"] },
    ],
    edges: [pipeline.edges[0]],
  };
  const { yaml, tool } = buildWorkflowArtBundle({
    snapshot: workflowSnapshot,
    workflowId: "hook-image-flow",
    workflowName: "截图处理",
    params: [],
    exposed: new Set(),
    values: {},
  });

  assert.match(yaml, /- id: capture\s+uses: sticker/);
  assert.ok(!yaml.includes("uses: screenshot"));
  const bindings = tool.execution?.workflowBindings as {
    inputs: Array<{ nodeId: string; target: string; kind: string }>;
  };
  assert.deepEqual(bindings.inputs[0], {
    workflowParam: "input",
    nodeId: "capture",
    target: "image",
    kind: "input_image",
  });
});
