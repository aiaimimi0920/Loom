import { artPackageIdentity } from "../artHubUi.ts";
import type { LoomToolDefinition } from "../loomApi.ts";
import { isConfigured } from "./shared.ts";
import type {
  ToolInputDefinition,
  ToolOutputDefinition,
  ToolParamDefinition,
  WorkflowGraphLite,
  WorkflowParamBindingCandidate,
  WorkflowPreviewNodeOption,
} from "./types.ts";
import { assertWorkflowGraphLimits } from "./validation.ts";

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);

const optionalString = (value: unknown) => (typeof value === "string" ? value : undefined);
const optionalNumber = (value: unknown) => (typeof value === "number" ? value : undefined);
const optionalBoolean = (value: unknown) => (typeof value === "boolean" ? value : undefined);
const optionalArray = (value: unknown) => (Array.isArray(value) ? value : undefined);

export const normalizeToolInputs = (tool: LoomToolDefinition): ToolInputDefinition[] => {
  if (!Array.isArray(tool.inputs)) return [];
  return tool.inputs.filter(isRecord).map((input) => ({
    name: optionalString(input.name),
    label: optionalString(input.label),
    type: optionalString(input.type),
    execution_type: optionalString(input.execution_type),
    executionType: optionalString(input.executionType),
    default: input.default,
    disabled: optionalBoolean(input.disabled),
  }));
};

export const normalizeToolParams = (tool: LoomToolDefinition): ToolParamDefinition[] => {
  if (!Array.isArray(tool.params)) return [];
  return tool.params.filter(isRecord).map((param) => ({
    id: optionalString(param.id),
    name: optionalString(param.name),
    label: optionalString(param.label),
    widget: optionalString(param.widget),
    data_type: optionalString(param.data_type),
    dataType: optionalString(param.dataType),
    default: param.default,
    min: optionalNumber(param.min),
    max: optionalNumber(param.max),
    step: optionalNumber(param.step),
    options: optionalArray(param.options),
    multiline: optionalBoolean(param.multiline),
    group: optionalString(param.group),
    required: optionalBoolean(param.required),
    secret: optionalBoolean(param.secret),
    disabled: optionalBoolean(param.disabled),
  }));
};

export const normalizeToolOutputs = (tool: LoomToolDefinition): ToolOutputDefinition[] => {
  if (!Array.isArray(tool.outputs)) return [];
  return tool.outputs.filter(isRecord).map((output) => ({
    name: optionalString(output.name),
    label: optionalString(output.label),
    type: optionalString(output.type),
    execution_type: optionalString(output.execution_type),
    executionType: optionalString(output.executionType),
  }));
};

export const toolLabel = (tool: LoomToolDefinition | undefined, fallback: string) => {
  if (!tool) return fallback;
  const label = isRecord(tool) ? optionalString(tool.label) : undefined;
  return label || tool.name || fallback;
};

export const mapParamUiType = (param: ToolParamDefinition) => {
  const dataType = param.dataType || param.data_type;
  if (param.widget === "slider" || param.widget === "number" || ["number", "integer", "int", "float"].includes(dataType || "")) {
    if (dataType === "integer" || dataType === "int") return "int";
    if (dataType === "float" || (typeof param.step === "number" && !Number.isInteger(param.step))) return "float";
    return typeof param.default === "number" && Number.isInteger(param.default) ? "int" : "float";
  }
  if (param.widget === "checkbox" || dataType === "bool" || dataType === "boolean") return "boolean";
  if (param.widget === "image_link" || dataType?.startsWith("image_")) return "image";
  return "string";
};

export const mapParamExecutionType = (param: ToolParamDefinition, uiType: string) =>
  param.dataType || param.data_type || (uiType === "image" ? "image_path" : uiType === "boolean" ? "bool" : uiType);

export const normalizeInputExecutionType = (input: ToolInputDefinition, uiType: string) =>
  input.executionType || input.execution_type || (uiType === "image" ? "image_path" : "string");

export const normalizeOutputType = (output?: ToolOutputDefinition) => {
  const type = output?.type === "text" ? "string" : output?.type || "string";
  const executionType = output?.executionType || output?.execution_type ||
    (type === "image" ? "image_buffer" : type === "file" ? "image_path" : type);
  return { type, executionType };
};

export function toolDefinitionsByIdentity(tools: LoomToolDefinition[]): Map<string, LoomToolDefinition> {
  const toolMap = new Map<string, LoomToolDefinition>();
  for (const tool of tools) {
    toolMap.set(tool.id, tool);
    const qualifiedId = artPackageIdentity(tool);
    if (qualifiedId) toolMap.set(qualifiedId, tool);
  }
  return toolMap;
}

export function collectWorkflowPreviewNodeOptions(
  workflow: WorkflowGraphLite,
  tools: LoomToolDefinition[],
): WorkflowPreviewNodeOption[] {
  assertWorkflowGraphLimits(workflow);
  const toolMap = toolDefinitionsByIdentity(tools);
  const outputCache = new Map<LoomToolDefinition, ToolOutputDefinition[]>();
  const outputsOf = (tool: LoomToolDefinition) => {
    let outputs = outputCache.get(tool);
    if (!outputs) {
      outputs = normalizeToolOutputs(tool);
      outputCache.set(tool, outputs);
    }
    return outputs;
  };

  return workflow.nodes.map((node) => {
    const tool = toolMap.get(node.uses);
    const outputs = tool
      ? outputsOf(tool).flatMap((output) => {
          const normalized = normalizeOutputType(output);
          const name = output.name?.trim();
          const imageLike = normalized.type === "image" || normalized.executionType.startsWith("image_");
          return name && imageLike ? [{ name, label: output.label?.trim() || name }] : [];
        })
      : node.uses === "sticker"
        ? [{ name: "output_image", label: "图像" }]
        : [];
    return { nodeId: node.id, label: toolLabel(tool, node.id), outputs };
  });
}

export function collectWorkflowParamBindingCandidates(
  workflow: WorkflowGraphLite,
  tools: LoomToolDefinition[],
): WorkflowParamBindingCandidate[] {
  assertWorkflowGraphLimits(workflow);
  const toolMap = toolDefinitionsByIdentity(tools);
  const paramCache = new Map<LoomToolDefinition, ToolParamDefinition[]>();
  const paramsOf = (tool: LoomToolDefinition) => {
    let params = paramCache.get(tool);
    if (!params) {
      params = normalizeToolParams(tool);
      paramCache.set(tool, params);
    }
    return params;
  };

  return workflow.nodes.flatMap((node) => {
    const tool = toolMap.get(node.uses);
    if (!tool) return [];
    const nodeLabel = toolLabel(tool, node.id);
    return paramsOf(tool).flatMap((param) => {
      const target = param.id || param.name;
      if (!target || param.disabled) return [];
      const type = mapParamUiType(param);
      const configuredDefault = node.with[target];
      return [{
        key: `${encodeURIComponent(node.id)}::${encodeURIComponent(target)}`,
        nodeId: node.id,
        nodeLabel,
        target,
        paramLabel: param.label || target,
        type,
        executionType: mapParamExecutionType(param, type),
        defaultValue: isConfigured(configuredDefault)
          ? String(configuredDefault)
          : param.default === undefined ? "" : String(param.default),
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
      }];
    });
  });
}
