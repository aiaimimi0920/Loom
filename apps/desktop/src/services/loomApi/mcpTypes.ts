// MCP server and call contracts.

export interface LoomMcpServer {
  id: string;
  serverId?: string;
  name: string;
  description?: string;
  transport?: "stdio" | "streamable-http" | "sse";
  command: string;
  args?: string[];
  env?: Record<string, string>;
  url?: string;
  headers?: Record<string, string>;
  enabled?: boolean;
  source?: "package" | "manual";
  package?: {
    qualifiedId: string;
    publisherId: string;
    version: string;
    digest: string;
    packageDir: string;
  };
  tools?: string[];
  credentialRequirements?: Array<{ id: string; label: string; required?: boolean }>;
  credentialRequired?: boolean;
  credentialBound?: boolean;
  usageCount?: number;
  usedByArtIds?: string[];
}

export interface LoomMcpServersResponse {
  servers?: LoomMcpServer[];
}

export interface LoomMcpTestResult {
  success?: boolean;
  tools?: unknown[];
  error?: string;
  server_info?: unknown;
  serverInfo?: unknown;
}

export interface LoomMcpCallToolResponse {
  status?: string;
  jsonrpc?: string;
  id?: number;
  result?: unknown;
  error?: unknown;
}
