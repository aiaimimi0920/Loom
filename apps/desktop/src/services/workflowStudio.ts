// Public compatibility facade. Implementation modules own parsing, graph editing, registry projection,
// and interface inference independently so callers retain the original import path and named exports.
export type {
  CurlImportResult,
  McpToolSchemaImportResult,
  ParsedPort,
  RawCommandImportResult,
  ToolInputDefinition,
  ToolParamDefinition,
  WorkflowBindingKind,
  WorkflowExecutionBindings,
  WorkflowGraphLite,
  WorkflowGraphNodePatch,
  WorkflowInputBinding,
  WorkflowInterfaceInference,
  WorkflowInterfacePort,
  WorkflowOutputBinding,
  WorkflowParamBindingCandidate,
  WorkflowPortType,
  WorkflowPreviewNodeOption,
  WorkflowStudioNode,
} from "./workflowStudio/types.ts";

export { parseCurlCommand, parseRawCommand } from "./workflowStudio/commands.ts";
export {
  addWorkflowGraphNode,
  deleteWorkflowGraphNode,
  parseWorkflowYamlLite,
  serializeWorkflowGraphLite,
  updateWorkflowGraphNode,
} from "./workflowStudio/graph.ts";
export { inferWorkflowArtInterface } from "./workflowStudio/inference.ts";
export { portsFromMcpToolSchema } from "./workflowStudio/mcp.ts";
export { autoTemplateResponse, parseTemplate } from "./workflowStudio/templates.ts";
export {
  collectWorkflowParamBindingCandidates,
  collectWorkflowPreviewNodeOptions,
  mapParamExecutionType,
  mapParamUiType,
  normalizeInputExecutionType,
  normalizeToolInputs,
  normalizeToolParams,
  toolDefinitionsByIdentity,
} from "./workflowStudio/tools.ts";
