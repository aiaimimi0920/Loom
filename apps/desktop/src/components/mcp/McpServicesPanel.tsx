// Renders installed MCP services while the hub retains mutation ownership.

import type { LoomMcpServer } from "../../services/loomApi";
import type { McpMarketplaceTestSnapshot } from "../../services/mcpMarketplace";
import { McpIcon } from "./McpIcon";
import { ActionTooltip, Badge, EmptyState, IconButton, Panel, TabPanel } from "@neuro/ui";

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
            return (
              <Panel
                className="mcp-card mcp-service-card"
                data-state={enabled ? "enabled" : "disabled"}
                key={server.id}
                title={enabled ? "已启用" : "已禁用"}
                aria-label={`${server.name}，${enabled ? "已启用" : "已禁用"}`}
                aria-busy={busy}
              >
                <div className="mcp-card__head">
                  <h3>{server.name}</h3>
                  <span className="mcp-card__icon" title="MCP" aria-label="MCP">
                    <McpIcon kind="plug" />
                  </span>
                </div>
                <div className="mcp-card__body">
                  <p className="mcp-card__description" title={server.description || "暂无描述"}>
                    {server.description || "暂无描述"}
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
                      ? `包 · ${server.package?.qualifiedId} @ ${server.package?.version}`
                      : server.transport === "streamable-http"
                        ? `远程 · ${server.url}`
                        : `手动 · ${server.command}`}
                  </p>
                  <div className="mcp-card__status">
                    <Badge tone={enabled ? "success" : "neutral"}>{enabled ? "已启用" : "已禁用"}</Badge>
                    {busy ? (
                      <Badge tone="info" role="status">
                        处理中
                      </Badge>
                    ) : snapshot ? (
                      <Badge
                        tone={snapshot.status === "success" ? "success" : "danger"}
                        role={snapshot.status === "error" ? "alert" : "status"}
                        title={
                          snapshot.status === "success"
                            ? `已发现 ${snapshot.toolCount} 个工具`
                            : snapshot.error || "连接失败"
                        }
                      >
                        <span className="mcp-card__status-text">
                          {snapshot.status === "success"
                            ? `${snapshot.toolCount} 个工具`
                            : snapshot.error || "连接失败"}
                        </span>
                      </Badge>
                    ) : server.credentialRequired ? (
                      <Badge tone={server.credentialBound ? "success" : "warning"} role="status">
                        {server.credentialBound ? "凭据已配置" : "需要配置凭据"}
                      </Badge>
                    ) : (
                      <Badge role="status">无需凭据</Badge>
                    )}
                  </div>
                  {server.usageCount ? <p className="mcp-card__usage">被 {server.usageCount} 个 Art 使用</p> : null}
                </div>
                <div className="mcp-card__actions">
                  <div className="mcp-card__action-buttons">
                    <ActionTooltip label={packaged ? "配置凭据" : "编辑"}>
                      <IconButton
                        compact
                        variant="quiet"
                        label={`${packaged ? "配置凭据" : "编辑"} ${server.name}`}
                        disabled={operationBusy}
                        onClick={() => (packaged ? onCredentials(server) : onEdit(server))}
                      >
                        <McpIcon kind="edit" />
                      </IconButton>
                    </ActionTooltip>
                    <ActionTooltip label="测试连接">
                      <IconButton
                        compact
                        variant="quiet"
                        label={`测试 ${server.name}`}
                        disabled={
                          operationBusy || !enabled || Boolean(server.credentialRequired && !server.credentialBound)
                        }
                        onClick={() => onTest(server)}
                      >
                        <McpIcon kind="test" />
                      </IconButton>
                    </ActionTooltip>
                    <ActionTooltip label={enabled ? "禁用" : "启用"}>
                      <IconButton
                        compact
                        variant={enabled ? "positive" : "quiet"}
                        label={`${enabled ? "禁用" : "启用"} ${server.name}`}
                        disabled={operationBusy}
                        onClick={() => onToggle(server)}
                      >
                        <McpIcon kind="power" />
                      </IconButton>
                    </ActionTooltip>
                    <ActionTooltip label="删除">
                      <IconButton
                        compact
                        variant="danger"
                        label={`删除 ${server.name}`}
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
          <EmptyState title="暂无 MCP 服务" />
        )}
      </div>
    </TabPanel>
  );
}
