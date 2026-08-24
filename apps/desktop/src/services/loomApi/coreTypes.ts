// Core daemon, capability, tool, and workflow contracts shared by desktop clients.

export type ConnectionState = "online" | "offline";

export interface LoomHealthResponse {
  status?: string;
}

export interface LoomModuleStatus {
  name: string;
  status: string;
  detail?: string | null;
}

export interface LoomStatusResponse {
  status?: string;
  modules?: LoomModuleStatus[];
  [key: string]: unknown;
}

export interface LoomCapability {
  id: string;
  description?: string;
  mode?: string;
  input_schema?: unknown;
  [key: string]: unknown;
}

export interface LoomCapabilitiesResponse {
  capabilities?: LoomCapability[];
}


export interface LoomToolExecution {
  type?: string;
  [key: string]: unknown;
}

export interface LoomToolDefinition {
  id: string;
  name: string;
  description?: string;
  enabled?: boolean;
  execution?: LoomToolExecution;
  inputs?: unknown[];
  outputs?: unknown[];
  params?: unknown[];
  metadata?: unknown;
}

export interface LoomArtRuntimeManifest {
  protocolVersion: "loom.art.runtime.v1" | string;
  entry: {
    command: string;
    args?: string[];
  };
}

export interface LoomToolsResponse {
  tools?: LoomToolDefinition[];
}

export interface LoomWorkflowMetadata {
  id: string;
  name: string;
  description?: string;
  nodeCount?: number;
  updatedAt?: string;
}

export interface LoomWorkflowsResponse {
  workflows?: LoomWorkflowMetadata[];
}

export interface LoomWorkflowBundle extends LoomWorkflowMetadata {
  data: string;
}

export interface LoomWorkflowBundleResponse {
  workflow?: LoomWorkflowBundle;
}
