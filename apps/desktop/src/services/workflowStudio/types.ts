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

export interface ToolOutputDefinition {
  name?: string;
  label?: string;
  type?: string;
  execution_type?: string;
  executionType?: string;
}
