// Defines Loom's deliberately small, reviewed MCP catalog.

import type { McpMarketCategory, McpMarketInstallOption, McpMarketServer } from "./types.ts";

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
