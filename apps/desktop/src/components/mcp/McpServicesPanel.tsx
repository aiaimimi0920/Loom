// Renders installed MCP services while the hub retains mutation ownership.

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
  return (
    <div
      className="art-hub__surface mcp-hub__surface"
      id="mcp-panel-services"
      role="tabpanel"
      aria-labelledby="mcp-tab-services"
      aria-busy={busyServerId !== null}
      hidden={hidden}
    >
      <div className="art-registry-grid mcp-card-grid">
        {servers.length ? servers.map((server) => {
          const enabled = server.enabled !== false;
          const packaged = server.source === "package" && Boolean(server.package);
          const snapshot = testSnapshots[server.id];
          const busy = busyServerId === server.id;
          return (
            <article
              className={`glass-card art-registry-card mcp-service-card ${enabled ? "art-registry-card--enabled" : "art-registry-card--disabled"}${busy ? " mcp-service-card--busy" : ""}`}
              key={server.id}
              title={enabled ? "已启用" : "已禁用"}
              aria-label={`${server.name}，${enabled ? "已启用" : "已禁用"}`}
              aria-busy={busy}
            >
              <div className="art-registry-card__head">
                <h3>{server.name}</h3>
                <span className="art-registry-card__framework-icon mcp-service-card__icon" title="MCP" aria-label="MCP">
                  <McpIcon kind="plug" />
                  {snapshot ? (
                    <i
                      className={`mcp-service-card__test-dot mcp-service-card__test-dot--${snapshot.status}`}
                      role="img"
                      title={snapshot.status === "success" ? `已发现 ${snapshot.toolCount} 个工具` : "连接失败"}
                      aria-label={snapshot.status === "success" ? `已发现 ${snapshot.toolCount} 个工具` : "连接失败"}
                    />
                  ) : null}
                </span>
              </div>
              <div className="mcp-card__body">
                <p className="art-registry-card__description" title={server.description || "暂无描述"}>{server.description || "暂无描述"}</p>
                <p className="mcp-card__identity" title={packaged ? `${server.package?.qualifiedId} · ${server.package?.version}` : server.transport === "streamable-http" ? `${server.id} · ${server.url}` : `${server.id} · ${server.command} ${(server.args || []).join(" ")}`}>
                  {packaged ? `包 · ${server.package?.qualifiedId} @ ${server.package?.version}` : server.transport === "streamable-http" ? `远程 · ${server.url}` : `手动 · ${server.command}`}
                </p>
                {busy ? (
                  <p className="mcp-card__state mcp-card__state--loading" role="status"><span className="mcp-busy-indicator" aria-hidden="true" />处理中</p>
                ) : snapshot ? (
                  <p
                    className={`mcp-card__state mcp-card__state--${snapshot.status}`}
                    role={snapshot.status === "error" ? "alert" : "status"}
                    title={snapshot.status === "success" ? `已发现 ${snapshot.toolCount} 个工具` : snapshot.error || "连接失败"}
                  >
                    {snapshot.status === "success" ? `${snapshot.toolCount} 个工具` : snapshot.error || "连接失败"}
                  </p>
                ) : server.credentialRequired ? (
                  <p
                    className={`mcp-card__state ${server.credentialBound ? "mcp-card__state--success" : "mcp-card__state--warning"}`}
                    role="status"
                  >
                    {server.credentialBound ? "凭据已配置" : "需要配置凭据"}
                  </p>
                ) : <p className="mcp-card__state" role="status">无需凭据</p>}
                {server.usageCount ? <p className="mcp-card__usage">被 {server.usageCount} 个 Art 使用</p> : null}
              </div>
              <div className="art-registry-card__actions">
                <div className="mcp-card__action-buttons">
                  <button className="art-card-action" type="button" aria-label={`${packaged ? "配置凭据" : "编辑"} ${server.name}`} title={packaged ? "配置凭据" : "编辑"} disabled={operationBusy} onClick={() => packaged ? onCredentials(server) : onEdit(server)}>
                    <McpIcon kind="edit" />
                  </button>
                  <button className="art-card-action" type="button" aria-label={`测试 ${server.name}`} title="测试连接" disabled={operationBusy || !enabled || Boolean(server.credentialRequired && !server.credentialBound)} onClick={() => onTest(server)}>
                    <McpIcon kind="test" />
                  </button>
                  <button className={enabled ? "art-card-action art-card-action--active" : "art-card-action"} type="button" aria-label={`${enabled ? "禁用" : "启用"} ${server.name}`} title={enabled ? "禁用" : "启用"} disabled={operationBusy} onClick={() => onToggle(server)}>
                    <McpIcon kind="power" />
                  </button>
                  <button className="art-card-action art-card-action--danger" type="button" aria-label={`删除 ${server.name}`} title="删除" disabled={operationBusy} onClick={() => onRemove(server)}>
                    <McpIcon kind="trash" />
                  </button>
                </div>
              </div>
            </article>
          );
        }) : <div className="mcp-hub__empty">暂无 MCP 服务</div>}
      </div>
    </div>
  );
}
