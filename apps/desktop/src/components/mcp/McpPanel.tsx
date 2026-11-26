// Adapts the MCP hub to application feedback and refresh contracts.
import { useCallback } from "react";
import { useUiText } from "../../i18n/index.ts";
import { LoomMcpServer } from "../../services/loomApi";
import { pushAppToast, requestAppConfirmation } from "../feedback/AppFeedback";
import { McpHub } from "./McpHub";

export function McpPanel({
  servers,
  baseUrl,
  refresh,
}: {
  servers: LoomMcpServer[];
  baseUrl: string;
  refresh: () => Promise<void>;
}) {
  const { t, tf } = useUiText();
  const notify = useCallback((level: "info" | "warning" | "error", text: string) => {
    pushAppToast({ level, text });
  }, []);
  return (
    <McpHub
      servers={servers}
      baseUrl={baseUrl}
      refresh={refresh}
      notify={notify}
      confirmRemove={(server) => {
        const name = server.name || server.id;
        return requestAppConfirmation({
          title: t("mcp.removeConfirm.title"),
          message: server.usageCount
            ? tf("mcp.removeConfirm.inUseMessage", { name, count: server.usageCount })
            : tf("mcp.removeConfirm.unusedMessage", { name }),
          confirmLabel: t("mcp.action.delete"),
          tone: "danger",
        });
      }}
    />
  );
}
