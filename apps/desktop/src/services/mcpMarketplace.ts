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

type RegistryPackageType = "npm" | "pypi" | "oci";

interface McpRegistryEnvironmentVariable {
  name?: string;
  default?: string;
  value?: string;
  isRequired?: boolean;
}

interface McpRegistryRemoteVariable {
  default?: string;
  value?: string;
  isRequired?: boolean;
  isSecret?: boolean;
  choices?: string[];
}

interface McpRegistryRemoteHeader extends McpRegistryEnvironmentVariable {
  description?: string;
  isSecret?: boolean;
}

interface McpRegistryRemote {
  type?: string;
  url?: string;
  variables?: Record<string, McpRegistryRemoteVariable>;
  headers?: McpRegistryRemoteHeader[];
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

interface McpRegistryLocalizedText {
  title?: string;
  description?: string;
}

interface McpRegistryServer {
  name?: string;
  title?: string;
  description?: string;
  // The Official Registry does not currently define localized text. Keep this
  // optional extension so compatible registries can provide real translations
  // without Loom inventing or machine-translating upstream content.
  localizations?: Record<string, McpRegistryLocalizedText>;
  repository?: {
    url?: string;
    source?: string;
  };
  websiteUrl?: string;
  packages?: McpRegistryPackage[];
  remotes?: McpRegistryRemote[];
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

export function parseMcpKeyValueLines(value: string): ParsedMcpKeyValueLines {
  return value.split(/\r?\n/).reduce<ParsedMcpKeyValueLines>((parsed, line, index) => {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith("#")) return parsed;
    const separator = trimmed.indexOf("=");
    if (separator <= 0) {
      parsed.invalidLineNumbers.push(index + 1);
      return parsed;
    }
    const key = trimmed.slice(0, separator).trim();
    if (!key) {
      parsed.invalidLineNumbers.push(index + 1);
      return parsed;
    }
    parsed.values[key] = trimmed.slice(separator + 1).trim();
    return parsed;
  }, { values: {}, invalidLineNumbers: [] });
}

export function isValidMcpRemoteUrl(value: string): boolean {
  try {
    const parsed = new URL(value.trim());
    return ["http:", "https:"].includes(parsed.protocol) && !parsed.username && !parsed.password;
  } catch {
    return false;
  }
}

export function findInstalledMcpServer(
  servers: readonly LoomMcpServer[],
  marketItem: Pick<McpMarketServer, "id">,
): LoomMcpServer | undefined {
  return servers.find((server) => server.id === marketItem.id);
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

interface CuratedMcpDefinition {
  id: string;
  name: string;
  description: string;
  category: McpMarketCategory;
  command: string;
  args: string[];
  registry: "npm" | "pypi" | "oci";
  packageName: string;
  sourceUrl: string;
  author: string;
  requiredEnvKeys?: string[];
  requiresManualConfiguration?: boolean;
}

function curatedLocalMcp(definition: CuratedMcpDefinition): McpMarketServer {
  const env = Object.fromEntries((definition.requiredEnvKeys || []).map((key) => [key, ""]));
  const option: McpMarketInstallOption = {
    id: `stdio:${definition.registry}:${definition.packageName}`,
    label: definition.registry === "npm" ? "本地 · Node.js" : definition.registry === "pypi" ? "本地 · Python" : "本地 · 容器",
    transport: "stdio",
    command: definition.command,
    args: [...definition.args],
    env,
    url: "",
    headers: {},
    installSource: { registry: definition.registry, packageName: definition.packageName },
    requiredEnvKeys: definition.requiredEnvKeys,
    requiresManualConfiguration: definition.requiresManualConfiguration,
  };
  return {
    id: definition.id,
    name: definition.name,
    description: definition.description,
    category: definition.category,
    transport: "stdio",
    command: option.command,
    args: option.args,
    env: option.env,
    url: "",
    headers: {},
    sourceUrl: definition.sourceUrl,
    sourceLabel: "Loom 精选",
    sourceKind: "curated",
    installSource: option.installSource,
    requiredEnvKeys: definition.requiredEnvKeys,
    author: definition.author,
    defaultEnabled: !definition.requiresManualConfiguration && !definition.requiredEnvKeys?.length,
    requiresManualConfiguration: definition.requiresManualConfiguration,
    installOptions: [option],
  };
}

// Loom exposes a deliberately small, reviewed catalog instead of forwarding
// the entire public Registry to end users. The upstream Registry integration
// remains available to the daemon for future catalog maintenance and imports.
export const MCP_MARKET_SERVERS: readonly McpMarketServer[] = [
  curatedLocalMcp({
    id: "loom.curated/memory",
    name: "持久记忆",
    description: "使用知识图谱保存实体、关系和观察结果，为智能体提供可持续维护的本地记忆。",
    category: "Memory",
    command: "npx",
    args: ["-y", "@modelcontextprotocol/server-memory"],
    registry: "npm",
    packageName: "@modelcontextprotocol/server-memory",
    sourceUrl: "https://github.com/modelcontextprotocol/servers/tree/main/src/memory",
    author: "Model Context Protocol",
  }),
  curatedLocalMcp({
    id: "loom.curated/filesystem",
    name: "文件系统",
    description: "在明确授权的目录内读取、编辑、搜索和管理文件，适合本地资料与项目操作。",
    category: "Local",
    command: "npx",
    args: ["-y", "@modelcontextprotocol/server-filesystem", "<允许访问的目录>"],
    registry: "npm",
    packageName: "@modelcontextprotocol/server-filesystem",
    sourceUrl: "https://github.com/modelcontextprotocol/servers/tree/main/src/filesystem",
    author: "Model Context Protocol",
    requiresManualConfiguration: true,
  }),
  curatedLocalMcp({
    id: "loom.curated/fetch",
    name: "网页读取",
    description: "抓取网页内容并转换为适合模型阅读的文本，用于资料检索、阅读和摘要。",
    category: "Web",
    command: "uvx",
    args: ["mcp-server-fetch"],
    registry: "pypi",
    packageName: "mcp-server-fetch",
    sourceUrl: "https://github.com/modelcontextprotocol/servers/tree/main/src/fetch",
    author: "Model Context Protocol",
  }),
  curatedLocalMcp({
    id: "loom.curated/git",
    name: "Git 仓库",
    description: "读取提交、分支和差异，并在指定的本地 Git 仓库中执行受控版本操作。",
    category: "Developer",
    command: "uvx",
    args: ["mcp-server-git", "--repository", "<仓库路径>"],
    registry: "pypi",
    packageName: "mcp-server-git",
    sourceUrl: "https://github.com/modelcontextprotocol/servers/tree/main/src/git",
    author: "Model Context Protocol",
    requiresManualConfiguration: true,
  }),
  curatedLocalMcp({
    id: "loom.curated/time",
    name: "时间与时区",
    description: "查询当前时间并进行时区转换，适合日程、跨地区协作和时间计算。",
    category: "Utility",
    command: "uvx",
    args: ["mcp-server-time"],
    registry: "pypi",
    packageName: "mcp-server-time",
    sourceUrl: "https://github.com/modelcontextprotocol/servers/tree/main/src/time",
    author: "Model Context Protocol",
  }),
  curatedLocalMcp({
    id: "loom.curated/sequential-thinking",
    name: "顺序思考",
    description: "通过可调整的分步推演处理复杂问题，适合规划、分析和需要反复修正的任务。",
    category: "Reasoning",
    command: "npx",
    args: ["-y", "@modelcontextprotocol/server-sequential-thinking"],
    registry: "npm",
    packageName: "@modelcontextprotocol/server-sequential-thinking",
    sourceUrl: "https://github.com/modelcontextprotocol/servers/tree/main/src/sequentialthinking",
    author: "Model Context Protocol",
  }),
  curatedLocalMcp({
    id: "loom.curated/playwright",
    name: "Playwright 浏览器",
    description: "使用结构化页面信息控制浏览器，适合网页交互、自动化验证和可重复测试。",
    category: "Browser",
    command: "npx",
    args: ["-y", "@playwright/mcp@latest"],
    registry: "npm",
    packageName: "@playwright/mcp",
    sourceUrl: "https://github.com/microsoft/playwright-mcp",
    author: "Microsoft",
  }),
  curatedLocalMcp({
    id: "loom.curated/github",
    name: "GitHub",
    description: "连接 GitHub 仓库、议题和拉取请求，适合代码检索、协作和项目维护。",
    category: "Developer",
    command: "docker",
    args: ["run", "-i", "--rm", "-e", "GITHUB_PERSONAL_ACCESS_TOKEN", "ghcr.io/github/github-mcp-server"],
    registry: "oci",
    packageName: "ghcr.io/github/github-mcp-server",
    sourceUrl: "https://github.com/github/github-mcp-server",
    author: "GitHub",
    requiredEnvKeys: ["GITHUB_PERSONAL_ACCESS_TOKEN"],
  }),
];

export function mapRegistryResponseToMarketplace(
  response: McpRegistryResponse,
  locale = "en",
): McpMarketServer[] {
  return dedupeMarketplaceServers(
    (response.servers || [])
      .filter((entry) => !["deprecated", "deleted"].includes(
        entry._meta?.["io.modelcontextprotocol.registry/official"]?.status || "active",
      ))
      .filter((entry) => entry._meta?.["io.modelcontextprotocol.registry/official"]?.isLatest !== false)
      .map((entry) => registryEntryToMarketplaceServer(entry, locale))
      .filter((server): server is McpMarketServer => Boolean(server)),
  );
}

export type McpPaginationItem = number | "start-ellipsis" | "end-ellipsis";

export function buildMcpPaginationItems(currentPage: number, totalPages: number): McpPaginationItem[] {
  const total = Math.max(1, Math.trunc(totalPages));
  const current = Math.min(total, Math.max(1, Math.trunc(currentPage)));
  if (total <= 7) return Array.from({ length: total }, (_, index) => index + 1);

  const pages = new Set([1, total, current - 1, current, current + 1]);
  if (current <= 3) [2, 3, 4].forEach((page) => pages.add(page));
  if (current >= total - 2) [total - 3, total - 2, total - 1].forEach((page) => pages.add(page));
  const ordered = [...pages].filter((page) => page >= 1 && page <= total).sort((left, right) => left - right);
  const items: McpPaginationItem[] = [];
  ordered.forEach((page, index) => {
    const previous = ordered[index - 1];
    if (previous !== undefined && page - previous > 1) {
      items.push(previous === 1 ? "start-ellipsis" : "end-ellipsis");
    }
    items.push(page);
  });
  return items;
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
  installOptionId?: string,
): LoomMcpServer {
  const option = marketItem.installOptions.find((candidate) => candidate.id === installOptionId) ||
    marketItem.installOptions.find((candidate) => candidate.transport === existing?.transport &&
      (candidate.transport === "stdio" ? candidate.command === existing?.command : candidate.url === existing?.url)) ||
    marketItem.installOptions.find((candidate) => candidate.transport === existing?.transport) ||
    marketItem.installOptions[0];
  if (!option) throw new Error(`MCP Registry entry ${marketItem.id} has no install option`);
  return {
    id: existing?.id || marketItem.id,
    name: marketItem.name,
    description: marketItem.description,
    transport: option.transport,
    command: option.command,
    args: [...option.args],
    env: mergeMarketplaceEnv(option.env, existing?.env),
    url: option.transport === "streamable-http" && existing?.transport === "streamable-http" && existing.url
      ? existing.url
      : option.url,
    headers: mergeMarketplaceEnv(option.headers, existing?.headers),
    enabled: existing?.enabled ?? marketItem.defaultEnabled ?? true,
  };
}

export function getMarketplaceHealth(
  marketItem: McpMarketServer,
  configuredServer?: LoomMcpServer,
  testSnapshot?: McpMarketplaceTestSnapshot,
): McpMarketplaceHealth {
  const installOption = marketItem.installOptions.find((option) =>
    configuredServer?.transport === option.transport &&
    (option.transport === "stdio" ? option.command === configuredServer.command : option.url === configuredServer?.url)) ||
    marketItem.installOptions.find((option) => configuredServer?.transport === option.transport) ||
    marketItem.installOptions[0];
  const requiredEnvKeys = installOption?.requiredEnvKeys || [];
  const requiredHeaderKeys = installOption?.requiredHeaderKeys || [];
  const requiredEnvPresent =
    (requiredEnvKeys.length === 0 || requiredEnvKeys.every((key) => Boolean(configuredServer?.env?.[key]?.trim()))) &&
    (requiredHeaderKeys.length === 0 || requiredHeaderKeys.every((key) => Boolean(configuredServer?.headers?.[key]?.trim())));
  const tags: McpMarketplaceHealth["tags"] = [];

  if (configuredServer) {
    tags.push({ label: "已安装", tone: "success" });
    tags.push({
      label: configuredServer.enabled === false ? "已禁用" : "已启用",
      tone: configuredServer.enabled === false ? "neutral" : "success",
    });
  }

  if (installOption?.requiresManualConfiguration) {
    tags.push({ label: "需要配置", tone: "warning" });
  }

  if (requiredEnvKeys.length > 0 || requiredHeaderKeys.length > 0) {
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

function registryEntryToMarketplaceServer(entry: McpRegistryEntry, locale: string): McpMarketServer | null {
  const registryServer = entry.server;
  if (!registryServer?.name) return null;

  const installOptions = buildRegistryInstallOptions(registryServer);
  const selected = installOptions[0];
  if (!selected) return null;
  const sourceUrl = registrySourceUrl(registryServer);
  const localized = findRegistryLocalization(registryServer.localizations, locale);
  const title = localized?.title?.trim() || registryServer.title?.trim() || readableNameFromRegistryId(registryServer.name);
  const description = localized?.description?.trim() || registryServer.description?.trim() || "MCP Registry 服务。";
  const category = inferRegistryCategory(`${title} ${description} ${registryServer.name}`);

  return {
    id: registryServer.name,
    name: title,
    description,
    category,
    transport: selected.transport,
    command: selected.command,
    args: selected.args,
    env: selected.env,
    url: selected.url,
    headers: selected.headers,
    sourceUrl,
    sourceLabel: "MCP Registry",
    sourceKind: "registry",
    installSource: {
      ...selected.installSource,
    },
    requiredEnvKeys: selected.requiredEnvKeys,
    requiredHeaderKeys: selected.requiredHeaderKeys,
    author: registryServer.repository?.source || registryServer.name.split("/")[0] || "registry",
    defaultEnabled: !(selected.requiredEnvKeys?.length || selected.requiredHeaderKeys?.length || selected.requiresManualConfiguration),
    requiresManualConfiguration: selected.requiresManualConfiguration,
    notes: buildRegistryNotes(entry, selected),
    installOptions,
  };
}

function registrySourceUrl(server: McpRegistryServer): string {
  for (const candidate of [server.repository?.url, server.websiteUrl]) {
    const normalized = candidate?.trim().replace(/^git\+https:\/\//i, "https://");
    if (!normalized) continue;
    try {
      const parsed = new URL(normalized);
      if (parsed.protocol === "https:" && !parsed.username && !parsed.password) return parsed.toString();
    } catch {
      // Ignore non-browser repository transports and use the next safe URL.
    }
  }
  return "https://registry.modelcontextprotocol.io";
}

function findRegistryLocalization(
  localizations: Record<string, McpRegistryLocalizedText> | undefined,
  locale: string,
): McpRegistryLocalizedText | undefined {
  if (!localizations) return undefined;
  const requested = locale.trim().replace(/_/g, "-");
  const language = requested.split("-")[0]?.toLowerCase();
  const candidates = [
    requested,
    requested.toLowerCase(),
    ...(language === "zh" ? ["zh-Hans", "zh-CN", "zh"] : []),
    ...(language === "en" ? ["en-US", "en"] : language ? [language] : []),
  ];
  const entries = Object.entries(localizations);
  for (const candidate of candidates) {
    const match = entries.find(([key]) => key.toLowerCase() === candidate.toLowerCase());
    if (match) return match[1];
  }
  return undefined;
}

function buildRegistryInstallOptions(server: McpRegistryServer): McpMarketInstallOption[] {
  const packages = (server.packages || [])
    .filter((item) => item.transport?.type === "stdio" && item.identifier && isSupportedRegistryType(item.registryType))
    .map((pkg) => buildPackageInstallOption(pkg))
    .filter((option): option is McpMarketInstallOption => Boolean(option));
  const remotes = (server.remotes || [])
    .filter((remote) => remote.type === "streamable-http" && remote.url)
    .map(buildRemoteInstallOption)
    .filter((option): option is McpMarketInstallOption => Boolean(option));
  const orderedPackages = ["npm", "pypi", "oci"].flatMap((registry) =>
    packages.filter((option) => option.installSource.registry === registry));
  return dedupeInstallOptions([...orderedPackages, ...remotes]);
}

function buildPackageInstallOption(pkg: McpRegistryPackage): McpMarketInstallOption | null {
  if (!pkg.identifier) return null;
  const installCommand = buildRegistryInstallCommand(pkg);
  if (!installCommand) return null;
  const env = buildRegistryEnv(pkg.environmentVariables || []);
  const requiredEnvKeys = (pkg.environmentVariables || [])
    .filter((item) => item.isRequired === true && item.name)
    .map((item) => item.name as string);
  const registry = normalizeRegistryType(pkg.registryType);
  const version = normalizeVersion(pkg.version);
  return {
    id: `stdio:${registry}:${pkg.identifier}:${version || "latest"}`,
    label: registry === "npm" ? "本地 · Node.js" : registry === "pypi" ? "本地 · Python" : "本地 · 容器",
    transport: "stdio",
    command: installCommand.command,
    args: installCommand.args,
    env,
    url: "",
    headers: {},
    installSource: { registry, packageName: pkg.identifier, version },
    requiredEnvKeys: requiredEnvKeys.length > 0 ? requiredEnvKeys : undefined,
    requiresManualConfiguration: hasRequiredPackageArguments(pkg),
  };
}

function buildRemoteInstallOption(remote: McpRegistryRemote): McpMarketInstallOption | null {
  if (!remote.url) return null;
  let url = remote.url;
  let unresolvedVariable = false;
  Object.entries(remote.variables || {}).forEach(([name, variable]) => {
    const value = variable.value ?? variable.default;
    if (value !== undefined && value !== "") {
      url = url.replaceAll(`{${name}}`, value);
    } else if (variable.isRequired !== false) {
      unresolvedVariable = true;
    }
  });
  unresolvedVariable ||= /\{[^}]+\}/.test(url);
  const headers = buildRegistryEnv(remote.headers || []);
  const requiredHeaderKeys = (remote.headers || [])
    .filter((item) => item.isRequired === true && item.name)
    .map((item) => item.name as string);
  return {
    id: `streamable-http:${remote.url}`,
    label: "远程 · Streamable HTTP",
    transport: "streamable-http",
    command: "",
    args: [],
    env: {},
    url,
    headers,
    installSource: { registry: "remote", packageName: remote.url },
    requiredHeaderKeys: requiredHeaderKeys.length > 0 ? requiredHeaderKeys : undefined,
    requiresManualConfiguration: unresolvedVariable,
  };
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

function isSupportedRegistryType(registryType: string | undefined): boolean {
  return registryType === "npm" || registryType === "pypi" || registryType === "oci";
}

function hasRequiredPackageArguments(pkg: McpRegistryPackage): boolean {
  return (pkg.packageArguments || []).some((item) => item.isRequired === true);
}

function buildRegistryNotes(entry: McpRegistryEntry, option: McpMarketInstallOption): string | undefined {
  const official = entry._meta?.["io.modelcontextprotocol.registry/official"];
  const notes: string[] = [];
  if (official?.updatedAt) notes.push(`注册表更新时间 ${official.updatedAt}`);
  if (option.requiresManualConfiguration) notes.push("启用前需要补充连接参数。");
  return notes.length > 0 ? notes.join(" | ") : undefined;
}

function dedupeInstallOptions(options: readonly McpMarketInstallOption[]): McpMarketInstallOption[] {
  const seen = new Set<string>();
  return options.filter((option) => {
    if (seen.has(option.id)) return false;
    seen.add(option.id);
    return true;
  });
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
