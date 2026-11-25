// Owns MCP discovery state, schema import, and stale-request cleanup.
import { useEffect, useRef, useState, type Dispatch, type SetStateAction } from "react";
import { testMcpConnection, type LoomMcpServer } from "../../services/loomApi";
import { portsFromMcpToolSchema } from "../../services/workflowStudio";
import type { StudioMessage } from "../app/appShell";
import { portDraftFromParsedPort, type ArtWizardPortDraft } from "./artWizardModel";
export function useArtMcpDiscovery({
  baseUrl,
  mcpServers,
  mcpServerId,
  mcpToolName,
  setMcpToolName,
  setInputPorts,
  setOutputPorts,
  setWizardMessage,
}: {
  baseUrl: string;
  mcpServers: LoomMcpServer[];
  mcpServerId: string;
  mcpToolName: string;
  setMcpToolName: Dispatch<SetStateAction<string>>;
  setInputPorts: Dispatch<SetStateAction<ArtWizardPortDraft[]>>;
  setOutputPorts: Dispatch<SetStateAction<ArtWizardPortDraft[]>>;
  setWizardMessage: Dispatch<SetStateAction<StudioMessage | null>>;
}) {
  const [mcpTools, setMcpTools] = useState<unknown[]>([]);

  const [selectedMcpSchemaToolName, setSelectedMcpSchemaToolName] = useState("");

  const [mcpDiscoveryBusy, setMcpDiscoveryBusy] = useState(false);

  const mcpDiscoveryVersionRef = useRef(0);

  const mcpToolLabel = (tool: unknown) => {
    if (tool && typeof tool === "object" && !Array.isArray(tool)) {
      const record = tool as Record<string, unknown>;
      if (typeof record.name === "string" && record.name.trim()) return record.name.trim();
    }
    return "mcp_tool";
  };

  const discoverMcpTools = async () => {
    const server = mcpServers.find((item) => item.id === mcpServerId);
    if (!server) {
      setWizardMessage({ kind: "error", text: "请选择 MCP 服务。" });
      return;
    }
    const requestVersion = ++mcpDiscoveryVersionRef.current;
    setMcpDiscoveryBusy(true);
    try {
      const result = await testMcpConnection(baseUrl, server);
      if (requestVersion !== mcpDiscoveryVersionRef.current) return;
      const tools = Array.isArray(result.tools) ? result.tools : [];
      setMcpTools(tools);
      const firstToolName = tools[0] ? mcpToolLabel(tools[0]) : "";
      setSelectedMcpSchemaToolName(firstToolName);
      if (firstToolName && !mcpToolName.trim()) setMcpToolName(firstToolName);
      setWizardMessage({
        kind: result.success === false ? "error" : "info",
        text: result.success === false ? result.error || "MCP 连接失败。" : `发现 ${tools.length} 个工具。`,
      });
    } catch (error) {
      if (requestVersion === mcpDiscoveryVersionRef.current) {
        setWizardMessage({ kind: "error", text: error instanceof Error ? error.message : "无法发现 MCP 工具。" });
      }
    } finally {
      if (requestVersion === mcpDiscoveryVersionRef.current) setMcpDiscoveryBusy(false);
    }
  };

  const useSelectedMcpToolSchema = () => {
    const tool = mcpTools.find((item) => mcpToolLabel(item) === selectedMcpSchemaToolName);
    const parsed = portsFromMcpToolSchema(tool);
    if (!parsed) {
      setWizardMessage({ kind: "error", text: "该工具没有可用的 input_schema。" });
      return;
    }
    setMcpToolName(parsed.toolName);
    if (parsed.suggestedInputs.length)
      setInputPorts(parsed.suggestedInputs.map((port) => portDraftFromParsedPort(port, "input")));
    if (parsed.suggestedOutputs.length)
      setOutputPorts(parsed.suggestedOutputs.map((port) => portDraftFromParsedPort(port, "output")));
    setWizardMessage({ kind: "info", text: "已导入 MCP 工具结构。" });
  };
  useEffect(
    () => () => {
      mcpDiscoveryVersionRef.current += 1;
    },
    [],
  );
  return {
    mcpTools,
    selectedMcpSchemaToolName,
    setSelectedMcpSchemaToolName,
    mcpDiscoveryBusy,
    mcpToolLabel,
    discoverMcpTools,
    useSelectedMcpToolSchema,
  };
}
