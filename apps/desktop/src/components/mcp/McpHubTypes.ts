// Defines the private component contracts shared across the MCP workspace.

import type { LoomMcpServer } from "../../services/loomApi";
import type { McpMarketServer } from "../../services/mcpMarketplace";

export type McpWorkspaceId = "services" | "store";
export type NotificationLevel = "info" | "warning" | "error";

export interface McpEditorState {
  mode: "create" | "edit" | "install" | "link";
  server: LoomMcpServer;
  marketItem?: McpMarketServer;
}
