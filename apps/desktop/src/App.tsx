import {
  DEFAULT_HOOK_BRIDGE_URL,
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
import { HookBridgePanel } from "./components/hook/HookBridgePanel";
import { type WorkflowArtCreationRequest } from "./components/hook/HookCanvasThumbnail";
import { McpPanel } from "./components/mcp/McpPanel";
import { SettingsPanel } from "./components/settings/SettingsPanel";
import { startHookBridgeWorkflowSync } from "./services/hookBridgeWorkflowSync";
import {
  getHookCanvasRefreshTrigger,
  type HookCanvasSnapshot,
  keepNewestHookCanvasSnapshot,
  readHookCanvasSnapshot,
} from "./services/hookCanvas";
import { createLatestRequestGate, createSingleFlightGate } from "./services/latestRequest";
import {
  bootstrapPackagedArts,
  DEFAULT_LOOM_DAEMON_URL,
  getLoomSettings,
  LoomSnapshot,
  readLoomSnapshot,
  retainAvailableSnapshotData,
  startHookBridge,
  startLoomDaemon,
  waitForLoomOnline,
} from "./services/loomApi";
import { applyLoomGeneralSettings } from "./services/loomGeneralSettings";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

export default function App() {
  const snapshotRequestGate = useRef(createLatestRequestGate());
  const snapshotSingleFlight = useRef(createSingleFlightGate());
  const hookCanvasRequestGate = useRef(createLatestRequestGate());
  const hookCanvasSingleFlight = useRef(createSingleFlightGate());
  const hookCanvasFlightBaseUrl = useRef<string | null>(null);
  const packagedArtsBootstrapBaseUrl = useRef<string | null>(null);
  const appMountedRef = useRef(true);
  const [activeSection, setActiveSection] = useState<SectionId>("mcp");
  const [railCollapsed, setRailCollapsed] = useState(false);
  const [snapshot, setSnapshot] = useState<LoomSnapshot>(fallbackSnapshot);
  const [loading, setLoading] = useState(false);
  const [autoStartAttempted, setAutoStartAttempted] = useState(false);
  const [pendingArtCreationRequest, setPendingArtCreationRequest] = useState<ArtCreationRequest | null>(null);
  const [hookCanvas, setHookCanvas] = useState<HookCanvasSnapshot | null>(null);
  const [hookCanvasLoading, setHookCanvasLoading] = useState(false);
  const [hookCanvasError, setHookCanvasError] = useState<string | null>(null);
  const [hookCanvasRefreshVersion, setHookCanvasRefreshVersion] = useState(0);
  const [hookBridgeUrl, setHookBridgeUrl] = useState(DEFAULT_HOOK_BRIDGE_URL);
  const hookCanvasRefreshTrigger = getHookCanvasRefreshTrigger({
    connectionState: snapshot.connectionState,
    baseUrl: snapshot.baseUrl,
    refreshVersion: hookCanvasRefreshVersion,
  });

  const refreshSnapshot = useCallback(async (abortSignal?: AbortSignal): Promise<LoomSnapshot> => {
    if (!appMountedRef.current) return fallbackSnapshot;
    return await snapshotSingleFlight.current.run(async () => {
      const requestToken = snapshotRequestGate.current.begin();
      const abortRequest = () => {
        if (snapshotRequestGate.current.isCurrent(requestToken)) {
          snapshotRequestGate.current.invalidate();
          setLoading(false);
        }
      };
      abortSignal?.addEventListener("abort", abortRequest, { once: true });
      setLoading(true);
      try {
        let baseUrl = DEFAULT_LOOM_DAEMON_URL;
        let nextHookBridgeUrl = DEFAULT_HOOK_BRIDGE_URL;
        try {
          const runtimeConfig = await invoke<RuntimeConfig>("resolve_loom_daemon_url");
          baseUrl = runtimeConfig.loomDaemonUrl || DEFAULT_LOOM_DAEMON_URL;
          nextHookBridgeUrl = runtimeConfig.hookBridgeUrl || DEFAULT_HOOK_BRIDGE_URL;
        } catch {
          baseUrl = DEFAULT_LOOM_DAEMON_URL;
        }
        const next = await readLoomSnapshot(baseUrl);
        if (!abortSignal?.aborted && snapshotRequestGate.current.isCurrent(requestToken)) {
          setHookBridgeUrl(nextHookBridgeUrl);
          setSnapshot((previous) => retainAvailableSnapshotData(previous, next));
        }
        return next;
      } finally {
        abortSignal?.removeEventListener("abort", abortRequest);
        if (snapshotRequestGate.current.isCurrent(requestToken)) {
          setLoading(false);
        }
      }
    });
  }, []);

  const refresh = useCallback(async (): Promise<void> => {
    await refreshSnapshot();
  }, [refreshSnapshot]);

  const refreshHookCanvas = useCallback(async (baseUrl: string) => {
    if (hookCanvasFlightBaseUrl.current !== baseUrl) {
      hookCanvasFlightBaseUrl.current = baseUrl;
      hookCanvasRequestGate.current.invalidate();
      hookCanvasSingleFlight.current.invalidate();
    }
    await hookCanvasSingleFlight.current.run(async () => {
      const requestToken = hookCanvasRequestGate.current.begin();
      setHookCanvasLoading(true);
      try {
        const next = await readHookCanvasSnapshot(baseUrl);
        if (hookCanvasRequestGate.current.isCurrent(requestToken)) {
          setHookCanvas((previous) => keepNewestHookCanvasSnapshot(previous, next));
          setHookCanvasError(null);
        }
      } catch (error) {
        if (hookCanvasRequestGate.current.isCurrent(requestToken)) {
          setHookCanvasError(error instanceof Error ? error.message : "无法读取 Hook 画布。");
        }
      } finally {
        if (hookCanvasRequestGate.current.isCurrent(requestToken)) {
          setHookCanvasLoading(false);
        }
      }
    });
  }, []);

  const startLocalService = async (abortSignal?: AbortSignal) => {
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
      hookCanvasRequestGate.current.invalidate();
      hookCanvasSingleFlight.current.invalidate();
    };
  }, []);

  useEffect(() => {
    if (snapshot.connectionState !== "online") return;
    let cancelled = false;
    void getLoomSettings(snapshot.baseUrl)
      .then(async (settings) => {
        if (cancelled) return;
        applyLoomGeneralSettings(settings.general);
        await invoke("apply_loom_general_settings", {
          settings: { minimizeToTray: settings.general.minimize_to_tray },
        });
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [snapshot.baseUrl, snapshot.connectionState]);

  useEffect(() => {
    if (hookCanvasRefreshTrigger === null) {
      hookCanvasFlightBaseUrl.current = null;
      hookCanvasRequestGate.current.invalidate();
      hookCanvasSingleFlight.current.invalidate();
      setHookCanvasLoading(false);
      return;
    }
    void refreshHookCanvas(snapshot.baseUrl);
  }, [hookCanvasRefreshTrigger, refreshHookCanvas, snapshot.baseUrl]);

  // Auto-sync the Hook canvas while online. Hook persists position and image
  // edits to session.json without always emitting a bridge broadcast, so poll
  // the snapshot on an interval. The daemon returns a cheap content revision and
  // keepNewestHookCanvasSnapshot dedupes by revision, so an unchanged canvas
  // does not cause a re-render or reload previews.
  useEffect(() => {
    if (hookCanvasRefreshTrigger === null || activeSection !== "hook-bridge") {
      return;
    }
    const interval = window.setInterval(() => {
      void refreshHookCanvas(snapshot.baseUrl);
    }, HOOK_CANVAS_POLL_INTERVAL_MS);
    return () => {
      window.clearInterval(interval);
    };
  }, [activeSection, hookCanvasRefreshTrigger, refreshHookCanvas, snapshot.baseUrl]);

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
      snapshot.connectionState !== "online"
      || packagedArtsBootstrapBaseUrl.current === snapshot.baseUrl
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
          text: lastError instanceof Error ? lastError.message : "无法加载打包 Art。",
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

  // Ensure the configured Hook bridge is running once the daemon is online.
  // Idempotent: the daemon returns 409 if already running, which we ignore.
  useEffect(() => {
    if (snapshot.connectionState !== "online") {
      return;
    }
    let bridgePort: number | undefined;
    try {
      const parsedPort = Number(new URL(hookBridgeUrl).port);
      bridgePort = Number.isInteger(parsedPort) && parsedPort > 0 ? parsedPort : undefined;
    } catch {
      bridgePort = undefined;
    }
    void startHookBridge(snapshot.baseUrl, bridgePort).catch(() => {
      // Already running or transient failure — the workflow-sync client below
      // will retry connecting regardless.
    });
  }, [hookBridgeUrl, snapshot.connectionState, snapshot.baseUrl]);

  useEffect(() => {
    if (
      snapshot.connectionState !== "online"
      || typeof window === "undefined"
      || typeof WebSocket === "undefined"
    ) {
      return;
    }
    const sync = startHookBridgeWorkflowSync({
      refresh,
      websocketUrl: hookBridgeUrl,
      invalidateHookCanvas: () => {
        setHookCanvasRefreshVersion((version) => version + 1);
      },
    });

    return () => {
      sync.dispose();
    };
  }, [hookBridgeUrl, refresh, snapshot.connectionState]);

  const openWorkflowArtCreator = useCallback((request: WorkflowArtCreationRequest) => {
    setPendingArtCreationRequest({
      requestId: `${request.workflowId}-${Date.now()}`,
      mode: "workflow",
      repositoryName: request.tool.id,
      name: request.tool.name || request.workflowName,
      description: request.tool.description || "由 Hook 工作流创建的 Art。",
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

  const runWindowCommand = useCallback(async (
    command: "minimize" | "toggle-maximize" | "close",
  ): Promise<void> => {
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
    <button
      className={activeSection === item.id ? "rail-item rail-item--active" : "rail-item"}
      type="button"
      key={item.id}
      title={item.label}
      aria-label={item.label}
      aria-current={activeSection === item.id ? "page" : undefined}
      data-testid={item.id === "hook-bridge" ? "nav-hook-bridge" : undefined}
      onClick={() => setActiveSection(item.id)}
    >
      <span className="rail-item__icon"><ShellIcon kind={item.icon} /></span>
      <span className="rail-item__label">{item.label}</span>
    </button>
  );

  return (
    <main className={railCollapsed ? "desktop-shell desktop-shell--rail-collapsed" : "desktop-shell"}>
      <AppToastViewport />
      <AppConfirmViewport />
      <header className="app-titlebar">
        <div
          className="app-titlebar__drag-region"
          data-tauri-drag-region
          onDoubleClick={() => void runWindowCommand("toggle-maximize")}
        >
          {activeSection === "settings" ? (
            <button
              className="app-titlebar__back"
              type="button"
              aria-label="返回 MCP"
              title="返回 MCP"
              onDoubleClick={(event) => event.stopPropagation()}
              onClick={() => setActiveSection("mcp")}
            >
              <ShellIcon kind="back" />
            </button>
          ) : null}
        </div>
        <div className="app-titlebar__controls">
          <button
            className={loading ? "window-control window-control--refresh window-control--loading" : "window-control window-control--refresh"}
            type="button"
            aria-label={loading ? "正在刷新" : "刷新"}
            title={loading ? "正在刷新" : "刷新"}
            onClick={() => void refresh()}
            disabled={loading}
          >
            <ShellIcon kind="refresh" />
          </button>
          <button
            className="window-control"
            type="button"
            aria-label="最小化"
            title="最小化"
            onClick={() => void runWindowCommand("minimize")}
          >
            <ShellIcon kind="minimize" />
          </button>
          <button
            className="window-control"
            type="button"
            aria-label="最大化或还原"
            title="最大化或还原"
            onClick={() => void runWindowCommand("toggle-maximize")}
          >
            <ShellIcon kind="maximize" />
          </button>
          <button
            className="window-control window-control--close"
            type="button"
            aria-label="关闭"
            title="关闭"
            onClick={() => void runWindowCommand("close")}
          >
            <ShellIcon kind="close" />
          </button>
        </div>
      </header>

      <aside className="left-rail">
        <div className="app-titlebar__brand">
          <button
            className="shell-icon-button shell-rail-toggle"
            type="button"
            aria-label={railCollapsed ? "展开侧栏" : "收起侧栏"}
            title={railCollapsed ? "展开侧栏" : "收起侧栏"}
            aria-expanded={!railCollapsed}
            onClick={() => setRailCollapsed((collapsed) => !collapsed)}
          >
            <span className="shell-rail-toggle__icon"><ShellIcon kind="sidebar" /></span>
            <span className="shell-rail-toggle__mark"><LoomMark /></span>
          </button>
          <span className="app-titlebar__product-mark"><LoomMark /></span>
          <strong className="app-titlebar__product-name">Loom</strong>
        </div>

        <nav className="rail-nav" aria-label="Loom sections">
          {primaryNavigationItems.map(renderNavigationItem)}
        </nav>

        <div className="rail-footer">
          <button
            className={activeSection === "devices" ? "rail-item rail-item--active rail-device-button" : "rail-item rail-device-button"}
            type="button"
            title="设备管理"
            aria-label="设备管理"
            aria-current={activeSection === "devices" ? "page" : undefined}
            onClick={() => setActiveSection("devices")}
          >
            <span className="rail-item__icon"><ShellIcon kind="device" /></span>
            <span className="rail-item__label">设备管理</span>
          </button>
          <nav className="rail-utility-nav" aria-label="Loom utilities">
            {utilityNavigationItems.map(renderNavigationItem)}
          </nav>
        </div>
      </aside>

      <section className={activeSection === "settings"
        ? "workspace-panel workspace-panel--settings"
        : activeSection === "registry" || activeSection === "hook-bridge"
          ? "workspace-panel workspace-panel--tooling"
          : "workspace-panel"}>
        {activeSection !== "devices" && activeSection !== "settings" ? <header className={activeSection === "registry" || activeSection === "hook-bridge"
          ? "workspace-header workspace-header--tooling"
          : "workspace-header"}>
          <div>
            {activeNavigation.eyebrow ? (
              <p className="section-kicker">{activeNavigation.eyebrow}</p>
            ) : null}
            <h1>{activeNavigation.label}</h1>
          </div>
        </header> : null}

        <div className={activeSection === "devices"
          ? "workspace-scroll workspace-scroll--devices"
          : activeSection === "settings"
            ? "workspace-scroll workspace-scroll--settings"
            : activeSection === "registry" || activeSection === "hook-bridge"
              ? "workspace-scroll workspace-scroll--tooling"
              : "workspace-scroll"}>
          {activeSection === "mcp" && (
            <McpPanel
              servers={snapshot.mcpServers}
              baseUrl={snapshot.baseUrl}
              refresh={refresh}
            />
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
          {activeSection === "hook-bridge" && (
            <HookBridgePanel
              baseUrl={snapshot.baseUrl}
              hookCanvas={hookCanvas}
              hookCanvasError={hookCanvasError}
              tools={snapshot.tools}
              onCreateWorkflowArt={openWorkflowArtCreator}
            />
          )}
          {activeSection === "devices" && (
            <DeviceManagementPanel
              baseUrl={snapshot.baseUrl}
              online={snapshot.connectionState === "online"}
            />
          )}
          {activeSection === "settings" && <SettingsPanel snapshot={snapshot} />}
        </div>
      </section>
    </main>
  );
}
