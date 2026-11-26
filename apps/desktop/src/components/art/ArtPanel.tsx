// Composes registry, marketplace, and plugin-security Art workspaces.
import { Button, Checkbox, Input, Notice, Tab, TabList, TabPanel, TabsRoot } from "@neuro/ui";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { uiFormat, uiText, useUiText } from "../../i18n/index.ts";
import {
  type ArtWorkspaceId,
  artWorkspaceItems,
  frameworkFilterLabel,
  frameworkIdentity,
  toggleArtFrameworkFilter,
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
import { ArtStoreCard } from "./ArtMarketplace";
import { ArtPublishDialog } from "./ArtPublishDialog";
import { FrameworkFilter } from "./FrameworkFilter";
import {
  type FrameworkBusyAction,
  FrameworkManagementDialog,
  readFrameworkPackageBase64,
} from "./FrameworkManagementDialog";
import { ArtCreationRequest } from "./artWizardModel";
import { RegistryPanel } from "./RegistryPanel";

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
  const { t, tf } = useUiText();
  const [activeWorkspace, setActiveWorkspace] = useState<ArtWorkspaceId>("registry");
  const [frameworks, setFrameworks] = useState<LoomFramework[]>([]);
  const [frameworksLoading, setFrameworksLoading] = useState(true);
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
  const frameworkIds = useMemo(
    () => [...new Set(frameworks.map((framework) => frameworkIdentity(framework)))],
    [frameworks],
  );

  const toggleFrameworkFilter = (frameworkId: string) => {
    setSelectedFrameworkIds((current) => toggleArtFrameworkFilter(current, frameworkIds, frameworkId));
  };

  const loadFrameworks = useCallback(async () => {
    const version = ++frameworkLoadVersion.current;
    setFrameworksLoading(true);
    try {
      const list = await listFrameworks(baseUrl);
      if (version !== frameworkLoadVersion.current) return;
      setFrameworks(list);
      setFrameworkError(null);
    } catch (error) {
      if (version === frameworkLoadVersion.current) {
        setFrameworkError(error instanceof Error ? error.message : uiText("art.framework.loadFailed"));
      }
    } finally {
      if (version === frameworkLoadVersion.current) setFrameworksLoading(false);
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
      const detail =
        snapshotResult.reason instanceof Error
          ? snapshotResult.reason.message
          : uiText("art.snapshot.refreshFailedDetail");
      setSnapshotRefreshError(uiFormat("art.snapshot.refreshFailedMessage", { detail }));
      return;
    }
    setSnapshotRefreshError(null);
  }, [loadFrameworks, refresh]);

  const toggleFramework = async (framework: LoomFramework) => {
    const identity = frameworkIdentity(framework);
    const wasInstalled = framework.installed;
    setFrameworkBusyId(identity);
    setFrameworkBusyAction("toggle");
    setFrameworkError(null);
    setFrameworkManagementMessage(null);
    try {
      if (wasInstalled) {
        await uninstallFramework(baseUrl, identity);
      } else {
        await installFramework(baseUrl, identity);
      }
      await synchronizeArtState();
      setFrameworkManagementMessage({
        kind: "info",
        text: tf(wasInstalled ? "art.framework.uninstalledMessage" : "art.framework.installedMessage", {
          framework: frameworkFilterLabel(framework),
        }),
      });
    } catch (error) {
      const detail = error instanceof Error ? error.message : t("art.framework.actionFailed");
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
        text: tf("art.framework.upgradedMessage", { framework: frameworkFilterLabel(framework) }),
      });
    } catch (error) {
      const detail = error instanceof Error ? error.message : t("art.framework.upgradeFailed");
      setFrameworkError(detail);
    } finally {
      setFrameworkBusyId(null);
      setFrameworkBusyAction(null);
    }
  };

  const closeFrameworkDialog = useCallback(() => {
    setFrameworkDialogOpen(false);
  }, []);

  const closeCreateDialog = useCallback(() => {
    setCreateDialogOpen(false);
    setCreateRequest(null);
  }, []);

  const closePublishDialog = useCallback(() => {
    setPublishDialogOpen(false);
  }, []);

  return (
    <TabsRoot
      className="art-hub"
      aria-label={t("art.hub.ariaLabel")}
      value={activeWorkspace}
      onValueChange={(value) => setActiveWorkspace(value as ArtWorkspaceId)}
    >
      <div className="art-hub__navigation">
        <TabList className="loom-art-tabs" label={t("art.workspace.ariaLabel")}>
          {artWorkspaceItems.map((item) => (
            <Tab key={item.id} value={item.id} id={`art-tab-${item.id}`} aria-controls={`art-panel-${item.id}`}>
              {item.label}
            </Tab>
          ))}
        </TabList>
        {activeWorkspace === "registry" ? (
          <FrameworkFilter
            frameworks={frameworks}
            selectedFrameworkIds={selectedFrameworkIds}
            onToggle={toggleFrameworkFilter}
            actions={
              <>
                <Button
                  className="framework-filter__create"
                  type="button"
                  onClick={() => {
                    setFrameworkDialogOpen(false);
                    setPublishDialogOpen(false);
                    setCreateRequest(null);
                    setCreateDialogOpen(true);
                  }}
                >
                  {t("art.action.createArt")}
                </Button>
                <Button
                  className="framework-filter__manage"
                  type="button"
                  onClick={() => {
                    setCreateDialogOpen(false);
                    setPublishDialogOpen(false);
                    setFrameworkManagementMessage(null);
                    setFrameworkDialogOpen(true);
                  }}
                >
                  {t("art.action.manageFrameworks")}
                </Button>
              </>
            }
          />
        ) : activeWorkspace === "store" ? (
          <FrameworkFilter
            frameworks={frameworks}
            selectedFrameworkIds={selectedFrameworkIds}
            onToggle={toggleFrameworkFilter}
            actions={
              <>
                <Button
                  className="framework-filter__publish"
                  type="button"
                  onClick={() => {
                    setCreateDialogOpen(false);
                    setFrameworkDialogOpen(false);
                    setPublishDialogOpen(true);
                  }}
                >
                  {t("art.action.publishArt")}
                </Button>
                <Input
                  className="loom-art-store-search"
                  type="search"
                  aria-label={t("art.store.searchAria")}
                  placeholder={t("art.store.searchPlaceholder")}
                  value={storeSearchText}
                  onChange={(event) => setStoreSearchText(event.target.value)}
                />
                <Checkbox
                  className="loom-art-official"
                  label={t("art.store.officialShort")}
                  title={t("art.store.officialOnlyAria")}
                  aria-label={t("art.store.officialOnlyAria")}
                  checked={storeOfficialOnly}
                  onChange={(event) => setStoreOfficialOnly(event.target.checked)}
                />
              </>
            }
          />
        ) : null}
      </div>

      {frameworkError ? (
        <Notice tone="danger" className="loom-art-workspace-message" role="alert">
          <span>{frameworkError}</span>
          <Button onClick={() => void loadFrameworks()}>{t("art.framework.retryStatus")}</Button>
        </Notice>
      ) : null}

      {snapshotRefreshError ? (
        <Notice tone="warning" className="loom-art-workspace-message" role="alert">
          <span>{snapshotRefreshError}</span>
          <Button onClick={() => void synchronizeArtState()}>{t("art.snapshot.resyncStatus")}</Button>
        </Notice>
      ) : null}

      <TabPanel
        value="registry"
        forceMount
        className="art-hub__surface"
        id="art-panel-registry"
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
      </TabPanel>
      <TabPanel
        value="store"
        forceMount
        className="art-hub__surface"
        id="art-panel-store"
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
      </TabPanel>
      <TabPanel
        value="security"
        forceMount
        className="art-hub__surface"
        id="art-panel-security"
        aria-labelledby="art-tab-security"
        hidden={activeWorkspace !== "security"}
      >
        <PluginSecurityPanel baseUrl={baseUrl} />
      </TabPanel>
      <FrameworkManagementDialog
        open={frameworkDialogOpen}
        frameworks={frameworks}
        loading={frameworksLoading}
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
    </TabsRoot>
  );
}
