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
    name: "Color transfer",
    description: "",
    enabled: true,
    outputs: [
      { name: "output", label: "Transferred image", type: "image", executionType: "image_buffer" },
      { name: "stats", label: "Stats", type: "string", executionType: "string" },
    ],
  } as LoomToolDefinition;
  const compress = {
    id: "image-compress",
    name: "Image compress",
    description: "",
    enabled: true,
    outputs: [
      { name: "output_image", label: "Compressed image", type: "image", executionType: "image_buffer" },
    ],
  } as LoomToolDefinition;
  const workflow = parseWorkflowYamlLite(`name: Transfer and compress
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
      label: "Color transfer",
      outputs: [{ name: "output", label: "Transferred image" }],
    },
    {
      nodeId: "compress",
      label: "Image compress",
      outputs: [{ name: "output_image", label: "Compressed image" }],
    },
    {
      nodeId: "sticker",
      label: "sticker",
      outputs: [{ name: "output_image", label: "Image" }],
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
    name: "Color transfer",
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
    name: "Color transfer",
    description: "",
    enabled: true,
    params: [
      {
        id: "strength",
        label: "Transfer strength",
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
        label: "Debug",
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
  const workflow = parseWorkflowYamlLite(`name: Transfer and compress
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
      nodeLabel: "Color transfer",
      target: "strength",
      paramLabel: "Transfer strength",
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
      nodeLabel: "Color transfer",
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
