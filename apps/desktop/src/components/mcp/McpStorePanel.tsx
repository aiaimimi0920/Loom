// Renders marketplace cards and pagination from already-filtered catalog data.

import type { LoomMcpServer } from "../../services/loomApi";
import {
  buildMarketplaceServerConfig,
  findInstalledMcpServer,
  getMarketplaceHealth,
  mcpMarketCategoryLabel,
  type McpMarketServer,
  type McpMarketplaceTestSnapshot,
  type McpPaginationItem,
} from "../../services/mcpMarketplace";
import { McpIcon } from "./McpIcon";
import { ActionTooltip, Badge, Button, EmptyState, IconButton, Panel, TabPanel } from "@neuro/ui";

interface McpStorePanelProps {
  hidden: boolean;
  marketServers: readonly McpMarketServer[];
  installedServers: readonly LoomMcpServer[];
  testSnapshots: Readonly<Record<string, McpMarketplaceTestSnapshot>>;
  busyServerId: string | null;
  busyMarketplaceId: string | null;
  operationBusy: boolean;
  filteredCount: number;
  totalPages: number;
  currentPage: number;
  paginationItems: readonly McpPaginationItem[];
  onPageChange: (page: number) => void;
  onOpenSource: (marketItem: McpMarketServer) => void;
  onConfigure: (marketItem: McpMarketServer, server: LoomMcpServer) => void;
  onInstall: (marketItem: McpMarketServer) => void;
  onTest: (server: LoomMcpServer) => void;
}

export function McpStorePanel({
  hidden,
  marketServers,
  installedServers,
  testSnapshots,
  busyServerId,
  busyMarketplaceId,
  operationBusy,
  filteredCount,
  totalPages,
  currentPage,
  paginationItems,
  onPageChange,
  onOpenSource,
  onConfigure,
  onInstall,
  onTest,
}: McpStorePanelProps) {
  const resolvedMarketPage = currentPage;
  const marketTotalPages = totalPages;
  return (
    <TabPanel value="store" forceMount className="art-hub__surface mcp-hub__surface" hidden={hidden}>
      <div className="mcp-card-grid">
        {marketServers.length ? (
          marketServers.map((marketItem) => {
            const configured = findInstalledMcpServer(installedServers, marketItem);
            const configuredSnapshot = configured ? testSnapshots[configured.id] : undefined;
            const configuredBusy = configured ? busyServerId === configured.id : false;
            const server = buildMarketplaceServerConfig(marketItem, configured);
            const health = getMarketplaceHealth(marketItem, server, configuredSnapshot);
            const requiresConfiguration = marketItem.requiresManualConfiguration || !health.requiredEnvPresent;
            const sourceTone = configured ? "installed" : marketItem.sourceKind;
            const sourceLabel = configured ? "已安装" : marketItem.sourceKind === "registry" ? "官方仓库" : "Loom 精选";
            const categoryLabel = mcpMarketCategoryLabel(marketItem.category);
            const connectionLabel =
              marketItem.installOptions.length > 1
                ? `${marketItem.installOptions.length} 种连接方式`
                : marketItem.installOptions[0]?.label || "待配置";
            const identityLabel = connectionLabel.startsWith(`${categoryLabel} ·`)
              ? connectionLabel
              : `${categoryLabel} · ${connectionLabel}`;
            return (
              <Panel
                className="mcp-card mcp-store-card"
                data-source={sourceTone}
                aria-label={marketItem.name}
                key={marketItem.id}
              >
                <div className="mcp-card__head">
                  <h3>{marketItem.name}</h3>
                  <div className="mcp-card__badges">
                    <Badge tone={configured ? "success" : marketItem.sourceKind === "registry" ? "info" : "neutral"}>
                      {sourceLabel}
                    </Badge>
                    <span className="mcp-card__icon" title="MCP" aria-label="MCP">
                      <McpIcon kind="plug" />
                    </span>
                  </div>
                </div>
                <div className="mcp-card__body">
                  <p className="mcp-card__description" title={marketItem.description}>
                    {marketItem.description}
                  </p>
                  <p
                    className="mcp-card__identity"
                    title={marketItem.installOptions.map((option) => option.label).join(" / ")}
                  >
                    {identityLabel}
                  </p>
                </div>
                <div className="mcp-card__actions">
                  <ActionTooltip label="打开介绍">
                    <IconButton
                      className="mcp-store-card__source-action"
                      compact
                      variant="quiet"
                      label={`打开 ${marketItem.name} 介绍`}
                      onClick={() => onOpenSource(marketItem)}
                    >
                      <McpIcon kind="external" />
                    </IconButton>
                  </ActionTooltip>
                  {requiresConfiguration && !configured ? (
                    <Button
                      compact
                      variant="quiet"
                      aria-label={`配置 ${marketItem.name}`}
                      disabled={operationBusy}
                      onClick={() => onConfigure(marketItem, server)}
                    >
                      安装前需配置
                    </Button>
                  ) : null}
                  {configured ? (
                    <Button
                      compact
                      variant="primary"
                      loading={configuredBusy}
                      aria-label={`${configuredSnapshot?.status === "error" ? "重试连接" : "连接"} ${marketItem.name}`}
                      title={configured.enabled === false ? "请先在服务页面启用" : "测试连接"}
                      disabled={operationBusy || configured.enabled === false}
                      onClick={() => onTest(configured)}
                    >
                      {configuredSnapshot?.status === "success"
                        ? "已连接"
                        : configuredSnapshot?.status === "error"
                          ? "重试"
                          : configured.enabled === false
                            ? "已禁用"
                            : "连接"}
                    </Button>
                  ) : (
                    <Button
                      compact
                      variant="primary"
                      loading={busyMarketplaceId === marketItem.id}
                      disabled={operationBusy}
                      onClick={() => onInstall(marketItem)}
                    >
                      安装
                    </Button>
                  )}
                </div>
              </Panel>
            );
          })
        ) : (
          <EmptyState title="没有匹配的 MCP 服务" />
        )}
      </div>
      <nav className="mcp-hub__pagination" aria-label="MCP 商店分页">
        <span className="mcp-hub__pagination-summary" aria-live="polite">
          {marketTotalPages > 0
            ? `共 ${filteredCount} 个 · 第 ${resolvedMarketPage}/${marketTotalPages} 页`
            : "共 0 个"}
        </span>
        {marketTotalPages > 1 ? (
          <div className="mcp-hub__pagination-controls">
            <Button
              compact
              aria-label="上一页"
              disabled={resolvedMarketPage <= 1}
              onClick={() => onPageChange(Math.max(1, resolvedMarketPage - 1))}
            >
              上一页
            </Button>
            {paginationItems.map((item) =>
              typeof item === "number" ? (
                <Button
                  compact
                  key={item}
                  aria-label={`第 ${item} 页`}
                  aria-current={item === resolvedMarketPage ? "page" : undefined}
                  onClick={() => onPageChange(item)}
                >
                  {item}
                </Button>
              ) : (
                <span className="mcp-hub__page-ellipsis" aria-hidden="true" key={item}>
                  …
                </span>
              ),
            )}
            <Button
              compact
              aria-label="下一页"
              disabled={resolvedMarketPage >= marketTotalPages}
              onClick={() => onPageChange(Math.min(marketTotalPages, resolvedMarketPage + 1))}
            >
              下一页
            </Button>
          </div>
        ) : null}
      </nav>
    </TabPanel>
  );
}
