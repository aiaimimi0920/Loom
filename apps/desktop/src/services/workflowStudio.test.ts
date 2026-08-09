import assert from "node:assert/strict";
import test from "node:test";

import {
  collectWorkflowParamBindingCandidates,
  collectWorkflowPreviewNodeOptions,
  mapParamUiType,
  parseWorkflowYamlLite,
  toolDefinitionsByIdentity,
} from "./workflowStudio.ts";
import type { LoomToolDefinition } from "./loomApi.ts";

test("keeps decimal sliders as floats even when their defaults are integers", () => {
  assert.equal(mapParamUiType({
    id: "gamma",
    widget: "slider",
    data_type: "number",
    default: 1,
    min: 0.1,
    max: 3,
    step: 0.1,
  }), "float");
  assert.equal(mapParamUiType({
    id: "exposure",
    widget: "slider",
    data_type: "number",
    default: 0,
    min: -4,
    max: 4,
    step: 0.1,
  }), "float");
});

test("lists image-capable outputs for every workflow preview node", () => {
  const transfer = {
    id: "color-transfer",
    name: "颜色迁移",
    description: "",
    enabled: true,
    outputs: [
      { name: "output", label: "迁移结果", type: "image", executionType: "image_buffer" },
      { name: "stats", label: "统计", type: "string", executionType: "string" },
    ],
  } as LoomToolDefinition;
  const compress = {
    id: "image-compress",
    name: "图片压缩",
    description: "",
    enabled: true,
    outputs: [
      { name: "output_image", label: "压缩结果", type: "image", executionType: "image_buffer" },
    ],
  } as LoomToolDefinition;
  const workflow = parseWorkflowYamlLite(`name: 迁移压缩
nodes:
  - id: transfer
    uses: color-transfer
  - id: compress
    uses: image-compress
    needs: [transfer]
  - id: sticker
    uses: sticker
`);

  assert.deepEqual(collectWorkflowPreviewNodeOptions(workflow, [transfer, compress]), [
    {
      nodeId: "transfer",
      label: "颜色迁移",
      outputs: [{ name: "output", label: "迁移结果" }],
    },
    {
      nodeId: "compress",
      label: "图片压缩",
      outputs: [{ name: "output_image", label: "压缩结果" }],
    },
    {
      nodeId: "sticker",
      label: "sticker",
      outputs: [{ name: "output_image", label: "图像" }],
    },
  ]);
});

test("keeps whole-step sliders and explicit integers as integer controls", () => {
  assert.equal(mapParamUiType({
    id: "strength",
    widget: "slider",
    data_type: "number",
    default: 100,
    min: 0,
    max: 100,
    step: 1,
  }), "int");
  assert.equal(mapParamUiType({
    id: "iterations",
    widget: "number",
    data_type: "integer",
    default: 1,
    step: 1,
  }), "int");
});

test("indexes installed tools by local and publisher-qualified Art identity", () => {
  const tool = {
    id: "custom-color-transfer",
    name: "颜色迁移",
    description: "",
    enabled: true,
    metadata: {
      artPackage: {
        qualifiedId: "neuro.official/custom-color-transfer",
      },
    },
  } as LoomToolDefinition;

  const tools = toolDefinitionsByIdentity([tool]);
  assert.equal(tools.get("custom-color-transfer"), tool);
  assert.equal(tools.get("neuro.official/custom-color-transfer"), tool);
});

test("lists baked workflow node params as public binding candidates", () => {
  const colorTransfer = {
    id: "custom-color-transfer",
    name: "颜色迁移",
    description: "",
    enabled: true,
    params: [
      {
        id: "strength",
        label: "迁移强度",
        widget: "slider",
        data_type: "number",
        default: 100,
        min: 0,
        max: 100,
        step: 1,
      },
      {
        id: "gamma",
        label: "Gamma",
        widget: "slider",
        data_type: "number",
        default: 1,
        step: 0.1,
      },
      {
        id: "internal_debug",
        label: "调试",
        data_type: "boolean",
        disabled: true,
      },
    ],
    metadata: {
      artPackage: {
        qualifiedId: "neuro.official/custom-color-transfer",
      },
    },
  } as LoomToolDefinition;
  const workflow = parseWorkflowYamlLite(`name: 迁移压缩
nodes:
  - id: transfer
    uses: neuro.official/custom-color-transfer
    with:
      strength: 87
`);

  const candidates = collectWorkflowParamBindingCandidates(workflow, [colorTransfer]);

  assert.deepEqual(candidates, [
    {
      key: "transfer::strength",
      nodeId: "transfer",
      nodeLabel: "颜色迁移",
      target: "strength",
      paramLabel: "迁移强度",
      type: "int",
      executionType: "number",
      defaultValue: "87",
      widget: "slider",
      dataType: "number",
      min: 0,
      max: 100,
      step: 1,
      options: undefined,
      multiline: undefined,
      group: undefined,
      required: undefined,
      secret: undefined,
    },
    {
      key: "transfer::gamma",
      nodeId: "transfer",
      nodeLabel: "颜色迁移",
      target: "gamma",
      paramLabel: "Gamma",
      type: "float",
      executionType: "number",
      defaultValue: "1",
      widget: "slider",
      dataType: "number",
      min: undefined,
      max: undefined,
      step: 0.1,
      options: undefined,
      multiline: undefined,
      group: undefined,
      required: undefined,
      secret: undefined,
    },
  ]);
});
