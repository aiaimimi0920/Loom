// Owns MCP workspace navigation, search/filter controls and package selection.

import { useRef } from "react";
import { Button, Input, Select, Tab, TabList } from "@neuro/ui";

import type { LoomMcpServer } from "../../services/loomApi";
import {
  MCP_MARKET_CATEGORIES,
  mcpMarketCategoryLabel,
  type McpMarketCategory,
} from "../../services/mcpMarketplace";
import type { McpEditorState, McpWorkspaceId } from "./McpHubTypes";

const workspaceItems: Array<{ id: McpWorkspaceId; label: string }> = [
  { id: "services", label: "服务" },
  { id: "store", label: "商店" },
];

const createRemoteMcpDraft = (): LoomMcpServer => ({
  id: `remote-mcp-${Date.now().toString(36)}`,
  name: "远程 MCP",
  description: "",
  transport: "streamable-http",
  command: "",
  args: [],
  env: {},
  url: "",
  headers: {},
  enabled: true,
});

const createLocalMcpDraft = (): LoomMcpServer => ({
  id: "local-mcp-server",
  name: "本地 MCP 服务",
  description: "",
  transport: "stdio",
  command: "npx",
  args: ["-y", "@modelcontextprotocol/server-memory"],
  env: {},
  url: "",
  headers: {},
  enabled: true,
});

interface McpHubToolbarProps {
  activeWorkspace: McpWorkspaceId;
  operationBusy: boolean;
  serviceSearchText: string;
  storeSearchText: string;
  marketCategory: McpMarketCategory | "All";
  onServiceSearchChange: (value: string) => void;
  onStoreSearchChange: (value: string) => void;
  onMarketCategoryChange: (category: McpMarketCategory | "All") => void;
  onInstallPackageFile: (file: File) => Promise<void>;
  onOpenEditor: (editor: McpEditorState) => void;
}

export function McpHubToolbar({
  activeWorkspace,
  operationBusy,
  serviceSearchText,
  storeSearchText,
  marketCategory,
  onServiceSearchChange,
  onStoreSearchChange,
  onMarketCategoryChange,
  onInstallPackageFile,
  onOpenEditor,
}: McpHubToolbarProps) {
  const packageFileInputRef = useRef<HTMLInputElement | null>(null);


  return (
    <div className="art-hub__navigation">
      <TabList label="MCP 工作区">
        {workspaceItems.map(item => <Tab key={item.id} value={item.id}>{item.label}</Tab>)}
      </TabList>

      <div className="framework-filter mcp-hub__toolbar">
        <div className="mcp-hub__toolbar-primary">
          {activeWorkspace === "services" ? (
            <>
              <input
                ref={packageFileInputRef}
                hidden
                type="file"
                accept=".zip,application/zip"
                onChange={(event) => {
                  const file = event.currentTarget.files?.[0];
                  event.currentTarget.value = "";
                  if (file) void onInstallPackageFile(file);
                }}
              />
              <Button variant="primary" disabled={operationBusy} onClick={() => packageFileInputRef.current?.click()}>安装 MCP 包</Button>
              <Button variant="quiet" disabled={operationBusy} onClick={() => onOpenEditor({
                mode: "create",
                server: createLocalMcpDraft(),
              })}>添加手动配置</Button>
            </>
          ) : (
            <Button
              variant="quiet"
              type="button"
              disabled={operationBusy}
              onClick={() => onOpenEditor({ mode: "link", server: createRemoteMcpDraft() })}
            >
              链接添加
            </Button>
          )}
        </div>
        <div className="framework-filter__actions mcp-hub__toolbar-actions">
          <Input
            className="mcp-hub__search"
            type="search"
            aria-label={activeWorkspace === "services" ? "搜索 MCP 服务" : "搜索 MCP 商店"}
            placeholder={activeWorkspace === "services" ? "搜索服务" : "搜索商店"}
            value={activeWorkspace === "services" ? serviceSearchText : storeSearchText}
            onChange={(event) => activeWorkspace === "services"
              ? onServiceSearchChange(event.target.value)
              : onStoreSearchChange(event.target.value)}
          />
          {activeWorkspace === "store" ? (
            <Select
              className="mcp-hub__category"
              aria-label="MCP 商店分类"
              value={marketCategory}
              onChange={(event) => onMarketCategoryChange(event.target.value as McpMarketCategory | "All")}
            >
              <option value="All">全部</option>
              {MCP_MARKET_CATEGORIES.map((category) => (
                <option key={category} value={category}>{mcpMarketCategoryLabel(category)}</option>
              ))}
            </Select>
          ) : null}
        </div>
      </div>
    </div>
  );
}
