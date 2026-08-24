// Builds the YAML and tool contract for wrapping a frozen Hook pipeline as an Art.

import type { LoomToolDefinition } from "../loomApi.ts";
import { serializeWorkflowGraphLite, type WorkflowStudioNode } from "../workflowStudio.ts";
import { inferCanvasWorkflowInterface } from "./instantiation.ts";
import type { ExposableParam, HookCanvasSnapshot, WorkflowArtBundle } from "./types.ts";
import {
  requireWorkflowReferenceToken,
  workflowEdgeBindings,
  workflowNodeUses,
  workflowOutputReference,
} from "./workflowBindings.ts";

function safeParamName(raw: string): string {
  const cleaned = raw.replace(/[^a-zA-Z0-9_]+/g, "_").replace(/^_+|_+$/g, "");
  return cleaned || "input";
}

function widgetForUiType(uiType: string): string {
  if (uiType === "image") return "image_link";
  if (uiType === "int" || uiType === "float" || uiType === "number") return "number";
  if (uiType === "boolean") return "checkbox";
  return "text";
}

export function buildWorkflowArtBundle(options: {
  snapshot: HookCanvasSnapshot;
  workflowId: string;
  workflowName: string;
  params: ExposableParam[];
  exposed: Set<string>;
  values: Record<string, string>;
}): WorkflowArtBundle {
  const { snapshot, workflowId, workflowName, params, exposed, values } = options;

  const paramsByNode = new Map<string, ExposableParam[]>();
  for (const param of params) {
    const list = paramsByNode.get(param.workflowNodeId) ?? [];
    list.push(param);
    paramsByNode.set(param.workflowNodeId, list);
  }

  const rawToWorkflowId = new Map<string, string>();
  const workflowNodeIds = new Set<string>();
  for (const node of snapshot.nodes) {
    const workflowNodeId = requireWorkflowReferenceToken(
      node.workflowNodeId || node.id,
      "workflow node id",
    );
    if (workflowNodeIds.has(workflowNodeId)) {
      throw new Error(`Duplicate workflow node id: ${workflowNodeId}`);
    }
    workflowNodeIds.add(workflowNodeId);
    rawToWorkflowId.set(node.id, workflowNodeId);
  }
  const memberIds = new Set(rawToWorkflowId.values());
  const edgeBindingsByTarget = workflowEdgeBindings(snapshot, rawToWorkflowId, memberIds);

  const nodes: WorkflowStudioNode[] = snapshot.nodes.map((node) => {
    const wid = node.workflowNodeId || node.id;
    const withMap: Record<string, string> = {};
    for (const param of paramsByNode.get(wid) ?? []) {
      const key = `${wid}::${param.target}`;
      // Empty unexposed values are omitted so the wrapped tool can apply its own default.
      if (!exposed.has(key) && !param.secret) {
        const value = values[key] ?? "";
        if (value !== "") withMap[param.target] = value;
      }
    }
    const edgeBindings = edgeBindingsByTarget.get(wid) ?? [];
    for (const binding of edgeBindings) {
      // Connected data overrides any stale baked value for the same target port.
      withMap[binding.targetPortId] = workflowOutputReference(binding);
    }
    const needs = [...new Set([
      ...(node.upstreamWorkflowNodeIds ?? []).filter((id) => memberIds.has(id)),
      ...edgeBindings.map((binding) => binding.sourceNodeId),
    ])];
    return {
      id: wid,
      uses: workflowNodeUses(node),
      needs,
      with: withMap,
    };
  });
  const yaml = serializeWorkflowGraphLite({ name: workflowName, description: "", nodes });

  const iface = inferCanvasWorkflowInterface(snapshot);
  const usedNames = new Set<string>();
  const reserve = (preferred: string) => {
    let candidate = safeParamName(preferred);
    let suffix = 2;
    while (usedNames.has(candidate)) {
      candidate = `${safeParamName(preferred)}_${suffix}`;
      suffix += 1;
    }
    usedNames.add(candidate);
    return candidate;
  };

  const bindingInputs: Array<{
    workflowParam: string;
    nodeId: string;
    target: string;
    kind: "input_image" | "param";
  }> = [];
  // Hook maps tool params and image inputs through distinct runtime channels.
  const toolInputs: Array<{
    id: string;
    name: string;
    label: string;
    widget: string;
    type: string;
    executionType: string;
    default: string;
  }> = [];
  const toolParams: Array<Record<string, unknown>> = [];

  iface.inputs.forEach((port, index) => {
    const wid = rawToWorkflowId.get(port.nodeId) ?? port.nodeId;
    const workflowParam = reserve(index === 0 ? "input" : `input_${index + 1}`);
    bindingInputs.push({ workflowParam, nodeId: wid, target: port.portId, kind: "input_image" });
    toolInputs.push({
      id: workflowParam,
      name: workflowParam,
      label: port.semanticTarget?.toLowerCase().includes("reference")
        || port.semanticTarget?.toLowerCase() === "ref"
        ? "参考图像"
        : index === 0
          ? "输入图像"
          : port.label,
      widget: "image_link",
      type: "image",
      executionType: "image_buffer",
      default: "",
    });
  });

  for (const param of params) {
    const key = `${param.workflowNodeId}::${param.target}`;
    if (!exposed.has(key)) continue;
    const workflowParam = reserve(param.target);
    const workflowNodeId = requireWorkflowReferenceToken(param.workflowNodeId, "workflow node id");
    bindingInputs.push({
      workflowParam,
      nodeId: workflowNodeId,
      target: param.target,
      kind: "param",
    });
    const toolParam: Record<string, unknown> = {
      id: workflowParam,
      name: workflowParam,
      label: param.label,
      widget: param.widget || widgetForUiType(param.uiType),
      type: param.uiType,
      executionType: param.executionType,
    };
    // Secret values belong to credential bindings, never ordinary tool defaults or workflow YAML.
    if (!param.secret) toolParam.default = values[key] ?? param.defaultValue ?? "";
    if (param.dataType) toolParam.data_type = param.dataType;
    if (typeof param.min === "number") toolParam.min = param.min;
    if (typeof param.max === "number") toolParam.max = param.max;
    if (typeof param.step === "number") toolParam.step = param.step;
    if (param.options?.length) toolParam.options = param.options;
    if (param.multiline) toolParam.multiline = true;
    if (param.group) toolParam.group = param.group;
    if (param.required) toolParam.required = true;
    if (param.secret) toolParam.secret = true;
    toolParams.push(toolParam);
  }

  const outPort = iface.outputs[0];
  const primaryOutput = outPort
    ? {
        nodeId: rawToWorkflowId.get(outPort.nodeId) ?? outPort.nodeId,
        output: outPort.portId,
        kind: "node_result" as const,
      }
    : undefined;

  const tool: LoomToolDefinition = {
    id: `hook-wf-${workflowId}`,
    name: workflowName,
    description: "由 Hook 工作流创建的 Art。",
    enabled: true,
    execution: {
      type: "workflow",
      workflowId,
      workflowBindings: { inputs: bindingInputs, primaryOutput },
    },
    inputs: toolInputs,
    params: toolParams,
    outputs: [
      { name: "result", label: "输出图像", type: "image", executionType: "image_buffer" },
    ],
    metadata: {
      hookWorkflow: { managedBy: "hook-workflow" },
    },
  };
  return { yaml, tool };
}
