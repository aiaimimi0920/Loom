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
    <div
      className="art-hub__surface mcp-hub__surface"
      id="mcp-panel-store"
      role="tabpanel"
      aria-labelledby="mcp-tab-store"
      hidden={hidden}
    >
      <div className="art-store-grid mcp-card-grid">
        {marketServers.length ? marketServers.map((marketItem) => {
          const configured = findInstalledMcpServer(installedServers, marketItem);
          const configuredSnapshot = configured ? testSnapshots[configured.id] : undefined;
          const configuredBusy = configured ? busyServerId === configured.id : false;
          const server = buildMarketplaceServerConfig(marketItem, configured);
          const health = getMarketplaceHealth(marketItem, server, configuredSnapshot);
          const requiresConfiguration = marketItem.requiresManualConfiguration || !health.requiredEnvPresent;
          const sourceTone = configured ? "installed" : marketItem.sourceKind;
          const sourceLabel = configured ? "已安装" : marketItem.sourceKind === "registry" ? "官方仓库" : "Loom 精选";
          const categoryLabel = mcpMarketCategoryLabel(marketItem.category);
          const connectionLabel = marketItem.installOptions.length > 1
            ? `${marketItem.installOptions.length} 种连接方式`
            : marketItem.installOptions[0]?.label || "待配置";
          const identityLabel = connectionLabel.startsWith(`${categoryLabel} ·`)
            ? connectionLabel
            : `${categoryLabel} · ${connectionLabel}`;
          return (
            <article className={`glass-card art-store-card mcp-store-card mcp-store-card--${sourceTone}`} key={marketItem.id}>
              <div className="art-store-card__head">
                <h3>{marketItem.name}</h3>
                <div className="art-store-card__badges">
                  <span className={`mcp-store-card__source mcp-store-card__source--${sourceTone}`}>
                    {sourceLabel}
                  </span>
                  <span className="mcp-store-card__icon" title="MCP" aria-label="MCP"><McpIcon kind="plug" /></span>
                </div>
              </div>
              <div className="art-store-card__body">
                <p className="art-store-card__description" title={marketItem.description}>{marketItem.description}</p>
                <p className="art-store-card__identity" title={marketItem.installOptions.map((option) => option.label).join(" / ")}>
                  {identityLabel}
                </p>
              </div>
              <div className="art-store-card__actions mcp-store-card__actions">
                <button className="art-card-action" type="button" aria-label={`打开 ${marketItem.name} 介绍`} title="打开介绍" onClick={() => onOpenSource(marketItem)}>
                  <McpIcon kind="external" />
                </button>
                {requiresConfiguration && !configured ? (
                  <button
                    className="mcp-store-card__configuration"
                    type="button"
                    aria-label={`配置 ${marketItem.name}`}
                    disabled={operationBusy}
                    onClick={() => onConfigure(marketItem, server)}
                  >
                    安装前需配置
                  </button>
                ) : null}
                {configured ? (
                  <button
                    className="signal-button mcp-store-card__install"
                    type="button"
                    aria-busy={configuredBusy}
                    aria-label={`${configuredSnapshot?.status === "error" ? "重试连接" : "连接"} ${marketItem.name}`}
                    title={configured.enabled === false ? "请先在服务页面启用" : "测试连接"}
                    disabled={operationBusy || configured.enabled === false}
                    onClick={() => onTest(configured)}
                  >
                    {configuredBusy
                      ? <><span className="mcp-busy-indicator" aria-hidden="true" />连接中</>
                      : configuredSnapshot?.status === "success"
                        ? "已连接"
                        : configuredSnapshot?.status === "error"
                          ? "重试"
                          : configured.enabled === false ? "已禁用" : "连接"}
                  </button>
                ) : (
                  <button
                    className="signal-button mcp-store-card__install"
                    type="button"
                    aria-busy={busyMarketplaceId === marketItem.id}
                    disabled={operationBusy}
                    onClick={() => onInstall(marketItem)}
                  >
                    {busyMarketplaceId === marketItem.id ? <><span className="mcp-busy-indicator" aria-hidden="true" />安装中</> : "安装"}
                  </button>
                )}
              </div>
            </article>
          );
        }) : <div className="mcp-hub__empty">没有匹配的 MCP 服务</div>}
      </div>
      <nav className="mcp-hub__pagination" aria-label="MCP 商店分页">
        <span className="mcp-hub__pagination-summary" aria-live="polite">
          {marketTotalPages > 0
            ? `共 ${filteredCount} 个 · 第 ${resolvedMarketPage}/${marketTotalPages} 页`
            : "共 0 个"}
        </span>
        {marketTotalPages > 1 ? (
          <div className="mcp-hub__pagination-controls">
            <button className="ghost-button" type="button" aria-label="上一页" disabled={resolvedMarketPage <= 1} onClick={() => onPageChange(Math.max(1, resolvedMarketPage - 1))}>上一页</button>
            {paginationItems.map((item) => typeof item === "number" ? (
              <button
                className={item === resolvedMarketPage ? "mcp-hub__page mcp-hub__page--active" : "mcp-hub__page"}
                type="button"
                key={item}
                aria-label={`第 ${item} 页`}
                aria-current={item === resolvedMarketPage ? "page" : undefined}
                onClick={() => onPageChange(item)}
              >
                {item}
              </button>
            ) : <span className="mcp-hub__page-ellipsis" aria-hidden="true" key={item}>…</span>)}
            <button className="ghost-button" type="button" aria-label="下一页" disabled={resolvedMarketPage >= marketTotalPages} onClick={() => onPageChange(Math.min(marketTotalPages, resolvedMarketPage + 1))}>下一页</button>
          </div>
        ) : null}
      </nav>
    </div>
  );
}
