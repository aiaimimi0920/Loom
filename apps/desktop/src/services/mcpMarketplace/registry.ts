// Normalizes external MCP Registry entries into Loom marketplace install choices.

import { dedupeMarketplaceServers, isValidMcpRemoteUrl } from "./helpers.ts";
import type {
  McpMarketCategory,
  McpMarketInstallOption,
  McpMarketServer,
  McpRegistryEntry,
  McpRegistryEnvironmentVariable,
  McpRegistryLocalizedText,
  McpRegistryPackage,
  McpRegistryPackageArgument,
  McpRegistryRemote,
  McpRegistryResponse,
  McpRegistryRuntimeArgument,
  McpRegistryServer,
  RegistryPackageType,
} from "./types.ts";

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
    const allowedValue = variable.choices?.length && value !== undefined
      ? variable.choices.includes(value) ? value : undefined
      : value;
    if (variable.isSecret === true || allowedValue === undefined || allowedValue === "") {
      unresolvedVariable = true;
    } else if (allowedValue !== undefined) {
      url = url.replaceAll(`{${name}}`, allowedValue);
    }
  });
  unresolvedVariable ||= /\{[^}]+\}/.test(url);
  const templateUrl = url.replace(/\{[^}]+\}/g, "loom-variable");
  if (!isValidMcpRemoteUrl(templateUrl) || (!unresolvedVariable && !isValidMcpRemoteUrl(url))) return null;
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
