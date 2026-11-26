// Composes the MCP workspace and owns all daemon mutations and operation serialization.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { TabsRoot } from "@neuro/ui";

import { useUiText } from "../../i18n/index.ts";
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
  const { t, tf, language } = useUiText();
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
      notify("info", tf("mcp.toast.packageInstalled", { name: installed.name || installed.id }));
      await refresh();
    } catch (error) {
      notify("error", error instanceof Error ? error.message : t("mcp.toast.packageInstallFailed"));
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
          ? tf("mcp.toast.openSourceFailedWithReason", { name: marketItem.name, reason: error.message })
          : tf("mcp.toast.openSourceFailed", { name: marketItem.name }),
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
        error: result.success === false ? result.error || t("mcp.test.failed") : undefined,
      },
    }));
    return tools.length;
  };

  const recordTestError = (server: LoomMcpServer, error: unknown) => {
    const message = error instanceof Error ? error.message : t("mcp.test.failed");
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
          ? tf("mcp.toast.connectionFailed", {
              name: server.name,
              reason: result.error || t("mcp.error.unknown"),
            })
          : tf("mcp.toast.connectedWithTools", { name: server.name, count: toolCount }),
      );
    } catch (error) {
      const message = recordTestError(server, error);
      notify("error", tf("mcp.toast.connectionFailed", { name: server.name, reason: message }));
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
      notify("error", error instanceof Error ? error.message : t("mcp.toast.saveFailed"));
      serverOperationRef.current = false;
      setBusyServerId(null);
      setBusyMarketplaceId(null);
      return;
    }

    try {
      await refresh();
    } catch (error) {
      notify(
        "warning",
        error instanceof Error
          ? tf("mcp.toast.savedRefreshFailedWithReason", { name: saved.name, reason: error.message })
          : tf("mcp.toast.savedRefreshFailed", { name: saved.name }),
      );
    }

    try {
      if (testAfterSave && saved.enabled !== false) {
        try {
          const result = await testInstalledMcpServer(baseUrl, saved.id);
          const toolCount = recordTestResult(saved, result);
          notify(
            result.success === false ? "error" : "info",
            result.success === false
              ? tf("mcp.toast.installedTestFailed", {
                  name: saved.name,
                  reason: result.error || t("mcp.error.unknown"),
                })
              : tf("mcp.toast.installedWithTools", { name: saved.name, count: toolCount }),
          );
        } catch (error) {
          const message = recordTestError(saved, error);
          notify("error", tf("mcp.toast.installedTestFailed", { name: saved.name, reason: message }));
        }
      } else {
        notify("info", tf("mcp.toast.saved", { name: saved.name }));
      }
    } finally {
      serverOperationRef.current = false;
      setBusyServerId(null);
      setBusyMarketplaceId(null);
      // Return focus only after the opening action is enabled again.
      setEditor(null);
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
      notify("info", tf(enabled ? "mcp.toast.enabled" : "mcp.toast.disabled", { name: server.name }));
      await refresh();
    } catch (error) {
      notify("error", error instanceof Error ? error.message : t("mcp.toast.statusUpdateFailed"));
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
      if (!(await confirmRemove(server))) return;
      await deleteMcpServer(baseUrl, server.id);
      notify("info", tf("mcp.toast.deleted", { name: server.name }));
      await refresh();
    } catch (error) {
      notify("error", error instanceof Error ? error.message : t("mcp.toast.deleteFailed"));
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
    let updated = false;
    try {
      await updateMcpServerCredentials(baseUrl, server.id, values, clear);
      updated = true;
      notify("info", tf("mcp.toast.credentialsUpdated", { name: server.name }));
      await refresh();
    } catch (error) {
      notify("error", error instanceof Error ? error.message : t("mcp.toast.credentialsSaveFailed"));
    } finally {
      serverOperationRef.current = false;
      setBusyServerId(null);
      // A successful mutation still closes if its follow-up refresh fails.
      if (updated) setCredentialServer(null);
    }
  };

  const normalizedServiceSearch = serviceSearchText.trim().toLowerCase();
  const normalizedStoreSearch = storeSearchText.trim().toLowerCase();
  const filteredServers = useMemo(
    () =>
      servers.filter(
        (server) =>
          !normalizedServiceSearch ||
          server.name.toLowerCase().includes(normalizedServiceSearch) ||
          server.id.toLowerCase().includes(normalizedServiceSearch) ||
          (server.serverId || "").toLowerCase().includes(normalizedServiceSearch) ||
          (server.package?.qualifiedId || "").toLowerCase().includes(normalizedServiceSearch) ||
          (server.tools || []).some((tool) => tool.toLowerCase().includes(normalizedServiceSearch)) ||
          (server.description || "").toLowerCase().includes(normalizedServiceSearch),
      ),
    [normalizedServiceSearch, servers],
  );
  const filteredMarketServers = useMemo(
    () =>
      MCP_MARKET_SERVERS.filter((server) => {
        const matchesSearch =
          !normalizedStoreSearch ||
          server.name.toLowerCase().includes(normalizedStoreSearch) ||
          server.id.toLowerCase().includes(normalizedStoreSearch) ||
          server.description.toLowerCase().includes(normalizedStoreSearch);
        return matchesSearch && (marketCategory === "All" || server.category === marketCategory);
      }),
    [language, marketCategory, normalizedStoreSearch],
  );
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
    <TabsRoot asChild value={activeWorkspace} onValueChange={(value) => setActiveWorkspace(value as McpWorkspaceId)}>
      <section className="art-hub mcp-hub" aria-label={t("mcp.hub.ariaLabel")}>
        <McpHubToolbar
          activeWorkspace={activeWorkspace}
          operationBusy={operationBusy}
          serviceSearchText={serviceSearchText}
          storeSearchText={storeSearchText}
          marketCategory={marketCategory}
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
    </TabsRoot>
  );
}
