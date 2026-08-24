// Defines marketplace view models and the upstream Registry response shape.

export type McpMarketCategory =
  | "Search"
  | "Web"
  | "Local"
  | "Memory"
  | "Browser"
  | "Developer"
  | "Utility"
  | "Reasoning"
  | "Legacy";

export interface McpMarketServer {
  id: string;
  name: string;
  description: string;
  category: McpMarketCategory;
  transport: "stdio" | "streamable-http";
  command: string;
  args: string[];
  env: Record<string, string>;
  url: string;
  headers: Record<string, string>;
  sourceUrl: string;
  sourceLabel: string;
  sourceKind: "registry" | "curated";
  installSource: {
    registry: "npm" | "pypi" | "oci" | "remote";
    packageName: string;
    version?: string;
  };
  requiredEnvKeys?: string[];
  requiredHeaderKeys?: string[];
  author?: string;
  defaultEnabled?: boolean;
  requiresManualConfiguration?: boolean;
  notes?: string;
  installOptions: McpMarketInstallOption[];
}

export interface McpMarketInstallOption {
  id: string;
  label: string;
  transport: "stdio" | "streamable-http";
  command: string;
  args: string[];
  env: Record<string, string>;
  url: string;
  headers: Record<string, string>;
  installSource: McpMarketServer["installSource"];
  requiredEnvKeys?: string[];
  requiredHeaderKeys?: string[];
  requiresManualConfiguration?: boolean;
}

export interface McpMarketplaceTestSnapshot {
  status: "success" | "error";
  toolCount: number;
  testedAt: string;
  error?: string;
}

export interface McpMarketplaceHealth {
  configured: boolean;
  requiredEnvPresent: boolean;
  toolCount?: number;
  lastTested?: string;
  lastError?: string;
  tags: Array<{
    label: string;
    tone: "success" | "warning" | "error" | "neutral";
  }>;
}

export type RegistryPackageType = "npm" | "pypi" | "oci";

export interface McpRegistryEnvironmentVariable {
  name?: string;
  default?: string;
  value?: string;
  isRequired?: boolean;
}

export interface McpRegistryRemoteVariable {
  default?: string;
  value?: string;
  isRequired?: boolean;
  isSecret?: boolean;
  choices?: string[];
}

export interface McpRegistryRemoteHeader extends McpRegistryEnvironmentVariable {
  description?: string;
  isSecret?: boolean;
}

export interface McpRegistryRemote {
  type?: string;
  url?: string;
  variables?: Record<string, McpRegistryRemoteVariable>;
  headers?: McpRegistryRemoteHeader[];
}

export interface McpRegistryRuntimeArgument {
  value?: string;
}

export interface McpRegistryPackageArgument {
  name?: string;
  default?: string;
  value?: string;
  isRequired?: boolean;
  type?: "named" | "positional";
}

export interface McpRegistryPackage {
  registryType?: string;
  identifier?: string;
  version?: string;
  transport?: {
    type?: string;
  };
  runtimeArguments?: McpRegistryRuntimeArgument[];
  packageArguments?: McpRegistryPackageArgument[];
  environmentVariables?: McpRegistryEnvironmentVariable[];
}

export interface McpRegistryLocalizedText {
  title?: string;
  description?: string;
}

export interface McpRegistryServer {
  name?: string;
  title?: string;
  description?: string;
  // Compatible registries may provide real translations even though the
  // Official Registry does not currently define localized text.
  localizations?: Record<string, McpRegistryLocalizedText>;
  repository?: {
    url?: string;
    source?: string;
  };
  websiteUrl?: string;
  packages?: McpRegistryPackage[];
  remotes?: McpRegistryRemote[];
}

export interface McpRegistryEntry {
  server?: McpRegistryServer;
  _meta?: {
    "io.modelcontextprotocol.registry/official"?: {
      status?: string;
      isLatest?: boolean;
      updatedAt?: string;
    };
  };
}

export interface McpRegistryResponse {
  servers?: McpRegistryEntry[];
  metadata?: {
    count?: number;
    nextCursor?: string;
  };
  loomRegistry?: {
    provider?: string;
    source?: "network" | "cache";
    stale?: boolean;
    fetchedAtMs?: number;
  };
}

export interface ParsedMcpKeyValueLines {
  values: Record<string, string>;
  invalidLineNumbers: number[];
}

export type McpPaginationItem = number | "start-ellipsis" | "end-ellipsis";
