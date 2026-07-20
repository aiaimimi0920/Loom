import type { LoomMcpServer } from "./loomApi";

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
  command: string;
  args: string[];
  env: Record<string, string>;
  sourceUrl: string;
  sourceLabel: string;
  sourceKind: "registry" | "curated";
  installSource: {
    registry: "npm" | "pypi" | "oci" | "github" | "ghcr";
    packageName: string;
    version?: string;
  };
  requiredEnvKeys?: string[];
  author?: string;
  defaultEnabled?: boolean;
  requiresManualConfiguration?: boolean;
  notes?: string;
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

type RegistryPackageType = "npm" | "pypi" | "oci";

interface McpRegistryEnvironmentVariable {
  name?: string;
  default?: string;
  value?: string;
  isRequired?: boolean;
}

interface McpRegistryRuntimeArgument {
  value?: string;
}

interface McpRegistryPackageArgument {
  name?: string;
  default?: string;
  value?: string;
  isRequired?: boolean;
  type?: "named" | "positional";
}

interface McpRegistryPackage {
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

interface McpRegistryServer {
  name?: string;
  title?: string;
  description?: string;
  repository?: {
    url?: string;
    source?: string;
  };
  websiteUrl?: string;
  packages?: McpRegistryPackage[];
}

interface McpRegistryEntry {
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
}

export const MCP_MARKET_CATEGORIES: readonly McpMarketCategory[] = [
  "Search",
  "Web",
  "Local",
  "Memory",
  "Browser",
  "Developer",
  "Utility",
  "Reasoning",
  "Legacy",
];

export const MCP_MARKET_CATEGORY_LABELS: Record<McpMarketCategory, string> = {
  Search: "搜索",
  Web: "网页",
  Local: "本地",
  Memory: "记忆",
  Browser: "浏览器",
  Developer: "开发",
  Utility: "工具",
  Reasoning: "推理",
  Legacy: "兼容",
};

export function mcpMarketCategoryLabel(category: McpMarketCategory): string {
  return MCP_MARKET_CATEGORY_LABELS[category] || category;
}

export const MCP_MARKET_SERVERS: readonly McpMarketServer[] = [
  {
    id: "brave-search",
    name: "Brave Search",
    description: "通过 Brave Search 搜索网页、本地、图片、视频、新闻和摘要结果。",
    category: "Search",
    command: "npx",
    args: ["-y", "@brave/brave-search-mcp-server", "--transport", "stdio"],
    env: { BRAVE_API_KEY: "" },
    sourceUrl: "https://github.com/brave/brave-search-mcp-server",
    sourceLabel: "Brave 官方 MCP 服务",
    sourceKind: "curated",
    installSource: {
      registry: "npm",
      packageName: "@brave/brave-search-mcp-server",
    },
    requiredEnvKeys: ["BRAVE_API_KEY"],
  },
  {
    id: "fetch",
    name: "Fetch",
    description: "抓取网页并转换为适合模型读取的内容。",
    category: "Web",
    command: "uvx",
    args: ["mcp-server-fetch"],
    env: {},
    sourceUrl: "https://github.com/modelcontextprotocol/servers/tree/main/src/fetch",
    sourceLabel: "MCP 参考服务",
    sourceKind: "curated",
    installSource: {
      registry: "pypi",
      packageName: "mcp-server-fetch",
    },
  },
  {
    id: "filesystem",
    name: "Filesystem",
    description: "只允许访问明确加入白名单的本地目录。",
    category: "Local",
    command: "npx",
    args: ["-y", "@modelcontextprotocol/server-filesystem", "<allowed-directory>"],
    env: {},
    sourceUrl: "https://github.com/modelcontextprotocol/servers/tree/main/src/filesystem",
    sourceLabel: "MCP 参考服务",
    sourceKind: "curated",
    installSource: {
      registry: "npm",
      packageName: "@modelcontextprotocol/server-filesystem",
    },
    defaultEnabled: false,
    requiresManualConfiguration: true,
    notes: "先把 <allowed-directory> 替换为允许读写的精确目录，再启用该服务。",
  },
  {
    id: "playwright",
    name: "Playwright Browser",
    description: "提供结构化页面快照和网页交互工具的浏览器自动化服务。",
    category: "Browser",
    command: "npx",
    args: ["-y", "@playwright/mcp@latest"],
    env: {},
    sourceUrl: "https://github.com/microsoft/playwright-mcp",
    sourceLabel: "Microsoft Playwright MCP 服务",
    sourceKind: "curated",
    installSource: {
      registry: "npm",
      packageName: "@playwright/mcp",
    },
  },
  {
    id: "memory",
    name: "Memory",
    description: "用于跨会话保留上下文的知识图谱记忆服务。",
    category: "Memory",
    command: "npx",
    args: ["-y", "@modelcontextprotocol/server-memory"],
    env: {},
    sourceUrl: "https://github.com/modelcontextprotocol/servers/tree/main/src/memory",
    sourceLabel: "MCP 参考服务",
    sourceKind: "curated",
    installSource: {
      registry: "npm",
      packageName: "@modelcontextprotocol/server-memory",
    },
  },
  {
    id: "github",
    name: "GitHub",
    description: "通过 GitHub MCP 服务自动处理仓库、Issue、Pull Request 和代码搜索。",
    category: "Developer",
    command: "docker",
    args: ["run", "-i", "--rm", "-e", "GITHUB_PERSONAL_ACCESS_TOKEN", "ghcr.io/github/github-mcp-server"],
    env: { GITHUB_PERSONAL_ACCESS_TOKEN: "" },
    sourceUrl: "https://github.com/github/github-mcp-server",
    sourceLabel: "GitHub 官方 MCP 服务",
    sourceKind: "curated",
    installSource: {
      registry: "ghcr",
      packageName: "ghcr.io/github/github-mcp-server",
    },
    requiredEnvKeys: ["GITHUB_PERSONAL_ACCESS_TOKEN"],
    defaultEnabled: false,
    notes: "Requires Docker and a GitHub token. Keep it disabled until the token is configured.",
  },
];

export function mapRegistryResponseToMarketplace(response: McpRegistryResponse): McpMarketServer[] {
  return dedupeMarketplaceServers(
    (response.servers || [])
      .filter((entry) => entry._meta?.["io.modelcontextprotocol.registry/official"]?.status !== "deprecated")
      .filter((entry) => entry._meta?.["io.modelcontextprotocol.registry/official"]?.isLatest !== false)
      .map(registryEntryToMarketplaceServer)
      .filter((server): server is McpMarketServer => Boolean(server)),
  );
}

export function mergeRegistryAndCuratedMarketplace(
  registryServers: readonly McpMarketServer[],
  curatedServers: readonly McpMarketServer[],
): McpMarketServer[] {
  return dedupeMarketplaceServers([...registryServers, ...curatedServers]);
}

export function buildMarketplaceServerConfig(
  marketItem: McpMarketServer,
  existing?: LoomMcpServer,
): LoomMcpServer {
  return {
    id: existing?.id || marketItem.id,
    name: marketItem.name,
    description: marketItem.description,
    command: marketItem.command,
    args: [...marketItem.args],
    env: mergeMarketplaceEnv(marketItem.env, existing?.env),
    enabled: existing?.enabled ?? marketItem.defaultEnabled ?? true,
  };
}

export function getMarketplaceHealth(
  marketItem: McpMarketServer,
  configuredServer?: LoomMcpServer,
  testSnapshot?: McpMarketplaceTestSnapshot,
): McpMarketplaceHealth {
  const requiredEnvKeys = getRequiredEnvKeys(marketItem);
  const requiredEnvPresent =
    requiredEnvKeys.length === 0 ||
    requiredEnvKeys.every((key) => Boolean(configuredServer?.env?.[key]?.trim()));
  const tags: McpMarketplaceHealth["tags"] = [];

  if (configuredServer) {
    tags.push({ label: "Configured", tone: "success" });
    tags.push({
      label: configuredServer.enabled === false ? "Disabled" : "Enabled",
      tone: configuredServer.enabled === false ? "neutral" : "success",
    });
  }

  if (marketItem.requiresManualConfiguration) {
    tags.push({ label: "需要配置", tone: "warning" });
  }

  if (requiredEnvKeys.length > 0) {
    tags.push({
      label: requiredEnvPresent ? "密钥已填" : "缺少密钥",
      tone: requiredEnvPresent ? "success" : "error",
    });
  }

  if (testSnapshot?.status === "success") {
    tags.push({ label: `已发现工具 ${testSnapshot.toolCount}`, tone: "success" });
  } else if (testSnapshot?.status === "error") {
    tags.push({ label: "测试失败", tone: "error" });
  }

  return {
    configured: Boolean(configuredServer),
    requiredEnvPresent,
    toolCount: testSnapshot?.status === "success" ? testSnapshot.toolCount : undefined,
    lastTested: testSnapshot?.testedAt,
    lastError: testSnapshot?.status === "error" ? testSnapshot.error : undefined,
    tags,
  };
}

function registryEntryToMarketplaceServer(entry: McpRegistryEntry): McpMarketServer | null {
  const registryServer = entry.server;
  if (!registryServer?.name) return null;

  const selectedPackage = selectInstallableRegistryPackage(registryServer.packages || []);
  if (!selectedPackage?.identifier) return null;

  const installCommand = buildRegistryInstallCommand(selectedPackage);
  if (!installCommand) return null;

  const env = buildRegistryEnv(selectedPackage.environmentVariables || []);
  const requiredEnvKeys = (selectedPackage.environmentVariables || [])
    .filter((item) => item.isRequired === true && item.name)
    .map((item) => item.name as string);
  const sourceUrl = registryServer.repository?.url || registryServer.websiteUrl || "https://registry.modelcontextprotocol.io";
  const title = registryServer.title?.trim() || readableNameFromRegistryId(registryServer.name);
  const description = registryServer.description?.trim() || "MCP Registry 服务。";
  const category = inferRegistryCategory(`${title} ${description} ${registryServer.name}`);

  return {
    id: registryServer.name,
    name: title,
    description,
    category,
    command: installCommand.command,
    args: installCommand.args,
    env,
    sourceUrl,
    sourceLabel: "MCP Registry",
    sourceKind: "registry",
    installSource: {
      registry: normalizeRegistryType(selectedPackage.registryType),
      packageName: selectedPackage.identifier,
      version: normalizeVersion(selectedPackage.version),
    },
    requiredEnvKeys: requiredEnvKeys.length > 0 ? requiredEnvKeys : undefined,
    author: registryServer.repository?.source || registryServer.name.split("/")[0] || "registry",
    defaultEnabled: requiredEnvKeys.length === 0,
    requiresManualConfiguration: hasRequiredPackageArguments(selectedPackage),
    notes: buildRegistryNotes(entry, selectedPackage),
  };
}

function selectInstallableRegistryPackage(packages: readonly McpRegistryPackage[]): McpRegistryPackage | undefined {
  const stdioPackages = packages.filter((item) => item.transport?.type === "stdio" && item.identifier);
  return (
    stdioPackages.find((item) => normalizeRegistryType(item.registryType) === "npm") ||
    stdioPackages.find((item) => normalizeRegistryType(item.registryType) === "pypi") ||
    stdioPackages.find((item) => normalizeRegistryType(item.registryType) === "oci")
  );
}

function buildRegistryInstallCommand(pkg: McpRegistryPackage): { command: string; args: string[] } | null {
  const registry = normalizeRegistryType(pkg.registryType);
  if (!pkg.identifier) return null;

  if (registry === "npm") {
    const runtimeArgs = valuesFromRuntimeArguments(pkg.runtimeArguments);
    return {
      command: "npx",
      args: [...(runtimeArgs.length > 0 ? runtimeArgs : ["-y"]), packageWithVersion(pkg.identifier, pkg.version, "@"), ...valuesFromPackageArguments(pkg.packageArguments)],
    };
  }

  if (registry === "pypi") {
    return {
      command: "uvx",
      args: [packageWithVersion(pkg.identifier, pkg.version, "=="), ...valuesFromPackageArguments(pkg.packageArguments)],
    };
  }

  if (registry === "oci") {
    return {
      command: "docker",
      args: ["run", "-i", "--rm", pkg.identifier],
    };
  }

  return null;
}

function buildRegistryEnv(environmentVariables: readonly McpRegistryEnvironmentVariable[]): Record<string, string> {
  const env: Record<string, string> = {};
  environmentVariables.forEach((item) => {
    if (item.name) env[item.name] = item.value ?? item.default ?? "";
  });
  return env;
}

function valuesFromRuntimeArguments(runtimeArguments: readonly McpRegistryRuntimeArgument[] = []): string[] {
  return runtimeArguments.map((item) => item.value?.trim()).filter((value): value is string => Boolean(value));
}

function valuesFromPackageArguments(packageArguments: readonly McpRegistryPackageArgument[] = []): string[] {
  return packageArguments.flatMap((item) => {
    const value = item.value ?? item.default ?? (item.name ? `<${item.name}>` : undefined);
    if (!value) return [];
    if (item.type === "named" && item.name) return [`--${item.name}`, value];
    return [value];
  });
}

function packageWithVersion(identifier: string, version: string | undefined, separator: "@" | "=="): string {
  const normalizedVersion = normalizeVersion(version);
  if (!normalizedVersion) return identifier;
  if (separator === "@") return `${identifier}@${normalizedVersion}`;
  return `${identifier}${separator}${normalizedVersion}`;
}

function normalizeVersion(version: string | undefined): string | undefined {
  const trimmed = version?.trim();
  if (!trimmed || trimmed === "latest") return undefined;
  return trimmed;
}

function normalizeRegistryType(registryType: string | undefined): RegistryPackageType {
  if (registryType === "pypi" || registryType === "oci") return registryType;
  return "npm";
}

function hasRequiredPackageArguments(pkg: McpRegistryPackage): boolean {
  return (pkg.packageArguments || []).some((item) => item.isRequired === true);
}

function buildRegistryNotes(entry: McpRegistryEntry, pkg: McpRegistryPackage): string | undefined {
  const official = entry._meta?.["io.modelcontextprotocol.registry/official"];
  const notes: string[] = [];
  if (official?.updatedAt) notes.push(`注册表更新时间 ${official.updatedAt}`);
  if (hasRequiredPackageArguments(pkg)) notes.push("启用前需要手动补充包参数。");
  return notes.length > 0 ? notes.join(" | ") : undefined;
}

function readableNameFromRegistryId(id: string): string {
  const name = id.split("/").pop() || id;
  return name
    .replace(/^mcp[-_]/i, "")
    .replace(/[-_]?mcp([-_]?server)?$/i, "")
    .replace(/[-_]+/g, " ")
    .replace(/\b\w/g, (char) => char.toUpperCase());
}

function inferRegistryCategory(text: string): McpMarketCategory {
  const normalized = text.toLowerCase();
  if (normalized.includes("search")) return "Search";
  if (normalized.includes("browser") || normalized.includes("playwright") || normalized.includes("puppeteer")) return "Browser";
  if (normalized.includes("file") || normalized.includes("filesystem")) return "Local";
  if (normalized.includes("memory")) return "Memory";
  if (normalized.includes("github") || normalized.includes("git") || normalized.includes("repository")) return "Developer";
  if (normalized.includes("fetch") || normalized.includes("web") || normalized.includes("http")) return "Web";
  if (normalized.includes("think") || normalized.includes("reason")) return "Reasoning";
  return "Utility";
}

function getRequiredEnvKeys(marketItem: McpMarketServer): string[] {
  if (marketItem.requiredEnvKeys) return marketItem.requiredEnvKeys;
  return Object.entries(marketItem.env)
    .filter(([, value]) => value.trim() === "")
    .map(([key]) => key);
}

function mergeMarketplaceEnv(
  marketEnv: Record<string, string>,
  existingEnv: Record<string, string> = {},
): Record<string, string> {
  const merged: Record<string, string> = { ...existingEnv };
  Object.entries(marketEnv).forEach(([key, value]) => {
    const existingValue = merged[key];
    if (existingValue === undefined || (existingValue.trim() === "" && value.trim() !== "")) {
      merged[key] = value;
    }
  });
  return merged;
}

function dedupeMarketplaceServers(servers: readonly McpMarketServer[]): McpMarketServer[] {
  const seen = new Set<string>();
  const result: McpMarketServer[] = [];
  servers.forEach((server) => {
    if (seen.has(server.id)) return;
    seen.add(server.id);
    result.push(server);
  });
  return result;
}
