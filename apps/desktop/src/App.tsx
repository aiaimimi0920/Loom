import {
  DEFAULT_OTTER_BRIDGE_URL,
  fallbackSnapshot,
  HOOK_CANVAS_POLL_INTERVAL_MS,
  LoomMark,
  NavigationItem,
  navigationItems,
  primaryNavigationItems,
  RuntimeConfig,
  SectionId,
  ShellIcon,
  utilityNavigationItems,
} from "./components/app/appShell";
import { ArtPanel } from "./components/art/ArtPanel";
import { ArtCreationRequest } from "./components/art/artWizardModel";
import { DeviceManagementPanel } from "./components/devices/DeviceManagementPanel";
import { AppConfirmViewport, AppToastViewport, pushAppToast } from "./components/feedback/AppFeedback";
import { OtterBridgePanel } from "./components/otter/OtterBridgePanel";
import { type WorkflowArtCreationRequest } from "./components/otter/OtterCanvasThumbnail";
import { McpPanel } from "./components/mcp/McpPanel";
import { SettingsPanel } from "./components/settings/SettingsPanel";
import { maintainOtterBridgeAvailability } from "./services/otterBridgeAvailability";
import { startOtterBridgeWorkflowSync } from "./services/otterBridgeWorkflowSync";
import {
  getOtterCanvasRefreshTrigger,
  type OtterCanvasSnapshot,
  keepNewestOtterCanvasSnapshot,
  readOtterCanvasSnapshot,
} from "./services/otterCanvas";
import { createLatestRequestGate, createSingleFlightGate } from "./services/latestRequest";
import { refreshSharedSnapshot } from "./services/snapshotRefresh";
import {
  bootstrapPackagedArts,
  DEFAULT_LOOM_DAEMON_URL,
  getLoomSettings,
  LoomSnapshot,
  readLoomSnapshot,
  retainAvailableSnapshotData,
  startLoomDaemon,
  waitForLoomOnline,
} from "./services/loomApi";
import { applyLoomGeneralSettings } from "./services/loomGeneralSettings";
import { isDesktopTauriShell, resolveBrowserDaemonBaseUrl } from "./services/loomRuntimeTarget";
import { uiText, useUiText } from "./i18n/index.ts";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { IconButton, NavigationItem as NeuroNavigationItem } from "@neuro/ui";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

