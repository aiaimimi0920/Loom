// Pure Art wizard draft conversion and workflow binding helpers.
import { LoomToolDefinition } from "../../services/loomApi";
import {
  type ParsedPort,
  type WorkflowBindingKind,
  type WorkflowExecutionBindings,
  type WorkflowInputBinding,
  type WorkflowOutputBinding,
} from "../../services/workflowStudio";
import { ArtWizardMode } from "../app/appShell";

export interface ArtWizardSubmitDraft {
  mode: ArtWizardMode;
  frameworkValues: Record<string, unknown>;
  repositoryName: string;
  name: string;
  description: string;
  command: string;
  argsText: string;
  endpoint: string;
  method: string;
  contentType: string;
  headersText: string;
  bodyText: string;
  mcpServerId: string;
  mcpToolName: string;
  workflowId: string;
  workflowPreviewOutput?: WorkflowOutputBinding;
  workflowPreviewRequiredNodes: string[];
  scriptEntryKind: "python" | "command";
  scriptSourcePath: string;
  scriptSourceCode: string;
  scriptSourceDirectory: string;
  inputPorts: ArtWizardPortDraft[];
  paramPorts: ArtWizardPortDraft[];
  outputPorts: ArtWizardPortDraft[];
  templateTool?: LoomToolDefinition;
}

export type ArtPortCaptureMode = "explicit_path" | "fixed_filename" | "derived_template" | "stdout";

export interface ArtWizardPortDraft {
  id: string;
  name: string;
  label: string;
  type: string;
  executionType: string;
  widget: string;
  dataType: string;
  defaultValue: string;
  min?: number;
  max?: number;
  step?: number;
  options?: unknown[];
  multiline: boolean;
  group: string;
  required: boolean;
  secret: boolean;
  disabled: boolean;
  jsonPath: string;
  captureMode: ArtPortCaptureMode;
  filename: string;
  originalValue: string;
  bindingNodeId: string;
  bindingTarget: string;
  bindingKind: WorkflowBindingKind | "";
}

export interface ArtCreationRequest {
  requestId: string;
  mode: ArtWizardMode;
  repositoryName: string;
  name: string;
  description: string;
  workflowId: string;
  templateTool?: LoomToolDefinition;
}

export const outputCaptureModes: ArtPortCaptureMode[] = [
  "explicit_path",
  "fixed_filename",
  "derived_template",
  "stdout",
];

export const defaultExecutionTypeForPort = (type: string, direction: "input" | "output") => {
  if (type === "image") return direction === "input" ? "image_path" : "image_buffer";
  if (type === "file") return "image_path";
  if (type === "boolean") return "bool";
  if (type === "int" || type === "float") return "number";
  return "string";
};

export const createPortDraft = (
  direction: "input" | "output",
  overrides: Partial<ArtWizardPortDraft> = {},
): ArtWizardPortDraft => {
  const type = overrides.type || (direction === "input" ? "image" : "image");
  return {
    id: overrides.id || "",
    name: overrides.name || (direction === "input" ? "input" : "result"),
    label: overrides.label || (direction === "input" ? "输入" : "结果"),
    type,
    executionType: overrides.executionType || defaultExecutionTypeForPort(type, direction),
    widget: overrides.widget || "",
    dataType: overrides.dataType || "",
    defaultValue: overrides.defaultValue || "",
    min: overrides.min,
    max: overrides.max,
    step: overrides.step,
    options: overrides.options,
    multiline: overrides.multiline ?? false,
    group: overrides.group || "",
    required: overrides.required ?? false,
    secret: overrides.secret ?? false,
    disabled: overrides.disabled ?? false,
    jsonPath: overrides.jsonPath || "",
    captureMode: overrides.captureMode || "explicit_path",
    filename: overrides.filename || "",
    originalValue: overrides.originalValue || "",
    bindingNodeId: overrides.bindingNodeId || "",
    bindingTarget: overrides.bindingTarget || "",
    bindingKind: overrides.bindingKind || "",
  };
};

