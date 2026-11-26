// Owns settings hydration, ordered autosave, runtime application and section state.
import {
  DEFAULT_LOOM_SETTINGS,
  getLoomAppPaths,
  getLoomSettings,
  getLoomShortcuts,
  OtterCacheSettings,
  listPluginTrust,
  LoomAppPaths,
  LoomArtStoreSettings,
  LoomCacheSettings,
  LoomMcpSettings,
  type LoomPluginTrustPolicy,
  LoomProxySettings,
  LoomSettings,
  LoomShortcutConfig,
  LoomSnapshot,
  saveLoomSettings,
  setPluginTrustPolicy,
} from "../../services/loomApi";
import { applyLoomGeneralSettings } from "../../services/loomGeneralSettings";
import { uiFormat, uiText } from "../../i18n/index.ts";
import { pushAppToast } from "../feedback/AppFeedback";
import { useApplicationLinks } from "./useApplicationLinks";
import {
  ApplicationDiagnosticsInfo,
  FALLBACK_APPLICATION_DIAGNOSTICS,
  LoomCacheSnapshotInfo,
  SettingsAppId,
  SettingsSectionId,
} from "./settingsModel";
import {
  GeneralSettingsValue,
  otterCachePreferencesForRuntime,
  otterCacheSettingsForUi,
  loomCachePreferencesForRuntime,
  loomCacheSettingsForUi,
} from "./SettingsPanels";
import { useSettingsCacheControls } from "./useSettingsCacheControls";
import { useSettingsShortcutEditors } from "./useSettingsShortcutEditors";
import { hydrateSettings } from "./settingsHydration";
import { isDesktopTauriShell } from "../../services/loomRuntimeTarget";
import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState } from "react";

