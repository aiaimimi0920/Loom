// Renders marketplace cards and pagination from already-filtered catalog data.

import { ActionTooltip, Badge, Button, EmptyState, IconButton, Panel, TabPanel } from "@neuro/ui";
import { useUiText } from "../../i18n/index.ts";
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
  const { t, tf } = useUiText();
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
            const sourceLabel = configured
              ? t("mcp.health.installed")
              : marketItem.sourceKind === "registry"
                ? t("mcp.market.source.registry")
                : t("mcp.market.source.curated");
            const categoryLabel = mcpMarketCategoryLabel(marketItem.category);
            const connectionLabel =
              marketItem.installOptions.length > 1
                ? tf("mcp.store.connectionCount", { count: marketItem.installOptions.length })
                : marketItem.installOptions[0]?.label || t("mcp.store.pendingConfig");
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
                    <span className="mcp-card__icon" title={t("mcp.icon.label")} aria-label={t("mcp.icon.label")}>
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
                  <ActionTooltip label={t("mcp.store.openOverview")}>
                    <IconButton
                      className="mcp-store-card__source-action"
                      compact
                      variant="quiet"
                      label={tf("mcp.store.openOverviewFor", { name: marketItem.name })}
                      onClick={() => onOpenSource(marketItem)}
                    >
                      <McpIcon kind="external" />
                    </IconButton>
                  </ActionTooltip>
                  {requiresConfiguration && !configured ? (
                    <Button
                      compact
                      variant="quiet"
                      aria-label={tf("mcp.store.configureFor", { name: marketItem.name })}
                      disabled={operationBusy}
                      onClick={() => onConfigure(marketItem, server)}
                    >
                      {t("mcp.store.configureBeforeInstall")}
                    </Button>
                  ) : null}
                  {configured ? (
                    <Button
                      compact
                      variant="primary"
                      loading={configuredBusy}
                      aria-label={
                        configuredSnapshot?.status === "error"
                          ? tf("mcp.store.retryConnectionFor", { name: marketItem.name })
                          : tf("mcp.store.connectFor", { name: marketItem.name })
                      }
                      title={
                        configured.enabled === false
                          ? t("mcp.store.enableInServicesFirst")
                          : t("mcp.action.testConnection")
                      }
                      disabled={operationBusy || configured.enabled === false}
                      onClick={() => onTest(configured)}
                    >
                      {configuredSnapshot?.status === "success"
                        ? t("mcp.store.connected")
                        : configuredSnapshot?.status === "error"
                          ? t("mcp.action.retry")
                          : configured.enabled === false
                            ? t("mcp.status.disabled")
                            : t("mcp.action.connect")}
                    </Button>
                  ) : (
                    <Button
                      compact
                      variant="primary"
                      loading={busyMarketplaceId === marketItem.id}
                      disabled={operationBusy}
                      onClick={() => onInstall(marketItem)}
                    >
                      {t("mcp.action.install")}
                    </Button>
                  )}
                </div>
              </Panel>
            );
          })
        ) : (
          <EmptyState title={t("mcp.store.empty")} />
        )}
      </div>
      <nav className="mcp-hub__pagination" aria-label={t("mcp.store.paginationAria")}>
        <span className="mcp-hub__pagination-summary" aria-live="polite">
          {marketTotalPages > 0
            ? tf("mcp.store.paginationSummary", {
                count: filteredCount,
                page: resolvedMarketPage,
                totalPages: marketTotalPages,
              })
            : t("mcp.store.paginationZero")}
        </span>
        {marketTotalPages > 1 ? (
          <div className="mcp-hub__pagination-controls">
            <Button
              compact
              aria-label={t("mcp.store.prevPage")}
              disabled={resolvedMarketPage <= 1}
              onClick={() => onPageChange(Math.max(1, resolvedMarketPage - 1))}
            >
              {t("mcp.store.prevPage")}
            </Button>
            {paginationItems.map((item) =>
              typeof item === "number" ? (
                <Button
                  compact
                  key={item}
                  aria-label={tf("mcp.store.pageNumber", { page: item })}
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
              aria-label={t("mcp.store.nextPage")}
              disabled={resolvedMarketPage >= marketTotalPages}
              onClick={() => onPageChange(Math.min(marketTotalPages, resolvedMarketPage + 1))}
            >
              {t("mcp.store.nextPage")}
            </Button>
          </div>
        ) : null}
      </nav>
    </TabPanel>
  );
}