export const portDraftFromParsedPort = (port: ParsedPort, direction: "input" | "output") =>
  createPortDraft(direction, {
    name: port.name,
    label: port.label || port.name,
    type: port.type,
    executionType: port.executionType || defaultExecutionTypeForPort(port.type, direction),
    defaultValue: port.default || "",
    jsonPath: port.jsonPath || "",
    originalValue: port.originalValue || "",
  });

export const recordValue = (value: unknown): Record<string, unknown> | null =>
  value && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;

export const stringValue = (value: unknown) => typeof value === "string" ? value : "";

export const numberValue = (value: unknown) => typeof value === "number" && Number.isFinite(value) ? value : undefined;

export const portDraftFromToolDefinition = (
  value: unknown,
  direction: "input" | "output",
): ArtWizardPortDraft | null => {
  const port = recordValue(value);
  if (!port) return null;
  const name = stringValue(port.name) || stringValue(port.id);
  if (!name) return null;
  const defaultValue = port.default === undefined || port.default === null
    ? ""
    : typeof port.default === "string"
      ? port.default
      : String(port.default);
  return createPortDraft(direction, {
    id: stringValue(port.id),
    name,
    label: stringValue(port.label) || name,
    type: stringValue(port.type) || "string",
    executionType: stringValue(port.executionType)
      || stringValue(port.execution_type)
      || defaultExecutionTypeForPort(stringValue(port.type), direction),
    widget: stringValue(port.widget),
    dataType: stringValue(port.data_type) || stringValue(port.dataType),
    defaultValue,
    min: numberValue(port.min) ?? numberValue(port.minimum),
    max: numberValue(port.max) ?? numberValue(port.maximum),
    step: numberValue(port.step),
    options: Array.isArray(port.options) ? port.options : undefined,
    multiline: port.multiline === true,
    group: stringValue(port.group),
    required: port.required === true,
    secret: port.secret === true,
    disabled: port.disabled === true,
    jsonPath: stringValue(port.jsonPath),
    captureMode: outputCaptureModes.includes(port.captureMode as ArtPortCaptureMode)
      ? port.captureMode as ArtPortCaptureMode
      : "explicit_path",
    filename: stringValue(port.filename),
    originalValue: stringValue(port.originalValue),
  });
};

export const toolPortDrafts = (
  values: unknown[] | undefined,
  direction: "input" | "output",
) => (values ?? [])
  .map((value) => portDraftFromToolDefinition(value, direction))
  .filter((port): port is ArtWizardPortDraft => port !== null);

export const workflowInputBindingKinds = new Set(["input_image", "input_value", "param"]);

export const normalizeWorkflowBindings = (value: unknown): WorkflowExecutionBindings => {
  const bindings = recordValue(value);
  const inputs: WorkflowInputBinding[] = Array.isArray(bindings?.inputs)
    ? bindings.inputs.flatMap((entry) => {
        const input = recordValue(entry);
        const workflowParam = stringValue(input?.workflowParam);
        const nodeId = stringValue(input?.nodeId);
        const target = stringValue(input?.target);
        const kind = stringValue(input?.kind);
        if (!workflowParam || !nodeId || !target || !workflowInputBindingKinds.has(kind)) return [];
        return [{
          workflowParam,
          nodeId,
          target,
          kind: kind as WorkflowInputBinding["kind"],
        }];
      })
    : [];
  const rawPrimaryOutput = recordValue(bindings?.primaryOutput);
  const primaryNodeId = stringValue(rawPrimaryOutput?.nodeId);
  const primaryTarget = stringValue(rawPrimaryOutput?.output);
  const rawPreviewOutput = recordValue(bindings?.previewOutput);
  const previewNodeId = stringValue(rawPreviewOutput?.nodeId);
  const previewTarget = stringValue(rawPreviewOutput?.output);
  const previewRequiredNodes = Array.isArray(bindings?.previewRequiredNodes)
    ? [...new Set(bindings.previewRequiredNodes.map(stringValue).filter(Boolean))]
    : [];
  return {
    inputs,
    ...(primaryNodeId && primaryTarget
      ? {
          primaryOutput: {
            nodeId: primaryNodeId,
            output: primaryTarget,
            kind: "node_result" as const,
          },
        }
      : {}),
    ...(previewNodeId && previewTarget
      ? {
          previewOutput: {
            nodeId: previewNodeId,
            output: previewTarget,
            kind: "node_result" as const,
          },
        }
      : {}),
    ...(previewRequiredNodes.length ? { previewRequiredNodes } : {}),
  };
};