export function useSettingsPanelController({ snapshot }: { snapshot: LoomSnapshot }) {
  const [draft, setDraftState] = useState<LoomSettings>(DEFAULT_LOOM_SETTINGS);
  const draftRef = useRef(draft);
  const [settingsLoading, setSettingsLoading] = useState(true);
  const [shortcuts, setShortcuts] = useState<LoomShortcutConfig[]>(Object.values(DEFAULT_LOOM_SETTINGS.shortcuts));
  const [appPaths, setAppPaths] = useState<LoomAppPaths | null>(null);
  const [appDiagnostics, setAppDiagnostics] = useState<Record<SettingsAppId, ApplicationDiagnosticsInfo>>(
    FALLBACK_APPLICATION_DIAGNOSTICS,
  );
  const settingsMountedRef = useRef(true);
  const {
    loomCacheSnapshot,
    loomCacheLoading,
    loomCacheBusyKind,
    otterCacheSnapshot,
    otterCacheLoading,
    otterCacheBusyKind,
    setLoomCacheSnapshot,
    refreshLoomCache,
    refreshOtterCache,
    clearLoomCache,
    clearOtterCache,
  } = useSettingsCacheControls(settingsMountedRef);
  const [artStoreTrustPolicy, setArtStoreTrustPolicy] = useState<LoomPluginTrustPolicy>("allow_unsigned");
  const [artStoreTrustPolicyBusy, setArtStoreTrustPolicyBusy] = useState(false);
  const [activeSettingsApp, setActiveSettingsApp] = useState<SettingsAppId>("loom");
  const [openSettingsSection, setOpenSettingsSection] = useState<SettingsSectionId | null>(null);
  const settingsHydratedRef = useRef(false);
  const settingsSaveTimerRef = useRef<number | null>(null);
  const settingsSavePromiseRef = useRef<Promise<void> | null>(null);
  const settingsSaveErrorRef = useRef<unknown>(null);
  const [connectionSwitching, setConnectionSwitching] = useState(false);
  const connectionSwitchingRef = useRef(false);
  const settingsGenerationRef = useRef(0);
  const [generalSettingsReady, setGeneralSettingsReady] = useState(false);
  const setDraft: typeof setDraftState = (update) => {
    if (!settingsHydratedRef.current || connectionSwitchingRef.current) return;
    const next = typeof update === "function" ? update(draftRef.current) : update;
    if (JSON.stringify(next.general) !== JSON.stringify(draftRef.current.general)) {
      setGeneralSettingsReady(true);
    }
    draftRef.current = next;
    pendingSettingsRef.current = next;
    setDraftState(next);
    if (settingsSaveTimerRef.current !== null) window.clearTimeout(settingsSaveTimerRef.current);
    settingsSaveTimerRef.current = window.setTimeout(() => {
      settingsSaveTimerRef.current = null;
      void flushSettingsQueue();
    }, 360);
  };
  const pendingSettingsRef = useRef<LoomSettings | null>(null);
  const lastSavedSettingsRef = useRef<LoomSettings>(DEFAULT_LOOM_SETTINGS);
  const settingsBaseUrlRef = useRef(snapshot.baseUrl);
  const shortcutsRef = useRef(shortcuts);
  const { checkApplicationUpdate, openApplicationLog, openRepository } = useApplicationLinks(appDiagnostics);

  const flushSettingsQueue = useCallback((): Promise<void> => {
    if (settingsSavePromiseRef.current) return settingsSavePromiseRef.current;
    const promise = (async () => {
    settingsSaveErrorRef.current = null;
    while (pendingSettingsRef.current) {
      const nextSettings = pendingSettingsRef.current;
      const baseUrl = settingsBaseUrlRef.current;
      const generation = settingsGenerationRef.current;
      pendingSettingsRef.current = null;
      try {
        const loomGeneralChanged =
          JSON.stringify(nextSettings.general) !== JSON.stringify(lastSavedSettingsRef.current.general);
        const loomCacheChanged =
          JSON.stringify(nextSettings.loom_cache) !== JSON.stringify(lastSavedSettingsRef.current.loom_cache);
        const otterCacheChanged =
          JSON.stringify(nextSettings.hook_cache) !== JSON.stringify(lastSavedSettingsRef.current.hook_cache);
        const saved = await saveLoomSettings(baseUrl, nextSettings);
        if (settingsGenerationRef.current !== generation) continue;
        lastSavedSettingsRef.current = saved;
        if (isDesktopTauriShell() && loomGeneralChanged) {
          try {
            await invoke("apply_loom_general_settings", {
              settings: { minimizeToTray: saved.general.minimize_to_tray },
            });
          } catch (error) {
            pushAppToast({
              level: "warning",
              text: error instanceof Error ? error.message : String(error),
            });
          }
        }
        if (isDesktopTauriShell() && loomCacheChanged) {
          try {
            setLoomCacheSnapshot(
              await invoke<LoomCacheSnapshotInfo>("apply_loom_cache_settings", {
                settings: loomCachePreferencesForRuntime(saved.loom_cache),
              }),
            );
          } catch (error) {
            pushAppToast({
              level: "warning",
              text: error instanceof Error ? error.message : String(error),
            });
          }
        }
        if (isDesktopTauriShell() && otterCacheChanged) {
          try {
            await invoke("wait_for_otter_cache_settings", {
              settings: otterCachePreferencesForRuntime(saved.hook_cache),
            });
          } catch (error) {
            pushAppToast({
              level: "warning",
              text: error instanceof Error ? error.message : String(error),
            });
          }
        }
      } catch (error) {
        if (settingsGenerationRef.current === generation) {
          settingsSaveErrorRef.current = error;
          pendingSettingsRef.current = null;
          if (settingsMountedRef.current) {
            const rollback = lastSavedSettingsRef.current;
            const rollbackShortcuts = Object.values(rollback.shortcuts);
            shortcutsRef.current = rollbackShortcuts;
            draftRef.current = rollback;
            setDraftState(rollback);
            setShortcuts(rollbackShortcuts);
          }
          pushAppToast({
            level: "error",
            text: error instanceof Error ? error.message : uiText("settings.toast.autosaveFailed"),
          });
          break;
        }
      }
    }
    })().finally(() => { settingsSavePromiseRef.current = null; });
    settingsSavePromiseRef.current = promise;
    return promise;
  }, []);

  useEffect(() => {
    if (generalSettingsReady) applyLoomGeneralSettings(draft.general);
  }, [draft.general.language, draft.general.minimize_to_tray, draft.general.theme, generalSettingsReady]);

  useEffect(() => {
    let cancelled = false;
    ++settingsGenerationRef.current;
    setSettingsLoading(true);
    setGeneralSettingsReady(false);
    settingsBaseUrlRef.current = snapshot.baseUrl;
    settingsHydratedRef.current = false;
    pendingSettingsRef.current = null;
    if (settingsSaveTimerRef.current !== null) {
      window.clearTimeout(settingsSaveTimerRef.current);
      settingsSaveTimerRef.current = null;
    }
    const loadSettings = async () => {
      try {
        const [loadedSettings, loadedShortcuts, loadedPaths] = await Promise.all([
          getLoomSettings(snapshot.baseUrl),
          getLoomShortcuts(snapshot.baseUrl),
          getLoomAppPaths(snapshot.baseUrl),
        ]);
        if (cancelled) return;
        const nextShortcuts = loadedShortcuts.length ? loadedShortcuts : Object.values(DEFAULT_LOOM_SETTINGS.shortcuts);
        const hydratedSettings = {
          ...hydrateSettings(loadedSettings, nextShortcuts),
          loom_cache: loomCacheSettingsForUi(loadedSettings.loom_cache),
          hook_cache: otterCacheSettingsForUi(loadedSettings.hook_cache),
        };
        lastSavedSettingsRef.current = hydratedSettings;
        shortcutsRef.current = nextShortcuts;
        draftRef.current = hydratedSettings;
        setDraftState(hydratedSettings);
        setShortcuts(nextShortcuts);
        setAppPaths(loadedPaths);
        settingsHydratedRef.current = true;
        setSettingsLoading(false);
        setGeneralSettingsReady(true);
      } catch (error) {
        if (cancelled) return;
        const fallbackShortcuts = Object.values(DEFAULT_LOOM_SETTINGS.shortcuts);
        lastSavedSettingsRef.current = DEFAULT_LOOM_SETTINGS;
        shortcutsRef.current = fallbackShortcuts;
        draftRef.current = DEFAULT_LOOM_SETTINGS;
        setDraftState(DEFAULT_LOOM_SETTINGS);
        setShortcuts(fallbackShortcuts);
        settingsHydratedRef.current = true;
        setSettingsLoading(false);
        pushAppToast({
          level: "error",
          text: error instanceof Error ? uiFormat("settings.toast.usingDefaultsWithError", { error: error.message }) : uiText("settings.toast.usingDefaults"),
        });
      }
    };
    void loadSettings();
    return () => {
      cancelled = true;
    };
  }, [snapshot.baseUrl]);

  const prepareConnectionSwitch = async () => {
    connectionSwitchingRef.current = true;
    setConnectionSwitching(true);
    setShortcutEditor(null);
    setQuickBindingEditor(null);
    if (settingsSaveTimerRef.current !== null) {
      window.clearTimeout(settingsSaveTimerRef.current);
      settingsSaveTimerRef.current = null;
    }
    await flushSettingsQueue();
    if (settingsSaveErrorRef.current) throw settingsSaveErrorRef.current;
  };

  useEffect(() => {
    if (!isDesktopTauriShell()) return;
    let cancelled = false;
    const loadDiagnostics = async () => {
      const results = await Promise.allSettled(
        (["loom", "hook"] as const).map((app) =>
          invoke<ApplicationDiagnosticsInfo>("resolve_application_diagnostics", { app }),
        ),
      );
      if (cancelled) return;
      setAppDiagnostics((current) => {
        const next = { ...current };
        results.forEach((result) => {
          if (result.status === "fulfilled") next[result.value.app] = result.value;
        });
        return next;
      });
    };
    void loadDiagnostics();
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (openSettingsSection !== "cache") return;
    if (activeSettingsApp === "loom") {
      void refreshLoomCache();
    } else {
      void refreshOtterCache();
    }
  }, [activeSettingsApp, openSettingsSection, refreshOtterCache, refreshLoomCache]);

  useEffect(() => {
    if (activeSettingsApp !== "loom" || openSettingsSection !== "art-store") return;
    let cancelled = false;
    void listPluginTrust(snapshot.baseUrl)
      .then((trustStore) => {
        if (!cancelled) setArtStoreTrustPolicy(trustStore.policy);
      })
      .catch((error) => {
        if (!cancelled) {
          pushAppToast({
            level: "error",
            text: error instanceof Error ? error.message : String(error),
          });
        }
      });
    return () => {
      cancelled = true;
    };
  }, [activeSettingsApp, openSettingsSection, snapshot.baseUrl]);

  useEffect(() => {
    settingsMountedRef.current = true;
    return () => {
      settingsMountedRef.current = false;
      if (settingsSaveTimerRef.current !== null) {
        window.clearTimeout(settingsSaveTimerRef.current);
      }
      void flushSettingsQueue();
    };
  }, [flushSettingsQueue]);

  const updateShortcutDraft = (id: string, label: string, keys: string) => {
    if (!settingsHydratedRef.current || connectionSwitchingRef.current) return;
    const existing = shortcutsRef.current.find((shortcut) => shortcut.id === id);
    const updated: LoomShortcutConfig = {
      id,
      label: existing?.label || label,
      keys,
      enabled: existing?.enabled ?? true,
    };
    const nextShortcuts = existing
      ? shortcutsRef.current.map((shortcut) => (shortcut.id === id ? updated : shortcut))
      : [...shortcutsRef.current, updated];
    shortcutsRef.current = nextShortcuts;
    setShortcuts(nextShortcuts);
    setDraft((current) => ({
      ...current,
      shortcuts: Object.fromEntries(nextShortcuts.map((shortcut) => [shortcut.id, shortcut])),
    }));
  };
  const {
    openShortcutGroups,
    shortcutEditor,
    setShortcutEditor,
    quickBindingEditor,
    setQuickBindingEditor,
    availableArtTools,
    availableArtToolById,
    resolveShortcutKeys,
    toggleShortcutGroup,
    openShortcutEditor,
    handleShortcutCapture,
    shortcutEditorConflict,
    applyShortcutEditor,
    openQuickBindingEditor,
    handleQuickBindingCapture,
    quickBindingConflict,
    applyQuickBindingEditor,
  } = useSettingsShortcutEditors({ draft, shortcuts, tools: snapshot.tools, updateShortcutDraft, setDraft });

  const updateNetworkDraft = (app: SettingsAppId, patch: Partial<LoomProxySettings>) => {
    setDraft((current) => ({
      ...current,
      network: {
        ...current.network,
        [app]: { ...current.network[app], ...patch },
      },
    }));
  };

  const updateOtterGeneralDraft = (patch: Partial<GeneralSettingsValue>) => {
    setDraft((current) => ({
      ...current,
      hook_general: {
        ...current.hook_general,
        ...(patch.theme === undefined ? {} : { theme: patch.theme }),
        ...(patch.closeToTray === undefined ? {} : { close_to_tray: patch.closeToTray }),
      },
    }));
  };

  const updateLoomCacheDraft = (patch: Partial<LoomCacheSettings>) => {
    setDraft((current) => ({
      ...current,
      loom_cache: { ...current.loom_cache, ...patch },
    }));
  };

  const updateMcpDraft = (patch: Partial<LoomMcpSettings>) => {
    setDraft((current) => ({
      ...current,
      mcp: { ...current.mcp, ...patch },
    }));
  };

  const updateArtStoreDraft = (patch: Partial<LoomArtStoreSettings>) => {
    setDraft((current) => ({
      ...current,
      art_store: { ...current.art_store, ...patch },
    }));
  };

  const updateArtStoreTrustPolicy = async (policy: LoomPluginTrustPolicy) => {
    const previous = artStoreTrustPolicy;
    setArtStoreTrustPolicy(policy);
    setArtStoreTrustPolicyBusy(true);
    try {
      const trustStore = await setPluginTrustPolicy(snapshot.baseUrl, policy);
      setArtStoreTrustPolicy(trustStore.policy);
      pushAppToast({ level: "info", text: uiText("settings.artStore.trustPolicyUpdated") });
    } catch (error) {
      setArtStoreTrustPolicy(previous);
      pushAppToast({
        level: "error",
        text: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setArtStoreTrustPolicyBusy(false);
    }
  };

  const updateOtterCacheDraft = (patch: Partial<OtterCacheSettings>) => {
    setDraft((current) => ({
      ...current,
      hook_cache: { ...current.hook_cache, ...patch },
    }));
  };

  const toggleMinimizeToTray = (enabled = !draft.general.minimize_to_tray) => {
    setDraft((current) => ({ ...current, general: { ...current.general, minimize_to_tray: enabled } }));
  };

  const toggleSettingsSection = (section: SettingsSectionId) => {
    setOpenSettingsSection((current) => (current === section ? null : section));
  };

  const selectSettingsApp = (app: SettingsAppId) => {
    if (connectionSwitchingRef.current) return;
    setActiveSettingsApp(app);
    setOpenSettingsSection(null);
  };

  return {
    settingsLoading,
    connectionSwitching,
    prepareConnectionSwitch,
    finishConnectionSwitch: () => {
      connectionSwitchingRef.current = false;
      setConnectionSwitching(false);
    },
    activeSettingsApp,
    appDiagnostics,
    appPaths,
    applyQuickBindingEditor,
    applyShortcutEditor,
    artStoreTrustPolicy,
    artStoreTrustPolicyBusy,
    availableArtToolById,
    availableArtTools,
    checkApplicationUpdate,
    clearOtterCache,
    clearLoomCache,
    draft,
    handleQuickBindingCapture,
    handleShortcutCapture,
    otterCacheBusyKind,
    otterCacheLoading,
    otterCacheSnapshot,
    loomCacheBusyKind,
    loomCacheLoading,
    loomCacheSnapshot,
    openApplicationLog,
    openQuickBindingEditor,
    openRepository,
    openSettingsSection,
    openShortcutEditor,
    openShortcutGroups,
    quickBindingConflict,
    quickBindingEditor,
    resolveShortcutKeys,
    selectSettingsApp,
    setDraft,
    setQuickBindingEditor,
    setShortcutEditor,
    shortcutEditor,
    shortcutEditorConflict,
    shortcuts,
    toggleMinimizeToTray,
    toggleSettingsSection,
    toggleShortcutGroup,
    updateArtStoreDraft,
    updateArtStoreTrustPolicy,
    updateOtterCacheDraft,
    updateOtterGeneralDraft,
    updateLoomCacheDraft,
    updateMcpDraft,
    updateNetworkDraft,
  };
}
