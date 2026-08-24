// Composes the MCP workspace and owns all daemon mutations and operation serialization.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";

import { normalizeHttpsExternalUrl } from "../../services/externalUrl";
import {
  deleteMcpServer,
  installMcpServerPackage,
  saveMcpServer,
  setMcpServerEnabled,
  testInstalledMcpServer,
  updateMcpServerCredentials,
  type LoomMcpServer,
} from "../../services/loomApi";
import {
  MCP_MARKET_SERVERS,
  buildMcpPaginationItems,
  buildMarketplaceServerConfig,
  findInstalledMcpServer,
  getMarketplaceHealth,
  type McpMarketCategory,
  type McpMarketServer,
  type McpMarketplaceTestSnapshot,
} from "../../services/mcpMarketplace";
import { McpCredentialDialog } from "./McpCredentialDialog";
import { McpHubToolbar } from "./McpHubToolbar";
import type { McpEditorState, McpWorkspaceId, NotificationLevel } from "./McpHubTypes";
import { assertMcpPackageFileSize, encodeMcpPackageBytes } from "./McpPackageFile";
import { McpServerDialog } from "./McpServerDialog";
import { McpServicesPanel } from "./McpServicesPanel";
import { McpStorePanel } from "./McpStorePanel";

export { McpCredentialDialog } from "./McpCredentialDialog";

interface McpHubProps {
  servers: LoomMcpServer[];
  baseUrl: string;
  refresh: () => Promise<void>;
  notify: (level: NotificationLevel, text: string) => void;
  confirmRemove: (server: LoomMcpServer) => Promise<boolean>;
}

const MCP_STORE_PAGE_SIZE = 24;

