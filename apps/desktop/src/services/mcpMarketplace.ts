// Stable public facade for MCP marketplace catalog, Registry and configuration concerns.

export { MCP_MARKET_SERVERS } from "./mcpMarketplace/catalog.ts";
export {
  buildMarketplaceServerConfig,
  getMarketplaceHealth,
  mergeRegistryAndCuratedMarketplace,
} from "./mcpMarketplace/configuration.ts";
export {
  MCP_MARKET_CATEGORIES,
  MCP_MARKET_CATEGORY_LABELS,
  buildMcpPaginationItems,
  findInstalledMcpServer,
  isValidMcpRemoteUrl,
  mcpMarketCategoryLabel,
  parseMcpKeyValueLines,
} from "./mcpMarketplace/helpers.ts";
export { mapRegistryResponseToMarketplace } from "./mcpMarketplace/registry.ts";
export type {
  McpMarketCategory,
  McpMarketInstallOption,
  McpMarketplaceHealth,
  McpMarketplaceTestSnapshot,
  McpMarketServer,
  McpPaginationItem,
  McpRegistryResponse,
  ParsedMcpKeyValueLines,
} from "./mcpMarketplace/types.ts";
