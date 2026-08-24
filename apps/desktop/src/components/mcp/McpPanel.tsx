// Adapts the MCP hub to application feedback and refresh contracts.
import { LoomMcpServer } from "../../services/loomApi";
import { pushAppToast, requestAppConfirmation } from "../feedback/AppFeedback";
import { McpHub } from "./McpHub";
import { useCallback } from "react";

export function McpPanel({
  servers,
  baseUrl,
  refresh,
}: {
  servers: LoomMcpServer[];
  baseUrl: string;
  refresh: () => Promise<void>;
}) {
  const notify = useCallback((level: "info" | "warning" | "error", text: string) => {
    pushAppToast({ level, text });
  }, []);
  return (
    <McpHub
      servers={servers}
      baseUrl={baseUrl}
      refresh={refresh}
      notify={notify}
      confirmRemove={(server) => requestAppConfirmation({
        title: "删除 MCP",
        message: server.usageCount
          ? `删除 ${server.name || server.id} 后，${server.usageCount} 个正在使用它的 Art 将返回 MCP 依赖缺失错误。仍要删除吗？`
          : `删除 ${server.name || server.id} 及其独立运行包和专属凭据。此操作不可撤销。`,
        confirmLabel: "删除",
        tone: "danger",
      })}
    />
  );
}