export function McpHub({ servers, baseUrl, refresh, notify, confirmRemove }: McpHubProps) {
  const [activeWorkspace, setActiveWorkspace] = useState<McpWorkspaceId>("services");
  const [serviceSearchText, setServiceSearchText] = useState("");
  const [storeSearchText, setStoreSearchText] = useState("");
  const [marketCategory, setMarketCategory] = useState<McpMarketCategory | "All">("All");
  const [marketPage, setMarketPage] = useState(1);
  const [busyServerId, setBusyServerId] = useState<string | null>(null);
  const [busyMarketplaceId, setBusyMarketplaceId] = useState<string | null>(null);
  const [editorBusy, setEditorBusy] = useState(false);
  const [testSnapshots, setTestSnapshots] = useState<Record<string, McpMarketplaceTestSnapshot>>({});
  const [editor, setEditor] = useState<McpEditorState | null>(null);
  const [credentialServer, setCredentialServer] = useState<LoomMcpServer | null>(null);
  const serverOperationRef = useRef(false);
  const editorBusyRef = useRef(false);
  const operationBusy = busyServerId !== null || busyMarketplaceId !== null;
  editorBusyRef.current = editorBusy;
  const closeEditor = useCallback(() => {
    if (!editorBusyRef.current) setEditor(null);
  }, []);
  const closeCredentialDialog = useCallback(() => {
    if (!serverOperationRef.current) setCredentialServer(null);
  }, []);

  const installPackageFile = async (file: File) => {
    if (serverOperationRef.current) return;
    serverOperationRef.current = true;
    setBusyServerId("__package-install__");
    try {
      assertMcpPackageFileSize(file.size);
      const bytes = new Uint8Array(await file.arrayBuffer());
      const installed = await installMcpServerPackage(baseUrl, encodeMcpPackageBytes(bytes));
      notify("info", `${installed.name || installed.id} 已作为独立 MCP 服务安装。`);
      await refresh();
    } catch (error) {
      notify("error", error instanceof Error ? error.message : "无法安装 MCP 服务包。");
    } finally {
      serverOperationRef.current = false;
      setBusyServerId(null);
    }
  };

  const openMarketplaceSource = async (marketItem: McpMarketServer) => {
    try {
      const sourceUrl = normalizeHttpsExternalUrl(marketItem.sourceUrl);
      if (isTauri()) {
        await invoke("open_mcp_source_url", { url: sourceUrl });
      } else {
        window.open(sourceUrl, "_blank", "noopener,noreferrer");
      }
    } catch (error) {
      notify(
        "error",
        error instanceof Error
          ? `无法打开 ${marketItem.name} 的介绍页面：${error.message}`
          : `无法打开 ${marketItem.name} 的介绍页面。`,
      );
    }
  };

  useEffect(() => {
    setMarketPage(1);
  }, [storeSearchText]);

  const recordTestResult = (server: LoomMcpServer, result: Awaited<ReturnType<typeof testInstalledMcpServer>>) => {
    const tools = Array.isArray(result.tools) ? result.tools : [];
    setTestSnapshots((current) => ({
      ...current,
      [server.id]: {
        status: result.success === false ? "error" : "success",
        toolCount: tools.length,
        testedAt: new Date().toISOString(),
        error: result.success === false ? result.error || "MCP 测试失败" : undefined,
      },
    }));
    return tools.length;
  };

  const recordTestError = (server: LoomMcpServer, error: unknown) => {
    const message = error instanceof Error ? error.message : "MCP 测试失败";
    setTestSnapshots((current) => ({
      ...current,
      [server.id]: {
        status: "error",
        toolCount: 0,
        testedAt: new Date().toISOString(),
        error: message,
      },
    }));
    return message;
  };

  const testServer = async (server: LoomMcpServer) => {
    if (serverOperationRef.current) return;
    serverOperationRef.current = true;
    setBusyServerId(server.id);
    try {
      const result = await testInstalledMcpServer(baseUrl, server.id);
      const toolCount = recordTestResult(server, result);
      notify(
        result.success === false ? "error" : "info",
        result.success === false
          ? `${server.name} 连接失败：${result.error || "未知错误"}`
          : `${server.name} 已连接，发现 ${toolCount} 个工具。`,
      );
    } catch (error) {
      const message = recordTestError(server, error);
      notify("error", `${server.name} 连接失败：${message}`);
    } finally {
      serverOperationRef.current = false;
      setBusyServerId(null);
    }
  };

  const persistServer = async (server: LoomMcpServer, testAfterSave: boolean) => {
    if (serverOperationRef.current) return;
    serverOperationRef.current = true;
    setBusyServerId(server.id);
    let saved: LoomMcpServer;
    try {
      saved = await saveMcpServer(baseUrl, server);
    } catch (error) {
      notify("error", error instanceof Error ? error.message : "无法保存 MCP 服务。");
      serverOperationRef.current = false;
      setBusyServerId(null);
      setBusyMarketplaceId(null);
      return;
    }

    setEditor(null);
    try {
      await refresh();
    } catch (error) {
      notify("warning", error instanceof Error ? `${saved.name} 已保存，但列表刷新失败：${error.message}` : `${saved.name} 已保存，但列表刷新失败。`);
    }

    try {
      if (testAfterSave && saved.enabled !== false) {
        try {
          const result = await testInstalledMcpServer(baseUrl, saved.id);
          const toolCount = recordTestResult(saved, result);
          notify(
            result.success === false ? "error" : "info",
            result.success === false
              ? `${saved.name} 已安装，但连接测试失败：${result.error || "未知错误"}`
              : `${saved.name} 已安装，发现 ${toolCount} 个工具。`,
          );
        } catch (error) {
          const message = recordTestError(saved, error);
          notify("error", `${saved.name} 已安装，但连接测试失败：${message}`);
        }
      } else {
        notify("info", `${saved.name} 已保存。`);
      }
    } finally {
      serverOperationRef.current = false;
      setBusyServerId(null);
      setBusyMarketplaceId(null);
    }
  };

  const installMarketplaceServer = async (marketItem: McpMarketServer) => {
    const existing = findInstalledMcpServer(servers, marketItem);
    if (existing) return;
    const server = buildMarketplaceServerConfig(marketItem);
    const health = getMarketplaceHealth(marketItem, server);
    if (marketItem.installOptions.length > 1 || marketItem.requiresManualConfiguration || !health.requiredEnvPresent) {
      setEditor({ mode: "install", server, marketItem });
      return;
    }
    setBusyMarketplaceId(marketItem.id);
    await persistServer(server, true);
  };

  const toggleServer = async (server: LoomMcpServer) => {
    if (serverOperationRef.current) return;
    serverOperationRef.current = true;
    setBusyServerId(server.id);
    try {
      const enabled = server.enabled === false;
      await setMcpServerEnabled(baseUrl, server.id, enabled);
      notify("info", `${server.name} 已${enabled ? "启用" : "禁用"}。`);
      await refresh();
    } catch (error) {
      notify("error", error instanceof Error ? error.message : "无法更新 MCP 服务状态。");
    } finally {
      serverOperationRef.current = false;
      setBusyServerId(null);
    }
  };

  const removeServer = async (server: LoomMcpServer) => {
    if (serverOperationRef.current) return;
    serverOperationRef.current = true;
    setBusyServerId(server.id);
    try {
      if (!await confirmRemove(server)) return;
      await deleteMcpServer(baseUrl, server.id);
      notify("info", `${server.name} 已删除。`);
      await refresh();
    } catch (error) {
      notify("error", error instanceof Error ? error.message : "无法删除 MCP 服务。");
    } finally {
      serverOperationRef.current = false;
      setBusyServerId(null);
    }
  };

  const saveCredentials = async (
    server: LoomMcpServer,
    values: Record<string, string>,
    clear: string[],
  ) => {
    if (serverOperationRef.current) return;
    serverOperationRef.current = true;
    setBusyServerId(server.id);
    try {
      await updateMcpServerCredentials(baseUrl, server.id, values, clear);
      notify("info", `${server.name} 的凭据已更新。`);
      setCredentialServer(null);
      await refresh();
    } catch (error) {
      notify("error", error instanceof Error ? error.message : "无法保存 MCP 凭据。");
    } finally {
      serverOperationRef.current = false;
      setBusyServerId(null);
    }
  };

  const normalizedServiceSearch = serviceSearchText.trim().toLowerCase();
  const normalizedStoreSearch = storeSearchText.trim().toLowerCase();
  const filteredServers = useMemo(() => servers.filter((server) => !normalizedServiceSearch ||
    server.name.toLowerCase().includes(normalizedServiceSearch) ||
    server.id.toLowerCase().includes(normalizedServiceSearch) ||
    (server.serverId || "").toLowerCase().includes(normalizedServiceSearch) ||
    (server.package?.qualifiedId || "").toLowerCase().includes(normalizedServiceSearch) ||
    (server.tools || []).some((tool) => tool.toLowerCase().includes(normalizedServiceSearch)) ||
    (server.description || "").toLowerCase().includes(normalizedServiceSearch)), [normalizedServiceSearch, servers]);
  const filteredMarketServers = useMemo(() => MCP_MARKET_SERVERS.filter((server) => {
    const matchesSearch = !normalizedStoreSearch || server.name.toLowerCase().includes(normalizedStoreSearch) ||
      server.id.toLowerCase().includes(normalizedStoreSearch) || server.description.toLowerCase().includes(normalizedStoreSearch);
    return matchesSearch && (marketCategory === "All" || server.category === marketCategory);
  }), [marketCategory, normalizedStoreSearch]);
  const marketTotalPages = Math.ceil(filteredMarketServers.length / MCP_STORE_PAGE_SIZE);
  const resolvedMarketPage = marketTotalPages > 0 ? Math.min(marketPage, marketTotalPages) : 1;
  const pagedMarketServers = useMemo(() => {
    const offset = (resolvedMarketPage - 1) * MCP_STORE_PAGE_SIZE;
    return filteredMarketServers.slice(offset, offset + MCP_STORE_PAGE_SIZE);
  }, [filteredMarketServers, resolvedMarketPage]);
  const paginationItems = useMemo(
    () => buildMcpPaginationItems(resolvedMarketPage, Math.max(1, marketTotalPages)),
    [marketTotalPages, resolvedMarketPage],
  );

  useEffect(() => {
    if (marketTotalPages > 0 && marketPage > marketTotalPages) setMarketPage(marketTotalPages);
  }, [marketPage, marketTotalPages]);

  return (
    <section className="art-hub mcp-hub" aria-label="MCP">
      <McpHubToolbar
        activeWorkspace={activeWorkspace}
        operationBusy={operationBusy}
        serviceSearchText={serviceSearchText}
        storeSearchText={storeSearchText}
        marketCategory={marketCategory}
        onWorkspaceChange={setActiveWorkspace}
        onServiceSearchChange={setServiceSearchText}
        onStoreSearchChange={setStoreSearchText}
        onMarketCategoryChange={(category) => {
          setMarketCategory(category);
          setMarketPage(1);
        }}
        onInstallPackageFile={installPackageFile}
        onOpenEditor={setEditor}
      />
      <McpServicesPanel
        hidden={activeWorkspace !== "services"}
        servers={filteredServers}
        testSnapshots={testSnapshots}
        busyServerId={busyServerId}
        operationBusy={operationBusy}
        onEdit={(server) => setEditor({ mode: "edit", server })}
        onCredentials={setCredentialServer}
        onTest={(server) => void testServer(server)}
        onToggle={(server) => void toggleServer(server)}
        onRemove={(server) => void removeServer(server)}
      />
      <McpStorePanel
        hidden={activeWorkspace !== "store"}
        marketServers={pagedMarketServers}
        installedServers={servers}
        testSnapshots={testSnapshots}
        busyServerId={busyServerId}
        busyMarketplaceId={busyMarketplaceId}
        operationBusy={operationBusy}
        filteredCount={filteredMarketServers.length}
        totalPages={marketTotalPages}
        currentPage={resolvedMarketPage}
        paginationItems={paginationItems}
        onPageChange={setMarketPage}
        onOpenSource={(marketItem) => void openMarketplaceSource(marketItem)}
        onConfigure={(marketItem, server) => setEditor({ mode: "install", server, marketItem })}
        onInstall={(marketItem) => void installMarketplaceServer(marketItem)}
        onTest={(server) => void testServer(server)}
      />
      <McpServerDialog
        editor={editor}
        busy={editorBusy}
        onClose={closeEditor}
        onSave={async (server) => {
          setEditorBusy(true);
          try {
            await persistServer(server, editor?.mode === "install" || editor?.mode === "link");
          } finally {
            setEditorBusy(false);
          }
        }}
      />
      <McpCredentialDialog
        server={credentialServer}
        busy={credentialServer ? busyServerId === credentialServer.id : false}
        onClose={closeCredentialDialog}
        onSave={async (values, clear) => {
          if (credentialServer) await saveCredentials(credentialServer, values, clear);
        }}
      />
    </section>
  );
}
