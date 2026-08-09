import type { LoomToolDefinition } from "./loomApi";
import { artPackageIdentity } from "./artHubUi.ts";

export type WorkflowPortType = "image" | "file" | "int" | "float" | "string" | "boolean";

export interface ParsedPort {
  name: string;
  type: WorkflowPortType;
  originalValue: string;
  isInput: boolean;
  label?: string;
  default?: string;
  jsonPath?: string;
  executionType?: string;
}

export interface CurlImportResult {
  url: string;
  method: string;
  headers: Record<string, string>;
  body: string;
  suggestedInputs: ParsedPort[];
}

export interface RawCommandImportResult {
  command: string;
  args: string[];
  argsText: string;
  ports: ParsedPort[];
}

export interface McpToolSchemaImportResult {
  toolName: string;
  suggestedInputs: ParsedPort[];
  suggestedOutputs: ParsedPort[];
}

export interface WorkflowStudioNode {
  id: string;
  uses: string;
  needs: string[];
  with: Record<string, string>;
}

export interface WorkflowGraphLite {
  name: string;
  description: string;
  nodes: WorkflowStudioNode[];
}

export interface WorkflowGraphNodePatch {
  id?: string;
  uses?: string;
  needs?: string[];
  with?: Record<string, string>;
}

export type WorkflowBindingKind = "input_image" | "input_value" | "param" | "node_result";

export interface WorkflowInputBinding {
  workflowParam: string;
  nodeId: string;
  target: string;
  kind: Exclude<WorkflowBindingKind, "node_result">;
}

export interface WorkflowOutputBinding {
  nodeId: string;
  output: string;
  kind: "node_result";
}

export interface WorkflowExecutionBindings {
  inputs: WorkflowInputBinding[];
  primaryOutput?: WorkflowOutputBinding;
  previewOutput?: WorkflowOutputBinding;
  previewRequiredNodes?: string[];
}

export interface WorkflowPreviewNodeOption {
  nodeId: string;
  label: string;
  outputs: Array<{
    name: string;
    label: string;
  }>;
}

export interface WorkflowInterfacePort {
  name: string;
  label: string;
  type: string;
  executionType: string;
  default?: string;
  bindingNodeId?: string;
  bindingTarget?: string;
  bindingKind?: WorkflowBindingKind;
  widget?: string;
  dataType?: string;
  min?: number;
  max?: number;
  step?: number;
  options?: unknown[];
  multiline?: boolean;
  group?: string;
  required?: boolean;
  secret?: boolean;
}

export interface WorkflowInterfaceInference {
  inputs: WorkflowInterfacePort[];
  outputs: WorkflowInterfacePort[];
  bindings: WorkflowExecutionBindings;
  warnings: string[];
}

export interface WorkflowParamBindingCandidate {
  key: string;
  nodeId: string;
  nodeLabel: string;
  target: string;
  paramLabel: string;
  type: string;
  executionType: string;
  defaultValue: string;
  widget?: string;
  dataType?: string;
  min?: number;
  max?: number;
  step?: number;
  options?: unknown[];
  multiline?: boolean;
  group?: string;
  required?: boolean;
  secret?: boolean;
}

export interface ToolInputDefinition {
  name?: string;
  label?: string;
  type?: string;
  execution_type?: string;
  executionType?: string;
  default?: unknown;
  disabled?: boolean;
}

export interface ToolParamDefinition {
  id?: string;
  name?: string;
  label?: string;
  widget?: string;
  data_type?: string;
  dataType?: string;
  default?: unknown;
  min?: number;
  max?: number;
  step?: number;
  options?: unknown[];
  multiline?: boolean;
  group?: string;
  required?: boolean;
  secret?: boolean;
  disabled?: boolean;
}

interface ToolOutputDefinition {
  name?: string;
  label?: string;
  type?: string;
  execution_type?: string;
  executionType?: string;
}

const fileExtensions = new Set([
  "png",
  "jpg",
  "jpeg",
  "webp",
  "gif",
  "bmp",
  "tiff",
  "svg",
  "mp3",
  "wav",
  "aac",
  "flac",
  "m4a",
  "ogg",
  "mp4",
  "mkv",
  "avi",
  "mov",
  "webm",
  "txt",
  "json",
  "pdf",
  "zip",
]);