export const workflowBindingsFromTool = (tool: LoomToolDefinition | undefined) =>
  normalizeWorkflowBindings(recordValue(tool?.execution)?.workflowBindings);

export const applyWorkflowInputBindingsToDrafts = (
  ports: ArtWizardPortDraft[],
  tool: LoomToolDefinition | undefined,
  kinds: ReadonlySet<string>,
  paramPorts = false,
) => {
  const bindings = workflowBindingsFromTool(tool).inputs;
  return ports.map((port) => {
    const workflowParam = paramPorts
      ? port.id.trim() || port.name.trim()
      : port.name.trim() || port.id.trim();
    const binding = bindings.find((candidate) => (
      candidate.workflowParam === workflowParam && kinds.has(candidate.kind)
    ));
    return binding
      ? {
          ...port,
          bindingNodeId: binding.nodeId,
          bindingTarget: binding.target,
          bindingKind: binding.kind,
        }
      : port;
  });
};

export const applyWorkflowOutputBindingToDrafts = (
  ports: ArtWizardPortDraft[],
  tool: LoomToolDefinition | undefined,
) => {
  const primaryOutput = workflowBindingsFromTool(tool).primaryOutput;
  if (!primaryOutput || !ports.length) return ports;
  return ports.map((port, index) => index === 0
    ? {
        ...port,
        bindingNodeId: primaryOutput.nodeId,
        bindingTarget: primaryOutput.output,
        bindingKind: "node_result" as const,
      }
    : port);
};

export const defaultWizardPorts = (mode: ArtWizardMode) => {
  switch (mode) {
    case "process":
      return {
        inputs: [createPortDraft("input", { name: "input", label: "输入", type: "file", executionType: "image_path" })],
        outputs: [createPortDraft("output", { name: "result", label: "结果", type: "file", executionType: "image_path" })],
      };
    case "cloud_api":
      return {
        inputs: [createPortDraft("input", { name: "image", label: "图像", type: "image", executionType: "image_path" })],
        outputs: [createPortDraft("output", { name: "result", label: "结果", type: "image", executionType: "image_path" })],
      };
    case "mcp":
      return {
        inputs: [createPortDraft("input", { name: "arguments", label: "参数", type: "string", executionType: "string" })],
        outputs: [createPortDraft("output", { name: "result", label: "结果", type: "string", executionType: "string" })],
      };
    case "workflow":
      return {
        inputs: [createPortDraft("input", { name: "input", label: "工作流输入", type: "image", executionType: "image_path" })],
        outputs: [createPortDraft("output", { name: "result", label: "工作流结果", type: "image", executionType: "image_path" })],
      };
    default:
      return {
        inputs: [createPortDraft("input")],
        outputs: [createPortDraft("output")],
      };
  }
};

