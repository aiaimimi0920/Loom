// Renders installed MCP services while the hub retains mutation ownership.

import { ActionTooltip, Badge, EmptyState, IconButton, Panel, TabPanel } from "@neuro/ui";
import { useUiText } from "../../i18n/index.ts";
import type { LoomMcpServer } from "../../services/loomApi";
import type { McpMarketplaceTestSnapshot } from "../../services/mcpMarketplace";
import { McpIcon } from "./McpIcon";

interface McpServicesPanelProps {
  hidden: boolean;
  servers: readonly LoomMcpServer[];
  testSnapshots: Readonly<Record<string, McpMarketplaceTestSnapshot>>;
  busyServerId: string | null;
  operationBusy: boolean;
  onEdit: (server: LoomMcpServer) => void;
  onCredentials: (server: LoomMcpServer) => void;
  onTest: (server: LoomMcpServer) => void;
  onToggle: (server: LoomMcpServer) => void;
  onRemove: (server: LoomMcpServer) => void;
}

export function McpServicesPanel({
  hidden,
  servers,
  testSnapshots,
  busyServerId,
  operationBusy,
  onEdit,
  onCredentials,
  onTest,
  onToggle,
  onRemove,
}: McpServicesPanelProps) {
  const { t, tf } = useUiText();
  return (
    <TabPanel
      value="services"
      forceMount
      className="art-hub__surface mcp-hub__surface"
      aria-busy={busyServerId !== null}
      hidden={hidden}
    >
      <div className="mcp-card-grid">
        {servers.length ? (
          servers.map((server) => {
            const enabled = server.enabled !== false;
            const packaged = server.source === "package" && Boolean(server.package);
            const snapshot = testSnapshots[server.id];
            const busy = busyServerId === server.id;
            const stateLabel = enabled ? t("mcp.status.enabled") : t("mcp.status.disabled");
            const descriptionText = server.description || t("mcp.service.noDescription");
            return (
              <Panel
                className="mcp-card mcp-service-card"
                data-state={enabled ? "enabled" : "disabled"}
                key={server.id}
                title={stateLabel}
                aria-label={tf("mcp.service.cardAria", { name: server.name, state: stateLabel })}
                aria-busy={busy}
              >
                <div className="mcp-card__head">
                  <h3>{server.name}</h3>
                  <span className="mcp-card__icon" title={t("mcp.icon.label")} aria-label={t("mcp.icon.label")}>
                    <McpIcon kind="plug" />
                  </span>
                </div>
                <div className="mcp-card__body">
                  <p className="mcp-card__description" title={descriptionText}>
                    {descriptionText}
                  </p>
                  <p
                    className="mcp-card__identity"
                    title={
                      packaged
                        ? `${server.package?.qualifiedId} · ${server.package?.version}`
                        : server.transport === "streamable-http"
                          ? `${server.id} · ${server.url}`
                          : `${server.id} · ${server.command} ${(server.args || []).join(" ")}`
                    }
                  >
                    {packaged
                      ? tf("mcp.service.identity.package", {
                          id: server.package?.qualifiedId ?? "",
                          version: server.package?.version ?? "",
                        })
                      : server.transport === "streamable-http"
                        ? tf("mcp.service.identity.remote", { url: server.url ?? "" })
                        : tf("mcp.service.identity.manual", { command: server.command })}
                  </p>
                  <div className="mcp-card__status">
                    <Badge tone={enabled ? "success" : "neutral"}>{stateLabel}</Badge>
                    {busy ? (
                      <Badge tone="info" role="status">
                        {t("mcp.service.busy")}
                      </Badge>
                    ) : snapshot ? (
                      <Badge
                        tone={snapshot.status === "success" ? "success" : "danger"}
                        role={snapshot.status === "error" ? "alert" : "status"}
                        title={
                          snapshot.status === "success"
                            ? tf("mcp.service.toolsDiscoveredTitle", { count: snapshot.toolCount })
                            : snapshot.error || t("mcp.service.connectionFailed")
                        }
                      >
                        <span className="mcp-card__status-text">
                          {snapshot.status === "success"
                            ? tf("mcp.service.toolsCount", { count: snapshot.toolCount })
                            : snapshot.error || t("mcp.service.connectionFailed")}
                        </span>
                      </Badge>
                    ) : server.credentialRequired ? (
                      <Badge tone={server.credentialBound ? "success" : "warning"} role="status">
                        {server.credentialBound
                          ? t("mcp.service.credentialConfigured")
                          : t("mcp.service.credentialRequired")}
                      </Badge>
                    ) : (
                      <Badge role="status">{t("mcp.service.credentialNotNeeded")}</Badge>
                    )}
                  </div>
                  {server.usageCount ? (
                    <p className="mcp-card__usage">{tf("mcp.service.usedByArts", { count: server.usageCount })}</p>
                  ) : null}
                </div>
                <div className="mcp-card__actions">
                  <div className="mcp-card__action-buttons">
                    <ActionTooltip label={packaged ? t("mcp.action.configureCredentials") : t("mcp.action.edit")}>
                      <IconButton
                        compact
                        variant="quiet"
                        label={
                          packaged
                            ? tf("mcp.action.configureCredentialsFor", { name: server.name })
                            : tf("mcp.action.editFor", { name: server.name })
                        }
                        disabled={operationBusy}
                        onClick={() => (packaged ? onCredentials(server) : onEdit(server))}
                      >
                        <McpIcon kind="edit" />
                      </IconButton>
                    </ActionTooltip>
                    <ActionTooltip label={t("mcp.action.testConnection")}>
                      <IconButton
                        compact
                        variant="quiet"
                        label={tf("mcp.action.testFor", { name: server.name })}
                        disabled={
                          operationBusy || !enabled || Boolean(server.credentialRequired && !server.credentialBound)
                        }
                        onClick={() => onTest(server)}
                      >
                        <McpIcon kind="test" />
                      </IconButton>
                    </ActionTooltip>
                    <ActionTooltip label={enabled ? t("mcp.action.disable") : t("mcp.action.enable")}>
                      <IconButton
                        compact
                        variant={enabled ? "positive" : "quiet"}
                        label={
                          enabled
                            ? tf("mcp.action.disableFor", { name: server.name })
                            : tf("mcp.action.enableFor", { name: server.name })
                        }
                        disabled={operationBusy}
                        onClick={() => onToggle(server)}
                      >
                        <McpIcon kind="power" />
                      </IconButton>
                    </ActionTooltip>
                    <ActionTooltip label={t("mcp.action.delete")}>
                      <IconButton
                        compact
                        variant="danger"
                        label={tf("mcp.action.deleteFor", { name: server.name })}
                        disabled={operationBusy}
                        onClick={() => onRemove(server)}
                      >
                        <McpIcon kind="trash" />
                      </IconButton>
                    </ActionTooltip>
                  </div>
                </div>
              </Panel>
            );
          })
        ) : (
          <EmptyState title={t("mcp.service.empty")} />
        )}
      </div>
    </TabPanel>
  );
}