const isFilePath = (value: string) => {
  const extension = value.split(".").pop()?.toLowerCase();
  return extension ? fileExtensions.has(extension) : false;
};

const isNumeric = (value: string) => value.trim() !== "" && Number.isFinite(Number(value));

const stripQuotes = (value: string) => value.replace(/^['"]|['"]$/g, "");

const tokenizeCommand = (command: string): string[] => {
  const tokens: string[] = [];
  let current = "";
  let quote: string | null = null;

  for (let index = 0; index < command.length; index += 1) {
    const char = command[index];
    if ((char === '"' || char === "'") && command[index - 1] !== "\\") {
      if (quote === char) {
        quote = null;
      } else if (!quote) {
        quote = char;
      } else {
        current += char;
      }
      continue;
    }

    if (/\s/.test(char) && !quote) {
      if (current) {
        tokens.push(current);
        current = "";
      }
      continue;
    }

    current += char;
  }

  if (current) tokens.push(current);
  return tokens;
};

const safeName = (value: string, fallback = "value") =>
  value.replace(/[^a-zA-Z0-9_-]/g, "_").replace(/^_+|_+$/g, "") || fallback;

const inferPrimitiveType = (value: unknown): WorkflowPortType => {
  if (typeof value === "number") return Number.isInteger(value) ? "int" : "float";
  if (typeof value === "boolean") return "boolean";
  if (typeof value === "string" && isFilePath(value)) return "file";
  return "string";
};

export function parseCurlCommand(curlCommand: string): CurlImportResult | null {
  if (!curlCommand.trim().toLowerCase().startsWith("curl")) return null;

  const tokens = tokenizeCommand(curlCommand);
  let url = "";
  let method = "GET";
  const headers: Record<string, string> = {};
  let body = "";

  for (let index = 1; index < tokens.length; index += 1) {
    const token = tokens[index];
    const next = tokens[index + 1] ?? "";
    if (token === "-X" || token === "--request") {
      method = stripQuotes(next).toUpperCase();
      index += 1;
    } else if (token === "-H" || token === "--header") {
      const [key, ...rest] = stripQuotes(next).split(":");
      if (key) headers[key.trim()] = rest.join(":").trim();
      index += 1;
    } else if (["--data", "-d", "--data-raw", "--data-binary"].includes(token)) {
      body = stripQuotes(next);
      if (method === "GET") method = "POST";
      index += 1;
    } else if (/^https?:\/\//i.test(token)) {
      url = stripQuotes(token);
    }
  }

  const suggestedInputs: ParsedPort[] = [];
  if (body) {
    try {
      const parsed = JSON.parse(body) as Record<string, unknown>;
      const templated = templateObjectInputs(parsed, suggestedInputs, []);
      body = JSON.stringify(templated, null, 2);
    } catch {
      suggestedInputs.push(...parseTemplate(body).filter((port) => port.isInput));
    }
  }

  return { url, method, headers, body, suggestedInputs };
}

export function parseRawCommand(rawCommand: string): RawCommandImportResult | null {
  const tokens = tokenizeCommand(rawCommand.trim());
  if (!tokens.length) return null;

  const [command, ...args] = tokens;
  const argsText = args.join("\n");
  const template = args.join(" ");
  const ports = parseTemplate(template);

  return {
    command,
    args,
    argsText,
    ports,
  };
}

const templateObjectInputs = (
  value: unknown,
  ports: ParsedPort[],
  path: Array<string | number>,
): unknown => {
  if (Array.isArray(value)) {
    return value.map((item, index) => templateObjectInputs(item, ports, [...path, index]));
  }
  if (value && typeof value === "object") {
    const output: Record<string, unknown> = {};
    for (const [key, nested] of Object.entries(value)) {
      output[key] = templateObjectInputs(nested, ports, [...path, key]);
    }
    return output;
  }
  if (value === undefined || value === null) return value;

  const name = safeName(path.join("_"), "param");
  ports.push({
    name,
    type: inferPrimitiveType(value),
    originalValue: String(value),
    isInput: true,
    label: name,
    default: String(value),
  });
  return `{{inputs.${name}.value}}`;
};

export function parseTemplate(template: string): ParsedPort[] {
  const ports: ParsedPort[] = [];
  const seen = new Set<string>();
  const regex =
    /\{\{(?:(inputs|outputs)\.)?([a-zA-Z0-9_-]+)(?:\.(path|value))?\}\}|\{\{(-{1,2}[a-zA-Z0-9_-]+)\}\}/g;
  let match: RegExpExecArray | null;

  while ((match = regex.exec(template)) !== null) {
    const flagName = match[4]?.replace(/^-+/, "");
    const category = match[1] || "inputs";
    const name = flagName || match[2];
    if (!name) continue;
    const key = `${category}.${name}`;
    if (seen.has(key)) continue;
    seen.add(key);

    const isInput = category !== "outputs";
    const property = match[3];
    const lower = name.toLowerCase();
    const type: WorkflowPortType = flagName
      ? "boolean"
      : property === "path" || lower.includes("file") || lower.includes("path")
        ? "file"
        : lower.includes("image") || lower.includes("img") || lower.includes("photo")
          ? "image"
          : lower.includes("width") || lower.includes("height") || lower.includes("seed")
            ? "int"
            : lower.includes("scale") || lower.includes("strength") || lower.includes("ratio")
              ? "float"
              : "string";

    ports.push({
      name,
      type,
      originalValue: "",
      isInput,
      label: flagName ? match[4] : name,
      default: flagName ? "false" : "",
    });
  }

  return ports;
}

export function autoTemplateResponse(jsonString: string): { templatedJson: string; ports: ParsedPort[] } {
  const ports: ParsedPort[] = [];
  try {
    const parsed = JSON.parse(jsonString) as unknown;
    const templated = templateObjectOutputs(parsed, ports, []);
    return { templatedJson: JSON.stringify(templated, null, 2), ports };
  } catch {
    return { templatedJson: jsonString, ports };
  }
}

const templateObjectOutputs = (
  value: unknown,
  ports: ParsedPort[],
  path: Array<string | number>,
): unknown => {
  if (Array.isArray(value)) {
    return value.map((item, index) => templateObjectOutputs(item, ports, [...path, index]));
  }
  if (value && typeof value === "object") {
    const output: Record<string, unknown> = {};
    for (const [key, nested] of Object.entries(value)) {
      output[key] = templateObjectOutputs(nested, ports, [...path, key]);
    }
    return output;
  }
  if (value === undefined || value === null) return value;

  const name = safeName(path.join("_"), "result");
  const jsonPath = path
    .map((segment, index) => (typeof segment === "number" ? `[${segment}]` : index === 0 ? segment : `.${segment}`))
    .join("")
    .replace(/\.\[/g, "[");
  ports.push({
    name,
    type: inferPrimitiveType(value),
    originalValue: String(value),
    isInput: false,
    label: name,
    jsonPath,
  });
  return `{{outputs.${name}.value}}`;
};

const schemaTypeToPortType = (name: string, schema: Record<string, unknown>): WorkflowPortType => {
  const lowerName = name.toLowerCase();
  const schemaType = schema.type;
  const format = typeof schema.format === "string" ? schema.format.toLowerCase() : "";
  const description = typeof schema.description === "string" ? schema.description.toLowerCase() : "";

  if (schemaType === "integer") return "int";
  if (schemaType === "number") return "float";
  if (schemaType === "boolean") return "boolean";
  if (
    lowerName.includes("image") ||
    lowerName.includes("screenshot") ||
    description.includes("image") ||
    format.includes("binary")
  ) {
    return "image";
  }
  if (lowerName.includes("path") || lowerName.includes("file") || format === "uri") return "file";
  return "string";
};

const portExecutionType = (type: WorkflowPortType, direction: "input" | "output") => {
  if (type === "image") return direction === "input" ? "image_path" : "image_buffer";
  if (type === "file") return "image_path";
  if (type === "boolean") return "bool";
  if (type === "int" || type === "float") return "number";
  return "string";
};

const readSchema = (tool: Record<string, unknown>) => {
  const inputSchema = tool.input_schema ?? tool.inputSchema;
  if (inputSchema && typeof inputSchema === "object" && !Array.isArray(inputSchema)) {
    return inputSchema as Record<string, unknown>;
  }
  return null;
};

export function portsFromMcpToolSchema(tool: unknown): McpToolSchemaImportResult | null {
  if (!tool || typeof tool !== "object" || Array.isArray(tool)) return null;
  const record = tool as Record<string, unknown>;
  const toolName = typeof record.name === "string" && record.name.trim() ? record.name.trim() : "mcp_tool";
  const schema = readSchema(record);
  const properties =
    schema?.properties && typeof schema.properties === "object" && !Array.isArray(schema.properties)
      ? schema.properties as Record<string, unknown>
      : {};

  const suggestedInputs = Object.entries(properties).flatMap(([name, property]) => {
    if (!property || typeof property !== "object" || Array.isArray(property)) return [];
    const propertyRecord = property as Record<string, unknown>;
    const type = schemaTypeToPortType(name, propertyRecord);
    const label =
      typeof propertyRecord.title === "string" && propertyRecord.title.trim()
        ? propertyRecord.title.trim()
        : name;
    return [{
      name: safeName(name),
      label,
      type,
      originalValue: "",
      isInput: true,
      default: propertyRecord.default === undefined ? "" : String(propertyRecord.default),
      executionType: portExecutionType(type, "input"),
    }];
  });

  const outputType =
    /screenshot|image|ocr|vision/i.test(toolName) ? "image" : "string";
  const suggestedOutputs: ParsedPort[] = [{
    name: outputType === "image" ? "image" : "result",
    label: outputType === "image" ? "Image" : "Result",
    type: outputType,
    originalValue: "",
    isInput: false,
    default: "",
    executionType: portExecutionType(outputType, "output"),
  }];

  return {
    toolName,
    suggestedInputs,
    suggestedOutputs,
  };
}

export function parseWorkflowYamlLite(yaml: string): WorkflowGraphLite {
  const graph: WorkflowGraphLite = { name: "未命名工作流", description: "", nodes: [] };
  let current: WorkflowStudioNode | null = null;
  let inWith = false;

  for (const rawLine of yaml.split(/\r?\n/)) {
    const line = rawLine.replace(/\t/g, "  ");
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith("#")) continue;

    const topLevel = /^([a-zA-Z0-9_-]+):\s*(.*)$/.exec(trimmed);
    if (topLevel && !rawLine.startsWith(" ")) {
      if (topLevel[1] === "name") graph.name = stripQuotes(topLevel[2]) || graph.name;
      if (topLevel[1] === "description") graph.description = stripQuotes(topLevel[2]);
      inWith = false;
      continue;
    }

    const nodeStart = /^-\s*id:\s*(.+)$/.exec(trimmed);
    if (nodeStart) {
      current = { id: stripQuotes(nodeStart[1]), uses: "", needs: [], with: {} };
      graph.nodes.push(current);
      inWith = false;
      continue;
    }

    if (!current) continue;

    const keyValue = /^([a-zA-Z0-9_-]+):\s*(.*)$/.exec(trimmed);
    if (!keyValue) continue;
    const [, key, value] = keyValue;
    if (key === "uses") {
      current.uses = stripQuotes(value);
      inWith = false;
    } else if (key === "needs") {
      current.needs = value
        .replace(/^\[|\]$/g, "")
        .split(",")
        .map((item) => stripQuotes(item.trim()))
        .filter(Boolean);
      inWith = false;
    } else if (key === "with") {
      inWith = true;
    } else if (inWith) {
      current.with[key] = stripQuotes(value);
    }
  }

  return graph;
}

const yamlScalar = (value: string) => {
  const trimmed = value.trim();
  if (!trimmed) return '""';
  if (/^[a-zA-Z0-9_./:@${}\-]+$/.test(trimmed)) return trimmed;
  return JSON.stringify(trimmed);
};

const uniqueNodeId = (nodes: WorkflowStudioNode[], preferred: string) => {
  const existing = new Set(nodes.map((node) => node.id));
  const base = safeName(preferred, "step");
  if (!existing.has(base)) return base;
  let suffix = 2;
  while (existing.has(`${base}-${suffix}`)) suffix += 1;
  return `${base}-${suffix}`;
};

export function serializeWorkflowGraphLite(graph: WorkflowGraphLite): string {
  const lines = [
    `name: ${yamlScalar(graph.name || "未命名工作流")}`,
    `description: ${yamlScalar(graph.description || "")}`,
  ];

  if (!graph.nodes.length) {
    lines.push("nodes: []");
    return `${lines.join("\n")}\n`;
  }

  lines.push("nodes:");
  for (const node of graph.nodes) {
    lines.push(`  - id: ${yamlScalar(node.id)}`);
    lines.push(`    uses: ${yamlScalar(node.uses)}`);
    if (node.needs.length) {
      lines.push(`    needs: [${node.needs.map(yamlScalar).join(", ")}]`);
    }
    const withEntries = Object.entries(node.with).filter(([key]) => key.trim());
    if (withEntries.length) {
      lines.push("    with:");
      for (const [key, value] of withEntries) {
        lines.push(`      ${safeName(key)}: ${yamlScalar(value)}`);
      }
    }
  }

  return `${lines.join("\n")}\n`;
}

export function updateWorkflowGraphNode(
  graph: WorkflowGraphLite,
  nodeId: string,
  patch: WorkflowGraphNodePatch,
): WorkflowGraphLite {
  const previousNode = graph.nodes.find((node) => node.id === nodeId);
  if (!previousNode) return graph;

  const requestedId = (patch.id ?? previousNode.id).trim();
  const otherNodes = graph.nodes.filter((node) => node.id !== nodeId);
  const nextId = requestedId === previousNode.id ? previousNode.id : uniqueNodeId(otherNodes, requestedId);

  return {
    ...graph,
    nodes: graph.nodes.map((node) => {
      if (node.id === nodeId) {
        return {
          id: nextId,
          uses: patch.uses ?? node.uses,
          needs: patch.needs ?? node.needs,
          with: patch.with ?? node.with,
        };
      }
      return {
        ...node,
        needs: node.needs.map((neededId) => (neededId === nodeId ? nextId : neededId)),
      };
    }),
  };
}

export function addWorkflowGraphNode(
  graph: WorkflowGraphLite,
  node: Partial<WorkflowStudioNode> = {},
): WorkflowGraphLite {
  const nextId = uniqueNodeId(graph.nodes, node.id || `step-${graph.nodes.length + 1}`);
  const nextNode: WorkflowStudioNode = {
    id: nextId,
    uses: node.uses || "",
    needs: [...(node.needs || [])],
    with: { ...(node.with || {}) },
  };
  return { ...graph, nodes: [...graph.nodes, nextNode] };
}

export function deleteWorkflowGraphNode(graph: WorkflowGraphLite, nodeId: string): WorkflowGraphLite {
  return {
    ...graph,
    nodes: graph.nodes
      .filter((node) => node.id !== nodeId)
      .map((node) => ({
        ...node,
        needs: node.needs.filter((neededId) => neededId !== nodeId),
      })),
  };
}

const isConfigured = (value: unknown) =>
  value !== undefined && value !== null && !(typeof value === "string" && value.trim() === "");

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

const normalizeToolOutputs = (tool: LoomToolDefinition): ToolOutputDefinition[] => {
  if (!Array.isArray(tool.outputs)) return [];
  return tool.outputs.filter(isRecord).map((output) => ({
    name: optionalString(output.name),
    label: optionalString(output.label),
    type: optionalString(output.type),
    execution_type: optionalString(output.execution_type),
    executionType: optionalString(output.executionType),
  }));
};

export function collectWorkflowPreviewNodeOptions(
  workflow: WorkflowGraphLite,
  tools: LoomToolDefinition[],
): WorkflowPreviewNodeOption[] {
  const toolMap = toolDefinitionsByIdentity(tools);
  return workflow.nodes.map((node) => {
    const tool = toolMap.get(node.uses);
    const outputs = tool
      ? normalizeToolOutputs(tool).flatMap((output) => {
          const normalized = normalizeOutputType(output);
          const name = output.name?.trim();
          const imageLike = normalized.type === "image" || normalized.executionType.startsWith("image_");
          if (!name || !imageLike) return [];
          return [{ name, label: output.label?.trim() || name }];
        })
      : node.uses === "sticker"
        ? [{ name: "output_image", label: "图像" }]
        : [];
    return {
      nodeId: node.id,
      label: toolLabel(tool, node.id),
      outputs,
    };
  });
}

const toolLabel = (tool: LoomToolDefinition | undefined, fallback: string) => {
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

export function toolDefinitionsByIdentity(
  tools: LoomToolDefinition[],
): Map<string, LoomToolDefinition> {
  const toolMap = new Map<string, LoomToolDefinition>();
  for (const tool of tools) {
    toolMap.set(tool.id, tool);
    const qualifiedId = artPackageIdentity(tool);
    if (qualifiedId) toolMap.set(qualifiedId, tool);
  }
  return toolMap;
}

export function collectWorkflowParamBindingCandidates(
  workflow: WorkflowGraphLite,
  tools: LoomToolDefinition[],
): WorkflowParamBindingCandidate[] {
  const toolMap = toolDefinitionsByIdentity(tools);
  return workflow.nodes.flatMap((node) => {
    const tool = toolMap.get(node.uses);
    if (!tool) return [];
    const nodeLabel = toolLabel(tool, node.id);
    return normalizeToolParams(tool).flatMap((param) => {
      const target = param.id || param.name;
      if (!target || param.disabled) return [];
      const type = mapParamUiType(param);
      const configuredDefault = node.with[target];
      const defaultValue = isConfigured(configuredDefault)
        ? configuredDefault
        : param.default === undefined
          ? ""
          : String(param.default);
      return [{
        key: `${encodeURIComponent(node.id)}::${encodeURIComponent(target)}`,
        nodeId: node.id,
        nodeLabel,
        target,
        paramLabel: param.label || target,
        type,
        executionType: mapParamExecutionType(param, type),
        defaultValue,
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

const normalizeOutputType = (output?: ToolOutputDefinition) => {
  const type = output?.type === "text" ? "string" : output?.type || "string";
  const executionType =
    output?.executionType ||
    output?.execution_type ||
    (type === "image" ? "image_buffer" : type === "file" ? "image_path" : type);
  return { type, executionType };
};

export function inferWorkflowArtInterface(
  workflow: WorkflowGraphLite,
  tools: LoomToolDefinition[],
): WorkflowInterfaceInference {
  const toolMap = toolDefinitionsByIdentity(tools);
  const incomingNodeIds = new Set(workflow.nodes.flatMap((node) => node.needs));
  const usedNames = new Set<string>();
  const inputs: WorkflowInterfacePort[] = [];
  const warnings: string[] = [];
  const bindings: WorkflowExecutionBindings = { inputs: [] };
  let imageInputBound = false;

  const reserveName = (preferred: string) => {
    let candidate = safeName(preferred);
    let suffix = 2;
    while (usedNames.has(candidate)) {
      candidate = `${safeName(preferred)}_${suffix}`;
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

    for (const input of normalizeToolInputs(tool)) {
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

    for (const param of normalizeToolParams(tool)) {
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
  const terminalOutputs = terminalTool ? normalizeToolOutputs(terminalTool) : [];
  const terminalOutput = terminalOutputs.find((output) => normalizeOutputType(output).type === "image") ||
    terminalOutputs[0];
  const normalizedOutput = normalizeOutputType(terminalOutput);
  const terminalNodeId = terminalNode?.id || workflow.nodes[workflow.nodes.length - 1]?.id || "result";
  const outputs: WorkflowInterfacePort[] = [
    {
      name: "result",
      label: terminalOutput?.label || "Result",
      type: normalizedOutput.type,
      executionType: normalizedOutput.executionType,
      bindingNodeId: terminalNodeId,
      bindingTarget: terminalOutput?.name || "result",
      bindingKind: "node_result",
    },
  ];
  bindings.primaryOutput = {
    nodeId: terminalNodeId,
    output: terminalOutput?.name || "result",
    kind: "node_result",
  };

  if (!workflow.nodes.length) warnings.push("工作流 YAML 没有节点。");
  if (!imageInputBound) warnings.push("没有检测到未绑定图像输入；该工作流可能依赖常量或上游文件。");

  return { inputs, outputs, bindings, warnings };
}