export const toolPortFromDraft = (port: ArtWizardPortDraft, direction: "input" | "output") => {
  const next: Record<string, unknown> = {
    name: port.name.trim() || (direction === "input" ? "input" : "result"),
    label: port.label.trim() || port.name.trim() || (direction === "input" ? "输入" : "结果"),
    type: port.type.trim() || "string",
    executionType: port.executionType.trim() || defaultExecutionTypeForPort(port.type, direction),
  };
  if (port.id.trim()) next.id = port.id.trim();
  if (port.widget.trim()) next.widget = port.widget.trim();
  if (port.dataType.trim()) next.data_type = port.dataType.trim();
  if (direction === "input") {
    if (port.defaultValue.trim()) next.default = port.defaultValue;
    if (typeof port.min === "number") next.min = port.min;
    if (typeof port.max === "number") next.max = port.max;
    if (typeof port.step === "number") next.step = port.step;
    if (port.options?.length) next.options = port.options;
    if (port.multiline) next.multiline = true;
    if (port.group.trim()) next.group = port.group.trim();
    if (port.required) next.required = true;
    if (port.secret) next.secret = true;
    if (port.disabled) next.disabled = true;
  } else {
    next.captureMode = port.captureMode;
    if (port.jsonPath.trim()) next.jsonPath = port.jsonPath.trim();
    if (port.filename.trim()) next.filename = port.filename.trim();
    if (port.originalValue.trim()) next.originalValue = port.originalValue.trim();
  }
  return next;
};

export const defaultWidgetForParam = (type: string) => {
  if (type === "image") return "image_link";
  if (type === "int" || type === "float" || type === "number") return "number";
  if (type === "boolean") return "checkbox";
  return "text";
};

export const toolParamFromDraft = (port: ArtWizardPortDraft) => ({
  ...toolPortFromDraft(port, "input"),
  id: port.id.trim() || port.name.trim(),
  widget: port.widget.trim() || defaultWidgetForParam(port.type),
});

export const workflowBindingsFromDraft = (
  draft: ArtWizardSubmitDraft,
): WorkflowExecutionBindings | undefined => {
  if (draft.mode !== "workflow") return undefined;
  const existing = workflowBindingsFromTool(draft.templateTool);
  const managedWorkflowParams = new Set<string>();
  const additions: WorkflowInputBinding[] = [];

  for (const port of draft.inputPorts) {
    const workflowParam = port.name.trim() || port.id.trim();
    if (!workflowParam) continue;
    managedWorkflowParams.add(workflowParam);
    if (
      port.bindingNodeId
      && port.bindingTarget
      && (port.bindingKind === "input_image" || port.bindingKind === "input_value")
    ) {
      additions.push({
        workflowParam,
        nodeId: port.bindingNodeId,
        target: port.bindingTarget,
        kind: port.bindingKind,
      });
    }
  }
  for (const port of draft.paramPorts) {
    const workflowParam = port.id.trim() || port.name.trim();
    if (!workflowParam) continue;
    managedWorkflowParams.add(workflowParam);
    if (port.bindingNodeId && port.bindingTarget && port.bindingKind === "param") {
      additions.push({
        workflowParam,
        nodeId: port.bindingNodeId,
        target: port.bindingTarget,
        kind: "param",
      });
    }
  }

  const retained = existing.inputs.filter((binding) => (
    !managedWorkflowParams.has(binding.workflowParam)
    && !additions.some((addition) => (
      addition.nodeId === binding.nodeId
      && addition.target === binding.target
      && addition.kind === binding.kind
    ))
  ));
  const inputs = [...retained, ...additions];
  const outputPort = draft.outputPorts.find((port) => (
    port.bindingKind === "node_result" && port.bindingNodeId && port.bindingTarget
  ));
  const primaryOutput = outputPort
    ? {
        nodeId: outputPort.bindingNodeId,
        output: outputPort.bindingTarget,
        kind: "node_result" as const,
      }
    : existing.primaryOutput;
  const previewOutput = draft.workflowPreviewOutput ?? existing.previewOutput;
  const previewRequiredNodes = [...new Set([
    ...draft.workflowPreviewRequiredNodes,
    ...(previewOutput ? [previewOutput.nodeId] : []),
  ])];

  if (!inputs.length && !primaryOutput && !previewOutput && !previewRequiredNodes.length) return undefined;
  return {
    inputs,
    ...(primaryOutput ? { primaryOutput } : {}),
    ...(previewOutput ? { previewOutput } : {}),
    ...(previewRequiredNodes.length ? { previewRequiredNodes } : {}),
  };
};
