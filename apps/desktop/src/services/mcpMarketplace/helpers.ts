// Owns small deterministic marketplace parsing, display and deduplication helpers.

import type { LoomMcpServer } from "../loomApi.ts";
import type {
  McpMarketCategory,
  McpMarketServer,
  McpPaginationItem,
  ParsedMcpKeyValueLines,
} from "./types.ts";

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

export function dedupeMarketplaceServers(servers: readonly McpMarketServer[]): McpMarketServer[] {
  const seen = new Set<string>();
  const result: McpMarketServer[] = [];
  servers.forEach((server) => {
    if (seen.has(server.id)) return;
    seen.add(server.id);
    result.push(server);
  });
  return result;
}
