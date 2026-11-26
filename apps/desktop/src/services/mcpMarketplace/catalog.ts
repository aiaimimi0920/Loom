// Defines Loom's deliberately small, reviewed MCP catalog.

import { uiText } from "../../i18n/index.ts";
import type { McpMarketCategory, McpMarketInstallOption, McpMarketServer } from "./types.ts";

interface CuratedMcpDefinition {
  id: string;
  nameKey: string;
  descriptionKey: string;
  category: McpMarketCategory;
  command: string;
  buildArgs: () => string[];
  registry: "npm" | "pypi" | "oci";
  packageName: string;
  sourceUrl: string;
  author: string;
  requiredEnvKeys?: string[];
  requiresManualConfiguration?: boolean;
}

function localInstallOptionLabel(registry: "npm" | "pypi" | "oci"): string {
  if (registry === "npm") return uiText("mcp.installOption.localNode");
  if (registry === "pypi") return uiText("mcp.installOption.localPython");
  return uiText("mcp.installOption.localContainer");
}

function curatedLocalMcp(definition: CuratedMcpDefinition): McpMarketServer {
  const env = Object.fromEntries((definition.requiredEnvKeys || []).map((key) => [key, ""]));
  const option: McpMarketInstallOption = {
    id: `stdio:${definition.registry}:${definition.packageName}`,
    get label() {
      return localInstallOptionLabel(definition.registry);
    },
    transport: "stdio",
    command: definition.command,
    get args() {
      return definition.buildArgs();
    },
    env,
    url: "",
    headers: {},
    installSource: { registry: definition.registry, packageName: definition.packageName },
    requiredEnvKeys: definition.requiredEnvKeys,
    requiresManualConfiguration: definition.requiresManualConfiguration,
  };
  return {
    id: definition.id,
    get name() {
      return uiText(definition.nameKey);
    },
    get description() {
      return uiText(definition.descriptionKey);
    },
    category: definition.category,
    transport: "stdio",
    command: option.command,
    get args() {
      return option.args;
    },
    env: option.env,
    url: "",
    headers: {},
    sourceUrl: definition.sourceUrl,
    get sourceLabel() {
      return uiText("mcp.market.source.curated");
    },
    sourceKind: "curated",
    installSource: option.installSource,
    requiredEnvKeys: definition.requiredEnvKeys,
    author: definition.author,
    defaultEnabled: !definition.requiresManualConfiguration && !definition.requiredEnvKeys?.length,
    requiresManualConfiguration: definition.requiresManualConfiguration,
    installOptions: [option],
  };
}

export const MCP_MARKET_SERVERS: readonly McpMarketServer[] = [
  curatedLocalMcp({
    id: "loom.curated/memory",
    nameKey: "mcp.catalog.memory.name",
    descriptionKey: "mcp.catalog.memory.description",
    category: "Memory",
    command: "npx",
    buildArgs: () => ["-y", "@modelcontextprotocol/server-memory"],
    registry: "npm",
    packageName: "@modelcontextprotocol/server-memory",
    sourceUrl: "https://github.com/modelcontextprotocol/servers/tree/main/src/memory",
    author: "Model Context Protocol",
  }),
  curatedLocalMcp({
    id: "loom.curated/filesystem",
    nameKey: "mcp.catalog.filesystem.name",
    descriptionKey: "mcp.catalog.filesystem.description",
    category: "Local",
    command: "npx",
    buildArgs: () => [
      "-y",
      "@modelcontextprotocol/server-filesystem",
      uiText("mcp.catalog.filesystem.argAllowedDirectory"),
    ],
    registry: "npm",
    packageName: "@modelcontextprotocol/server-filesystem",
    sourceUrl: "https://github.com/modelcontextprotocol/servers/tree/main/src/filesystem",
    author: "Model Context Protocol",
    requiresManualConfiguration: true,
  }),
  curatedLocalMcp({
    id: "loom.curated/fetch",
    nameKey: "mcp.catalog.fetch.name",
    descriptionKey: "mcp.catalog.fetch.description",
    category: "Web",
    command: "uvx",
    buildArgs: () => ["mcp-server-fetch"],
    registry: "pypi",
    packageName: "mcp-server-fetch",
    sourceUrl: "https://github.com/modelcontextprotocol/servers/tree/main/src/fetch",
    author: "Model Context Protocol",
  }),
  curatedLocalMcp({
    id: "loom.curated/git",
    nameKey: "mcp.catalog.git.name",
    descriptionKey: "mcp.catalog.git.description",
    category: "Developer",
    command: "uvx",
    buildArgs: () => ["mcp-server-git", "--repository", uiText("mcp.catalog.git.argRepositoryPath")],
    registry: "pypi",
    packageName: "mcp-server-git",
    sourceUrl: "https://github.com/modelcontextprotocol/servers/tree/main/src/git",
    author: "Model Context Protocol",
    requiresManualConfiguration: true,
  }),
  curatedLocalMcp({
    id: "loom.curated/time",
    nameKey: "mcp.catalog.time.name",
    descriptionKey: "mcp.catalog.time.description",
    category: "Utility",
    command: "uvx",
    buildArgs: () => ["mcp-server-time"],
    registry: "pypi",
    packageName: "mcp-server-time",
    sourceUrl: "https://github.com/modelcontextprotocol/servers/tree/main/src/time",
    author: "Model Context Protocol",
  }),
  curatedLocalMcp({
    id: "loom.curated/sequential-thinking",
    nameKey: "mcp.catalog.sequentialThinking.name",
    descriptionKey: "mcp.catalog.sequentialThinking.description",
    category: "Reasoning",
    command: "npx",
    buildArgs: () => ["-y", "@modelcontextprotocol/server-sequential-thinking"],
    registry: "npm",
    packageName: "@modelcontextprotocol/server-sequential-thinking",
    sourceUrl: "https://github.com/modelcontextprotocol/servers/tree/main/src/sequentialthinking",
    author: "Model Context Protocol",
  }),
  curatedLocalMcp({
    id: "loom.curated/playwright",
    nameKey: "mcp.catalog.playwright.name",
    descriptionKey: "mcp.catalog.playwright.description",
    category: "Browser",
    command: "npx",
    buildArgs: () => ["-y", "@playwright/mcp@latest"],
    registry: "npm",
    packageName: "@playwright/mcp",
    sourceUrl: "https://github.com/microsoft/playwright-mcp",
    author: "Microsoft",
  }),
  curatedLocalMcp({
    id: "loom.curated/github",
    nameKey: "mcp.catalog.github.name",
    descriptionKey: "mcp.catalog.github.description",
    category: "Developer",
    command: "docker",
    buildArgs: () => ["run", "-i", "--rm", "-e", "GITHUB_PERSONAL_ACCESS_TOKEN", "ghcr.io/github/github-mcp-server"],
    registry: "oci",
    packageName: "ghcr.io/github/github-mcp-server",
    sourceUrl: "https://github.com/github/github-mcp-server",
    author: "GitHub",
    requiredEnvKeys: ["GITHUB_PERSONAL_ACCESS_TOKEN"],
  }),
];
