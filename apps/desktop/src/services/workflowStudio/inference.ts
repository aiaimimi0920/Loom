import type { LoomToolDefinition } from "../loomApi.ts";
import { isConfigured, safeName } from "./shared.ts";
import {
  mapParamExecutionType,
  mapParamUiType,
  normalizeInputExecutionType,
  normalizeOutputType,
  normalizeToolInputs,
  normalizeToolOutputs,
  normalizeToolParams,
  toolDefinitionsByIdentity,
  toolLabel,
} from "./tools.ts";
import type {
  ToolInputDefinition,
  ToolOutputDefinition,
  ToolParamDefinition,
  WorkflowExecutionBindings,
  WorkflowGraphLite,
  WorkflowInterfaceInference,
  WorkflowInterfacePort,
} from "./types.ts";
import { assertWorkflowGraphLimits } from "./validation.ts";

export function inferWorkflowArtInterface(
  workflow: WorkflowGraphLite,
  tools: LoomToolDefinition[],
): WorkflowInterfaceInference {
  assertWorkflowGraphLimits(workflow);
  const toolMap = toolDefinitionsByIdentity(tools);
  const inputCache = new Map<LoomToolDefinition, ToolInputDefinition[]>();
  const paramCache = new Map<LoomToolDefinition, ToolParamDefinition[]>();
  const outputCache = new Map<LoomToolDefinition, ToolOutputDefinition[]>();
  const inputsOf = (tool: LoomToolDefinition) => inputCache.get(tool) ?? (() => {
    const value = normalizeToolInputs(tool);
    inputCache.set(tool, value);
    return value;
  })();
  const paramsOf = (tool: LoomToolDefinition) => paramCache.get(tool) ?? (() => {
    const value = normalizeToolParams(tool);
    paramCache.set(tool, value);
    return value;
  })();
  const outputsOf = (tool: LoomToolDefinition) => outputCache.get(tool) ?? (() => {
    const value = normalizeToolOutputs(tool);
    outputCache.set(tool, value);
    return value;
  })();

  const incomingNodeIds = new Set<string>();
  for (const node of workflow.nodes) {
    for (const neededId of node.needs) incomingNodeIds.add(neededId);
  }
  const usedNames = new Set<string>();
  const inputs: WorkflowInterfacePort[] = [];
  const warnings: string[] = [];
  const bindings: WorkflowExecutionBindings = { inputs: [] };
  let imageInputBound = false;

  const reserveName = (preferred: string) => {
    const base = safeName(preferred);
    let candidate = base;
    let suffix = 2;
    while (usedNames.has(candidate)) {
      candidate = `${base}_${suffix}`;
      suffix += 1;
    }
    usedNames.add(candidate);
    return candidate;
  };

  for (const node of workflow.nodes) {
    const tool = toolMap.get(node.uses);
    if (!tool) {
      warnings.push(`未找到 ${node.uses} 的注册表定义，已使用结构化回退推断接口。`);
      continue;
    }
    const nodeName = toolLabel(tool, node.id);

    for (const input of inputsOf(tool)) {
      const target = input.name || "input";
      if (isConfigured(node.with[target])) continue;
      const type = input.type === "text" ? "string" : input.type || "string";
      const imageLike = type === "image" || normalizeInputExecutionType(input, type).startsWith("image_");
      if (imageLike && imageInputBound) {
        warnings.push(`发现多个未绑定图像输入，已跳过 ${nodeName} / ${input.label || target}。`);
        continue;
      }
      const workflowParam = reserveName(imageLike ? "input" : `${node.id}_${target}`);
      inputs.push({
        name: workflowParam,
        label: imageLike ? "输入图像" : `${nodeName} / ${input.label || target}`,
        type,
        executionType: normalizeInputExecutionType(input, type),
        default: input.default === undefined ? "" : String(input.default),
        bindingNodeId: node.id,
        bindingTarget: target,
        bindingKind: imageLike ? "input_image" : "input_value",
      });
      bindings.inputs.push({
        workflowParam,
        nodeId: node.id,
        target,
        kind: imageLike ? "input_image" : "input_value",
      });
      if (imageLike) imageInputBound = true;
    }

    for (const param of paramsOf(tool)) {
      const target = param.id || param.name;
      if (!target || param.disabled || isConfigured(node.with[target])) continue;
      const type = mapParamUiType(param);
      const workflowParam = reserveName(`${node.id}_${target}`);
      inputs.push({
        name: workflowParam,
        label: `${nodeName} / ${param.label || target}`,
        type,
        executionType: mapParamExecutionType(param, type),
        default: param.default === undefined ? "" : String(param.default),
        bindingNodeId: node.id,
        bindingTarget: target,
        bindingKind: "param",
        widget: param.widget,
        dataType: param.dataType || param.data_type,
        min: param.min,
        max: param.max,
        step: param.step,
        options: param.options,
        multiline: param.multiline,
        group: param.group,
        required: param.required,
        secret: param.secret,
      });
      bindings.inputs.push({ workflowParam, nodeId: node.id, target, kind: "param" });
    }
  }

  const terminalNode = [...workflow.nodes].reverse().find((node) => !incomingNodeIds.has(node.id));
  const terminalTool = terminalNode ? toolMap.get(terminalNode.uses) : undefined;
  const terminalOutputs = terminalTool ? outputsOf(terminalTool) : [];
  const terminalOutput = terminalOutputs.find((output) => normalizeOutputType(output).type === "image") || terminalOutputs[0];
  const normalizedOutput = normalizeOutputType(terminalOutput);
  const terminalNodeId = terminalNode?.id || workflow.nodes[workflow.nodes.length - 1]?.id || "result";
  const outputs: WorkflowInterfacePort[] = [{
    name: "result",
    label: terminalOutput?.label || "Result",
    type: normalizedOutput.type,
    executionType: normalizedOutput.executionType,
    bindingNodeId: terminalNodeId,
    bindingTarget: terminalOutput?.name || "result",
    bindingKind: "node_result",
  }];
  bindings.primaryOutput = {
    nodeId: terminalNodeId,
    output: terminalOutput?.name || "result",
    kind: "node_result",
  };

  if (!workflow.nodes.length) warnings.push("工作流 YAML 没有节点。");
  if (!imageInputBound) warnings.push("没有检测到未绑定图像输入；该工作流可能依赖常量或上游文件。");
  return { inputs, outputs, bindings, warnings };
}
