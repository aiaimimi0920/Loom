// Composes registry, marketplace, and plugin-security Art workspaces.
import {
  type ArtWorkspaceId,
  artWorkspaceItems,
  frameworkFilterLabel,
  frameworkIdentity,
  nextArtWorkspaceIndex,
} from "../../services/artHubUi";
import {
  getLoomSettings,
  installFramework,
  listFrameworks,
  type LoomFramework,
  LoomMcpServer,
  LoomToolDefinition,
  LoomWorkflowMetadata,
  uninstallFramework,
  upgradeFrameworkPackage,
} from "../../services/loomApi";
import { StudioMessage } from "../app/appShell";
import { PluginSecurityPanel } from "../security/PluginSecurityPanel";
import {
  ArtPublishDialog,
  ArtStoreCard,
  FrameworkFilter,
} from "./ArtMarketplace";
import {
  type FrameworkBusyAction,
  FrameworkManagementDialog,
  readFrameworkPackageBase64,
} from "./FrameworkManagementDialog";
import { ArtCreationRequest } from "./artWizardModel";
import { RegistryPanel } from "./RegistryPanel";
import {
  type KeyboardEvent,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

export function ArtPanel({
  tools,
  mcpServers,
  workflows,
  baseUrl,
  refresh,
  pendingCreationRequest,
  onCreationRequestHandled,
}: {
  tools: LoomToolDefinition[];
  mcpServers: LoomMcpServer[];
  workflows: LoomWorkflowMetadata[];
  baseUrl: string;
  refresh: () => Promise<void>;
  pendingCreationRequest: ArtCreationRequest | null;
  onCreationRequestHandled: () => void;
}) {
  const [activeWorkspace, setActiveWorkspace] = useState<ArtWorkspaceId>("registry");
  const [frameworks, setFrameworks] = useState<LoomFramework[]>([]);
  const [frameworkBusyId, setFrameworkBusyId] = useState<string | null>(null);
  const [frameworkBusyAction, setFrameworkBusyAction] = useState<FrameworkBusyAction>(null);
  const [frameworkError, setFrameworkError] = useState<string | null>(null);
  const [frameworkManagementMessage, setFrameworkManagementMessage] = useState<StudioMessage | null>(null);
  const [frameworkDialogOpen, setFrameworkDialogOpen] = useState(false);
  const [createDialogOpen, setCreateDialogOpen] = useState(false);
  const [createRequest, setCreateRequest] = useState<ArtCreationRequest | null>(null);
  const [publishDialogOpen, setPublishDialogOpen] = useState(false);
  const [storeSearchText, setStoreSearchText] = useState("");
  const [storeOfficialOnly, setStoreOfficialOnly] = useState(false);
  const [storeCatalogRefreshToken, setStoreCatalogRefreshToken] = useState(0);
  const [snapshotRefreshError, setSnapshotRefreshError] = useState<string | null>(null);
  const [selectedFrameworkIds, setSelectedFrameworkIds] = useState<Set<string> | null>(null);
  const frameworkLoadVersion = useRef(0);
  const createArtButtonRef = useRef<HTMLButtonElement | null>(null);
  const publishArtButtonRef = useRef<HTMLButtonElement | null>(null);
  const frameworkManageButtonRef = useRef<HTMLButtonElement | null>(null);
  const tabRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const frameworkIds = useMemo(
    () => [...new Set(frameworks.map((framework) => frameworkIdentity(framework)))],
    [frameworks],
  );

  const toggleFrameworkFilter = (frameworkId: string) => {
    setSelectedFrameworkIds((current) => {
      const next = current === null ? new Set(frameworkIds) : new Set(current);
      if (next.has(frameworkId)) {
        next.delete(frameworkId);
      } else {
        next.add(frameworkId);
      }
      return next.size === frameworkIds.length ? null : next;
    });
  };

  const loadFrameworks = useCallback(async () => {
    const version = ++frameworkLoadVersion.current;
    try {
      const list = await listFrameworks(baseUrl);
      if (version !== frameworkLoadVersion.current) return;
      setFrameworks(list);
      setFrameworkError(null);
    } catch (error) {
      if (version === frameworkLoadVersion.current) {
        setFrameworkError(error instanceof Error ? error.message : "无法读取框架列表。");
      }
    }
  }, [baseUrl]);

  useEffect(() => {
    setSnapshotRefreshError(null);
    void loadFrameworks();
    return () => {
      frameworkLoadVersion.current += 1;
    };
  }, [loadFrameworks]);

  useEffect(() => {
    let cancelled = false;
    void getLoomSettings(baseUrl)
      .then((settings) => {
        if (!cancelled) setStoreOfficialOnly(settings.art_store?.official_only === true);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [baseUrl]);

  useEffect(() => {
    if (!pendingCreationRequest) return;
    setActiveWorkspace("registry");
    setFrameworkDialogOpen(false);
    setPublishDialogOpen(false);
    setCreateRequest(pendingCreationRequest);
    setCreateDialogOpen(true);
    onCreationRequestHandled();
  }, [onCreationRequestHandled, pendingCreationRequest]);

  const synchronizeArtState = useCallback(async () => {
    const [, snapshotResult] = await Promise.allSettled([loadFrameworks(), refresh()]);
    if (snapshotResult.status === "rejected") {
      const detail = snapshotResult.reason instanceof Error
        ? snapshotResult.reason.message
        : "无法刷新 Loom 主快照。";
      setSnapshotRefreshError(`Art 操作已完成，但主快照刷新失败：${detail}`);
      return;
    }
    setSnapshotRefreshError(null);
  }, [loadFrameworks, refresh]);

  const toggleFramework = async (framework: LoomFramework) => {
    const identity = frameworkIdentity(framework);
    const action = framework.installed ? "卸载" : "安装";
    setFrameworkBusyId(identity);
    setFrameworkBusyAction("toggle");
    setFrameworkError(null);
    setFrameworkManagementMessage(null);
    try {
      if (framework.installed) {
        await uninstallFramework(baseUrl, identity);
      } else {
        await installFramework(baseUrl, identity);
      }
      await synchronizeArtState();
      setFrameworkManagementMessage({ kind: "info", text: `已${action} ${frameworkFilterLabel(framework)}。` });
    } catch (error) {
      const detail = error instanceof Error ? error.message : "框架操作失败。";
      setFrameworkError(detail);
    } finally {
      setFrameworkBusyId(null);
      setFrameworkBusyAction(null);
    }
  };

  const upgradeFramework = async (framework: LoomFramework, file: File) => {
    if (!framework.installed) return;
    const identity = frameworkIdentity(framework);
    setFrameworkBusyId(identity);
    setFrameworkBusyAction("upgrade");
    setFrameworkError(null);
    setFrameworkManagementMessage(null);
    try {
      const zipBase64 = await readFrameworkPackageBase64(file);
      await upgradeFrameworkPackage(baseUrl, identity, zipBase64);
      await synchronizeArtState();
      setFrameworkManagementMessage({
        kind: "info",
        text: `已更新 ${frameworkFilterLabel(framework)}。`,
      });
    } catch (error) {
      const detail = error instanceof Error ? error.message : "框架更新失败。";
      setFrameworkError(detail);
    } finally {
      setFrameworkBusyId(null);
      setFrameworkBusyAction(null);
    }
  };

  const closeFrameworkDialog = useCallback(() => {
    setFrameworkDialogOpen(false);
    window.setTimeout(() => frameworkManageButtonRef.current?.focus(), 0);
  }, []);

  const closeCreateDialog = useCallback(() => {
    setCreateDialogOpen(false);
    setCreateRequest(null);
    window.setTimeout(() => createArtButtonRef.current?.focus(), 0);
  }, []);

  const closePublishDialog = useCallback(() => {
    setPublishDialogOpen(false);
    window.setTimeout(() => publishArtButtonRef.current?.focus(), 0);
  }, []);

  const selectAdjacentWorkspace = (event: KeyboardEvent<HTMLButtonElement>, index: number) => {
    const nextIndex = nextArtWorkspaceIndex(event.key, index, artWorkspaceItems.length);
    if (nextIndex === null) return;
    event.preventDefault();
    const next = artWorkspaceItems[nextIndex];
    setActiveWorkspace(next.id);
    tabRefs.current[nextIndex]?.focus();
  };

  return (
    <section className="art-hub" aria-label="Art">
      <div className="art-hub__navigation">
        <div
          className={activeWorkspace === "registry" || activeWorkspace === "store" ? "art-hub__tabs art-hub__tabs--with-filter" : "art-hub__tabs"}
          role="tablist"
          aria-label="Art 工作区"
        >
          {artWorkspaceItems.map((item, index) => {
            const active = activeWorkspace === item.id;
            return (
              <button
                key={item.id}
                ref={(element) => {
                  tabRefs.current[index] = element;
                }}
                id={`art-tab-${item.id}`}
                className={active ? "art-hub__tab art-hub__tab--active" : "art-hub__tab"}
                type="button"
                role="tab"
                aria-selected={active}
                aria-controls={`art-panel-${item.id}`}
                tabIndex={active ? 0 : -1}
                onClick={() => setActiveWorkspace(item.id)}
                onKeyDown={(event) => selectAdjacentWorkspace(event, index)}
              >
                <span>{item.label}</span>
              </button>
            );
          })}
        </div>
        {activeWorkspace === "registry" ? (
          <FrameworkFilter
            frameworks={frameworks}
            selectedFrameworkIds={selectedFrameworkIds}
            onToggle={toggleFrameworkFilter}
            actions={(
              <>
                <button
                  className="ghost-button framework-filter__create"
                  type="button"
                  ref={createArtButtonRef}
                  onClick={() => {
                    setFrameworkDialogOpen(false);
                    setPublishDialogOpen(false);
                    setCreateRequest(null);
                    setCreateDialogOpen(true);
                  }}
                >
                  创建 Art
                </button>
                <button
                  className="ghost-button framework-filter__manage"
                  type="button"
                  ref={frameworkManageButtonRef}
                  onClick={() => {
                    setCreateDialogOpen(false);
                    setPublishDialogOpen(false);
                    setFrameworkManagementMessage(null);
                    setFrameworkDialogOpen(true);
                  }}
                >
                  管理框架
                </button>
              </>
            )}
          />
        ) : activeWorkspace === "store" ? (
          <FrameworkFilter
            frameworks={frameworks}
            selectedFrameworkIds={selectedFrameworkIds}
            onToggle={toggleFrameworkFilter}
            actions={(
              <>
                <button
                  className="ghost-button framework-filter__publish"
                  type="button"
                  ref={publishArtButtonRef}
                  onClick={() => {
                    setCreateDialogOpen(false);
                    setFrameworkDialogOpen(false);
                    setPublishDialogOpen(true);
                  }}
                >
                  发布 Art
                </button>
                <input
                  className="framework-filter__search"
                  type="search"
                  aria-label="搜索 Art"
                  placeholder="搜索 Art"
                  value={storeSearchText}
                  onChange={(event) => setStoreSearchText(event.target.value)}
                />
                <label className={storeOfficialOnly ? "framework-filter__official framework-filter__official--checked" : "framework-filter__official"}>
                  <input
                    type="checkbox"
                    aria-label="只显示官方"
                    checked={storeOfficialOnly}
                    onChange={(event) => setStoreOfficialOnly(event.target.checked)}
                  />
                  <span title="只显示官方">官</span>
                </label>
              </>
            )}
          />
        ) : null}
      </div>

      {frameworkError ? (
        <div className="art-hub__notice" role="alert">
          <span>{frameworkError}</span>
          <button className="ghost-button" type="button" onClick={() => void loadFrameworks()}>
            重试框架状态
          </button>
        </div>
      ) : null}

      {snapshotRefreshError ? (
        <div className="art-hub__notice art-hub__notice--warning" role="alert">
          <span>{snapshotRefreshError}</span>
          <button className="ghost-button" type="button" onClick={() => void synchronizeArtState()}>
            重新同步 Art 状态
          </button>
        </div>
      ) : null}

      <div
        className="art-hub__surface"
        id="art-panel-registry"
        role="tabpanel"
        aria-labelledby="art-tab-registry"
        hidden={activeWorkspace !== "registry"}
      >
        <RegistryPanel
          tools={tools}
          mcpServers={mcpServers}
          workflows={workflows}
          frameworks={frameworks}
          selectedFrameworkIds={selectedFrameworkIds}
          createDialogOpen={createDialogOpen}
          createRequest={createRequest}
          onCloseCreateDialog={closeCreateDialog}
          reloadFrameworks={loadFrameworks}
          baseUrl={baseUrl}
          refresh={refresh}
        />
      </div>
      <div
        className="art-hub__surface"
        id="art-panel-store"
        role="tabpanel"
        aria-labelledby="art-tab-store"
        hidden={activeWorkspace !== "store"}
      >
        <ArtStoreCard
          baseUrl={baseUrl}
          active={activeWorkspace === "store"}
          frameworks={frameworks}
          selectedFrameworkIds={selectedFrameworkIds}
          searchText={storeSearchText}
          officialOnly={storeOfficialOnly}
          refreshToken={storeCatalogRefreshToken}
          onInstalled={synchronizeArtState}
        />
      </div>
      <div
        className="art-hub__surface"
        id="art-panel-security"
        role="tabpanel"
        aria-labelledby="art-tab-security"
        hidden={activeWorkspace !== "security"}
      >
        <PluginSecurityPanel baseUrl={baseUrl} />
      </div>
      <FrameworkManagementDialog
        open={frameworkDialogOpen}
        frameworks={frameworks}
        busyId={frameworkBusyId}
        busyAction={frameworkBusyAction}
        error={frameworkError}
        message={frameworkManagementMessage}
        onClose={closeFrameworkDialog}
        onToggle={toggleFramework}
        onUpgrade={upgradeFramework}
      />
      <ArtPublishDialog
        open={publishDialogOpen}
        tools={tools}
        baseUrl={baseUrl}
        onClose={closePublishDialog}
        onPublished={async () => {
          await synchronizeArtState();
          setStoreCatalogRefreshToken((current) => current + 1);
        }}
      />
    </section>
  );
}