export default function App() {
  const { t } = useUiText();
  const snapshotRequestGate = useRef(createLatestRequestGate());
  const snapshotSingleFlight = useRef(createSingleFlightGate());
  const otterCanvasRequestGate = useRef(createLatestRequestGate());
  const otterCanvasSingleFlight = useRef(createSingleFlightGate());
  const otterCanvasFlightBaseUrl = useRef<string | null>(null);
  const packagedArtsBootstrapBaseUrl = useRef<string | null>(null);
  const appMountedRef = useRef(true);
  const [activeSection, setActiveSection] = useState<SectionId>("mcp");
  const [railCollapsed, setRailCollapsed] = useState(() => window.matchMedia("(max-width: 820px)").matches);
  const [snapshot, setSnapshot] = useState<LoomSnapshot>(fallbackSnapshot);
  const [loading, setLoading] = useState(false);
  const [autoStartAttempted, setAutoStartAttempted] = useState(false);
  const [pendingArtCreationRequest, setPendingArtCreationRequest] = useState<ArtCreationRequest | null>(null);
  const [otterCanvas, setOtterCanvas] = useState<OtterCanvasSnapshot | null>(null);
  const [otterCanvasLoading, setOtterCanvasLoading] = useState(false);
  const [otterCanvasError, setOtterCanvasError] = useState<string | null>(null);
  const [otterCanvasRefreshVersion, setOtterCanvasRefreshVersion] = useState(0);
  const [hookBridgeUrl, setOtterBridgeUrl] = useState(DEFAULT_OTTER_BRIDGE_URL);
  const otterCanvasRefreshTrigger = getOtterCanvasRefreshTrigger({
    connectionState: snapshot.connectionState,
    baseUrl: snapshot.baseUrl,
    refreshVersion: otterCanvasRefreshVersion,
  });

  useEffect(() => {
    const query = window.matchMedia("(max-width: 820px)");
    const updateRail = (event: MediaQueryListEvent) => setRailCollapsed(event.matches);
    query.addEventListener("change", updateRail);
    return () => query.removeEventListener("change", updateRail);
  }, []);

  const refreshSnapshot = useCallback(async (abortSignal?: AbortSignal): Promise<LoomSnapshot> => {
    if (!appMountedRef.current) return fallbackSnapshot;
    const result = await refreshSharedSnapshot({
      latest: snapshotRequestGate.current,
      flight: snapshotSingleFlight.current,
      signal: abortSignal,
      loading: setLoading,
      read: async () => {
        if (!isDesktopTauriShell()) {
          const baseUrl = resolveBrowserDaemonBaseUrl();
          const next = await readLoomSnapshot(baseUrl);
          return { next, nextOtterBridgeUrl: DEFAULT_OTTER_BRIDGE_URL };
        }
        let baseUrl = DEFAULT_LOOM_DAEMON_URL;
        let nextOtterBridgeUrl = DEFAULT_OTTER_BRIDGE_URL;
        try {
          const runtimeConfig = await invoke<RuntimeConfig>("resolve_loom_daemon_url");
          baseUrl = runtimeConfig.loomDaemonUrl || DEFAULT_LOOM_DAEMON_URL;
          nextOtterBridgeUrl = runtimeConfig.hookBridgeUrl || DEFAULT_OTTER_BRIDGE_URL;
        } catch {
          baseUrl = DEFAULT_LOOM_DAEMON_URL;
        }
        const next = await readLoomSnapshot(baseUrl);
        return { next, nextOtterBridgeUrl };
      },
      apply: ({ next, nextOtterBridgeUrl }) => {
        setOtterBridgeUrl(nextOtterBridgeUrl);
        setSnapshot((previous) => retainAvailableSnapshotData(previous, next));
      },
    });
    return result.next;
  }, []);

  const refresh = useCallback(async (): Promise<void> => {
    await refreshSnapshot();
  }, [refreshSnapshot]);

  const refreshOtterCanvas = useCallback(async (baseUrl: string) => {
    if (otterCanvasFlightBaseUrl.current !== baseUrl) {
      otterCanvasFlightBaseUrl.current = baseUrl;
      otterCanvasRequestGate.current.invalidate();
      otterCanvasSingleFlight.current.invalidate();
    }
    await otterCanvasSingleFlight.current.run(async () => {
      const requestToken = otterCanvasRequestGate.current.begin();
      setOtterCanvasLoading(true);
      try {
        const next = await readOtterCanvasSnapshot(baseUrl);
        if (otterCanvasRequestGate.current.isCurrent(requestToken)) {
          setOtterCanvas((previous) => keepNewestOtterCanvasSnapshot(previous, next));
          setOtterCanvasError(null);
        }
      } catch (error) {
        if (otterCanvasRequestGate.current.isCurrent(requestToken)) {
          setOtterCanvasError(error instanceof Error ? error.message : uiText("otter.canvas.readFailed"));
        }
      } finally {
        if (otterCanvasRequestGate.current.isCurrent(requestToken)) {
          setOtterCanvasLoading(false);
        }
      }
    });
  }, []);

  const startLocalService = async (abortSignal?: AbortSignal) => {
    if (!isDesktopTauriShell()) {
      await refreshSnapshot(abortSignal).catch(() => undefined);
      return;
    }
    try {
      await startLoomDaemon();
      if (abortSignal?.aborted) return;
      await waitForLoomOnline(refreshSnapshot);
    } catch {
      if (!abortSignal?.aborted) {
        await refreshSnapshot(abortSignal).catch(() => undefined);
      }
    }
  };

  useEffect(() => {
    const abortController = new AbortController();
    appMountedRef.current = true;
    setAutoStartAttempted(true);
    void startLocalService(abortController.signal);
    return () => {
      appMountedRef.current = false;
      abortController.abort();
      snapshotRequestGate.current.invalidate();
      snapshotSingleFlight.current.invalidate();
      otterCanvasRequestGate.current.invalidate();
      otterCanvasSingleFlight.current.invalidate();
    };
  }, []);

  useEffect(() => {
    if (snapshot.connectionState !== "online") return;
    let cancelled = false;
    void getLoomSettings(snapshot.baseUrl)
      .then(async (settings) => {
        if (cancelled) return;
        applyLoomGeneralSettings(settings.general);
        if (isDesktopTauriShell()) {
          await invoke("apply_loom_general_settings", {
            settings: { minimizeToTray: settings.general.minimize_to_tray },
          });
        }
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [snapshot.baseUrl, snapshot.connectionState]);

  useEffect(() => {
    if (otterCanvasRefreshTrigger === null) {
      otterCanvasFlightBaseUrl.current = null;
      otterCanvasRequestGate.current.invalidate();
      otterCanvasSingleFlight.current.invalidate();
      setOtterCanvasLoading(false);
      return;
    }
    void refreshOtterCanvas(snapshot.baseUrl);
  }, [otterCanvasRefreshTrigger, refreshOtterCanvas, snapshot.baseUrl]);

  // Auto-sync the Otter canvas while online. Otter persists position and image
  // edits to session.json without always emitting a bridge broadcast, so poll
  // the snapshot on an interval. The daemon returns a cheap content revision and
  // keepNewestOtterCanvasSnapshot dedupes by revision, so an unchanged canvas
  // does not cause a re-render or reload previews.
  useEffect(() => {
    if (otterCanvasRefreshTrigger === null || activeSection !== "otter-bridge") {
      return;
    }
    const interval = window.setInterval(() => {
      void refreshOtterCanvas(snapshot.baseUrl);
    }, HOOK_CANVAS_POLL_INTERVAL_MS);
    return () => {
      window.clearInterval(interval);
    };
  }, [activeSection, otterCanvasRefreshTrigger, refreshOtterCanvas, snapshot.baseUrl]);

  useEffect(() => {
    if (
      autoStartAttempted ||
      loading ||
      snapshot.connectionState !== "offline" ||
      snapshot.checkedAt === fallbackSnapshot.checkedAt
    ) {
      return;
    }
    setAutoStartAttempted(true);
    void startLocalService();
  }, [autoStartAttempted, loading, snapshot.connectionState, snapshot.checkedAt]);

  useEffect(() => {
    if (
      !isDesktopTauriShell() ||
      snapshot.connectionState !== "online" ||
      packagedArtsBootstrapBaseUrl.current === snapshot.baseUrl
    ) {
      return;
    }
    packagedArtsBootstrapBaseUrl.current = snapshot.baseUrl;
    let cancelled = false;
    void (async () => {
      let lastError: unknown = null;
      for (let attempt = 0; attempt < 3; attempt += 1) {
        if (cancelled) return;
        try {
          const result = await bootstrapPackagedArts(snapshot.baseUrl);
          if (!cancelled && result.applied) {
            await refresh();
          }
          return;
        } catch (error) {
          lastError = error;
          if (attempt < 2) {
            await new Promise((resolve) => window.setTimeout(resolve, 1000 * (attempt + 1)));
          }
        }
      }
      if (!cancelled) {
        pushAppToast({
          level: "error",
          text: lastError instanceof Error ? lastError.message : uiText("shell.packagedArtsBootstrapFailed"),
        });
      }
    })();
    return () => {
      cancelled = true;
      if (packagedArtsBootstrapBaseUrl.current === snapshot.baseUrl) {
        packagedArtsBootstrapBaseUrl.current = null;
      }
    };
  }, [refresh, snapshot.baseUrl, snapshot.connectionState]);

  // Keep the daemon-owned bridge alive; WebSocket retries alone cannot recover
  // when a transient startup failure leaves the listener stopped.
  useEffect(() => {
    if (snapshot.connectionState !== "online") return;
    const availability = maintainOtterBridgeAvailability({
      baseUrl: snapshot.baseUrl,
      websocketUrl: hookBridgeUrl,
    });
    return () => availability.dispose();
  }, [hookBridgeUrl, snapshot.connectionState, snapshot.baseUrl]);

  useEffect(() => {
    if (snapshot.connectionState !== "online" || typeof window === "undefined") {
      return;
    }
    const sync = startOtterBridgeWorkflowSync({
      refresh,
      invalidateOtterCanvas: () => {
        setOtterCanvasRefreshVersion((version) => version + 1);
      },
    });

    return () => {
      sync.dispose();
    };
  }, [refresh, snapshot.connectionState]);

  const openWorkflowArtCreator = useCallback((request: WorkflowArtCreationRequest) => {
    setPendingArtCreationRequest({
      requestId: `${request.workflowId}-${Date.now()}`,
      mode: "workflow",
      repositoryName: request.tool.id,
      name: request.tool.name || request.workflowName,
      description: request.tool.description || uiText("otter.workflowArt.defaultDescription"),
      workflowId: request.workflowId,
      templateTool: request.tool,
    });
    setActiveSection("registry");
  }, []);

  const handleArtCreationRequestHandled = useCallback(() => {
    setPendingArtCreationRequest(null);
  }, []);

  const activeNavigation = useMemo(
    () => navigationItems.find((item) => item.id === activeSection) ?? navigationItems[0],
    [activeSection],
  );

  const runWindowCommand = useCallback(async (command: "minimize" | "toggle-maximize" | "close"): Promise<void> => {
    try {
      const currentWindow = getCurrentWindow();
      if (command === "minimize") await currentWindow.minimize();
      if (command === "toggle-maximize") await currentWindow.toggleMaximize();
      if (command === "close") await currentWindow.close();
    } catch {
      // Browser previews do not expose a native Tauri window.
    }
  }, []);

  const renderNavigationItem = (item: NavigationItem) => (
    <NeuroNavigationItem
      className="rail-item"
      active={activeSection === item.id}
      key={item.id}
      title={t(item.labelKey)}
      aria-label={t(item.labelKey)}
      data-testid={item.id === "otter-bridge" ? "nav-otter-bridge" : undefined}
      onClick={() => setActiveSection(item.id)}
    >
      <span className="rail-item__icon"><ShellIcon kind={item.icon} /></span>
      <span className="rail-item__label">{t(item.labelKey)}</span>
    </NeuroNavigationItem>
  );

  const isTooling = activeSection === "registry" || activeSection === "otter-bridge";
  const panelClass =
    activeSection === "settings"
      ? "workspace-panel workspace-panel--settings"
      : isTooling
        ? "workspace-panel workspace-panel--tooling"
        : "workspace-panel";
  const scrollClass =
    activeSection === "devices"
      ? "workspace-scroll workspace-scroll--devices"
      : activeSection === "settings"
        ? "workspace-scroll workspace-scroll--settings"
        : isTooling
          ? "workspace-scroll workspace-scroll--tooling"
          : "workspace-scroll";

  return (
    <main className={railCollapsed ? "desktop-shell desktop-shell--rail-collapsed" : "desktop-shell"}>
      <AppToastViewport />
      <AppConfirmViewport />
      <header className="app-titlebar">
        <div
          className="app-titlebar__drag-region"
          data-tauri-drag-region={isDesktopTauriShell() || undefined}
          onDoubleClick={isDesktopTauriShell() ? () => void runWindowCommand("toggle-maximize") : undefined}
        >
          {activeSection === "settings" ? (
            <IconButton
              className="app-titlebar__back"
              variant="quiet"
              label={t("shell.titlebar.backToMcp")}
              title={t("shell.titlebar.backToMcp")}
              onDoubleClick={(event) => event.stopPropagation()}
              onClick={() => setActiveSection("mcp")}
            >
              <ShellIcon kind="back" />
            </IconButton>
          ) : null}
        </div>
        <div className="app-titlebar__controls">
          <IconButton
            className="loom-window-control loom-window-control--refresh"
            variant="quiet"
            label={t("common.refresh")}
            title={t("common.refresh")}
            loading={loading}
            onClick={() => void refresh()}
          >
            <ShellIcon kind="refresh" />
          </IconButton>
          {isDesktopTauriShell() ? <><IconButton
            className="loom-window-control"
            variant="quiet"
            label={t("shell.window.minimize")}
            title={t("shell.window.minimize")}
            onClick={() => void runWindowCommand("minimize")}
          >
            <ShellIcon kind="minimize" />
          </IconButton>
          <IconButton
            className="loom-window-control"
            variant="quiet"
            label={t("shell.window.toggleMaximize")}
            title={t("shell.window.toggleMaximize")}
            onClick={() => void runWindowCommand("toggle-maximize")}
          >
            <ShellIcon kind="maximize" />
          </IconButton>
          <IconButton
            className="loom-window-control"
            variant="danger"
            label={t("common.close")}
            title={t("common.close")}
            onClick={() => void runWindowCommand("close")}
          >
            <ShellIcon kind="close" />
          </IconButton></> : null}
        </div>
      </header>

      <aside className="left-rail">
        <div className="app-titlebar__brand">
          <IconButton
            className="shell-rail-toggle"
            variant="quiet"
            label={railCollapsed ? t("shell.rail.expand") : t("shell.rail.collapse")}
            title={railCollapsed ? t("shell.rail.expand") : t("shell.rail.collapse")}
            aria-expanded={!railCollapsed}
            onClick={() => setRailCollapsed((collapsed) => !collapsed)}
          >
            <span className="shell-rail-toggle__icon"><ShellIcon kind="sidebar" /></span>
            <span className="shell-rail-toggle__mark"><LoomMark /></span>
          </IconButton>
          <span className="app-titlebar__product-mark"><LoomMark /></span>
          <strong className="app-titlebar__product-name">Loom</strong>
        </div>

        <nav className="rail-nav" aria-label={t("shell.rail.sectionsAria")}>
          {primaryNavigationItems.map(renderNavigationItem)}
        </nav>

        <div className="rail-footer">
          <NeuroNavigationItem
            className="rail-item rail-device-button"
            active={activeSection === "devices"}
            title={t("shell.nav.devices")}
            aria-label={t("shell.nav.devices")}
            onClick={() => setActiveSection("devices")}
          >
            <span className="rail-item__icon"><ShellIcon kind="device" /></span>
            <span className="rail-item__label">{t("shell.nav.devices")}</span>
          </NeuroNavigationItem>
          <nav className="rail-utility-nav" aria-label={t("shell.rail.utilitiesAria")}>
            {utilityNavigationItems.map(renderNavigationItem)}
          </nav>
        </div>
      </aside>

      <section className={panelClass}>
        {activeSection !== "devices" && activeSection !== "settings" ? (
          <header className={isTooling ? "workspace-header workspace-header--tooling" : "workspace-header"}>
            <div><h1>{t(activeNavigation.labelKey)}</h1></div>
          </header>
        ) : null}

        <div className={scrollClass}>
          {activeSection === "mcp" && (
            <McpPanel servers={snapshot.mcpServers} baseUrl={snapshot.baseUrl} refresh={refresh} />
          )}
          {activeSection === "registry" && (
            <ArtPanel
              tools={snapshot.tools}
              mcpServers={snapshot.mcpServers}
              workflows={snapshot.workflows}
              baseUrl={snapshot.baseUrl}
              refresh={refresh}
              pendingCreationRequest={pendingArtCreationRequest}
              onCreationRequestHandled={handleArtCreationRequestHandled}
            />
          )}
          {activeSection === "otter-bridge" && (
            <OtterBridgePanel
              baseUrl={snapshot.baseUrl}
              otterCanvas={otterCanvas}
              otterCanvasError={otterCanvasError}
              tools={snapshot.tools}
              onCreateWorkflowArt={openWorkflowArtCreator}
            />
          )}
          {activeSection === "devices" && (
            <DeviceManagementPanel baseUrl={snapshot.baseUrl} online={snapshot.connectionState === "online"} />
          )}
          {activeSection === "settings" && <SettingsPanel snapshot={snapshot} />}
        </div>
      </section>
    </main>
  );
}
