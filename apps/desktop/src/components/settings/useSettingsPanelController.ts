// Owns settings hydration, autosave, diagnostics, cache, and shortcut state.
import {
  DEFAULT_LOOM_SETTINGS,
  getLoomAppPaths,
  getLoomSettings,
  getLoomShortcuts,
  HookCacheSettings,
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
import { pushAppToast, requestAppConfirmation } from "../feedback/AppFeedback";
import { useApplicationLinks } from "./useApplicationLinks";
import {
  ApplicationDiagnosticsInfo,
  FALLBACK_APPLICATION_DIAGNOSTICS,
  HOOK_SHORTCUT_GROUPS,
  HookCacheClearResult,
  HookCacheSnapshotInfo,
  HookShortcutContext,
  HookShortcutDisplayItem,
  LoomCacheClearResult,
  LoomCacheSnapshotInfo,
  QuickBindingEditorState,
  SettingsAppId,
  SettingsSectionId,
  shortcutContextsOverlap,
  ShortcutEditorState,
  shortcutFromKeyboardEvent,
  ShortcutSlot,
  shortcutSlotCount,
  ShortcutSlots,
  splitShortcutAlternatives,
} from "./settingsModel";
import {
  formatCacheBytes,
  GeneralSettingsValue,
  hookCachePreferencesForRuntime,
  hookCacheSettingsForUi,
  loomCachePreferencesForRuntime,
  loomCacheSettingsForUi,
} from "./SettingsPanels";
import { invoke } from "@tauri-apps/api/core";
import {
  type KeyboardEvent,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

export function useSettingsPanelController({ snapshot }: { snapshot: LoomSnapshot }) {
  const [draft, setDraft] = useState<LoomSettings>(DEFAULT_LOOM_SETTINGS);
  const [shortcuts, setShortcuts] = useState<LoomShortcutConfig[]>(
    Object.values(DEFAULT_LOOM_SETTINGS.shortcuts),
  );
  const [appPaths, setAppPaths] = useState<LoomAppPaths | null>(null);
  const [appDiagnostics, setAppDiagnostics] = useState<Record<SettingsAppId, ApplicationDiagnosticsInfo>>(
    FALLBACK_APPLICATION_DIAGNOSTICS,
  );
  const [loomCacheSnapshot, setLoomCacheSnapshot] = useState<LoomCacheSnapshotInfo | null>(null);
  const [loomCacheLoading, setLoomCacheLoading] = useState(false);
  const [loomCacheBusyKind, setLoomCacheBusyKind] = useState<string | null>(null);
  const [hookCacheSnapshot, setHookCacheSnapshot] = useState<HookCacheSnapshotInfo | null>(null);
  const [hookCacheLoading, setHookCacheLoading] = useState(false);
  const [hookCacheBusyKind, setHookCacheBusyKind] = useState<string | null>(null);
  const [artStoreTrustPolicy, setArtStoreTrustPolicy] = useState<LoomPluginTrustPolicy>("allow_unsigned");
  const [artStoreTrustPolicyBusy, setArtStoreTrustPolicyBusy] = useState(false);
  const [activeSettingsApp, setActiveSettingsApp] = useState<SettingsAppId>("loom");
  const [openSettingsSection, setOpenSettingsSection] = useState<SettingsSectionId | null>(null);
  const [openShortcutGroups, setOpenShortcutGroups] = useState<Set<string>>(() => new Set());
  const [shortcutEditor, setShortcutEditor] = useState<ShortcutEditorState | null>(null);
  const [quickBindingEditor, setQuickBindingEditor] = useState<QuickBindingEditorState | null>(null);
  const settingsHydratedRef = useRef(false);
  const suppressNextSettingsSaveRef = useRef(false);
  const settingsSaveTimerRef = useRef<number | null>(null);
  const settingsSaveActiveRef = useRef(false);
  const settingsMountedRef = useRef(true);
  const pendingSettingsRef = useRef<LoomSettings | null>(null);
  const lastSavedSettingsRef = useRef<LoomSettings>(DEFAULT_LOOM_SETTINGS);
  const settingsBaseUrlRef = useRef(snapshot.baseUrl);
  const shortcutsRef = useRef(shortcuts);
  const availableArtTools = useMemo(
    () => snapshot.tools
      .filter((tool) => tool.enabled !== false)
      .sort((left, right) => (left.name || left.id).localeCompare(right.name || right.id, "zh-CN")),
    [snapshot.tools],
  );
  const availableArtToolById = useMemo(
    () => new Map(availableArtTools.map((tool) => [tool.id, tool])),
    [availableArtTools],
  );
  const { checkApplicationUpdate, openApplicationLog, openRepository } = useApplicationLinks(appDiagnostics);

  const flushSettingsQueue = useCallback(async () => {
    if (settingsSaveActiveRef.current) return;
    settingsSaveActiveRef.current = true;
    while (pendingSettingsRef.current) {
      const nextSettings = pendingSettingsRef.current;
      const baseUrl = settingsBaseUrlRef.current;
      pendingSettingsRef.current = null;
      try {
        const loomGeneralChanged = JSON.stringify(nextSettings.general)
          !== JSON.stringify(lastSavedSettingsRef.current.general);
        const loomCacheChanged = JSON.stringify(nextSettings.loom_cache)
          !== JSON.stringify(lastSavedSettingsRef.current.loom_cache);
        const hookCacheChanged = JSON.stringify(nextSettings.hook_cache)
          !== JSON.stringify(lastSavedSettingsRef.current.hook_cache);
        const saved = await saveLoomSettings(baseUrl, nextSettings);
        if (settingsBaseUrlRef.current === baseUrl) {
          lastSavedSettingsRef.current = saved;
        }
        if (loomGeneralChanged) {
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
        if (loomCacheChanged) {
          try {
            setLoomCacheSnapshot(await invoke<LoomCacheSnapshotInfo>("apply_loom_cache_settings", {
              settings: loomCachePreferencesForRuntime(saved.loom_cache),
            }));
          } catch (error) {
            pushAppToast({
              level: "warning",
              text: error instanceof Error ? error.message : String(error),
            });
          }
        }
        if (hookCacheChanged) {
          try {
            await invoke("wait_for_hook_cache_settings", {
              settings: hookCachePreferencesForRuntime(saved.hook_cache),
            });
          } catch (error) {
            pushAppToast({
              level: "warning",
              text: error instanceof Error ? error.message : String(error),
            });
          }
        }
      } catch (error) {
        if (settingsBaseUrlRef.current === baseUrl) {
          pendingSettingsRef.current = null;
          if (settingsMountedRef.current) {
            suppressNextSettingsSaveRef.current = true;
            const rollback = lastSavedSettingsRef.current;
            const rollbackShortcuts = Object.values(rollback.shortcuts);
            shortcutsRef.current = rollbackShortcuts;
            setDraft(rollback);
            setShortcuts(rollbackShortcuts);
          }
          pushAppToast({
            level: "error",
            text: error instanceof Error ? error.message : "设置自动保存失败",
          });
        }
        break;
      }
    }
    settingsSaveActiveRef.current = false;
  }, []);

  const refreshLoomCache = useCallback(async () => {
    setLoomCacheLoading(true);
    try {
      const next = await invoke<LoomCacheSnapshotInfo>("get_loom_cache_snapshot");
      if (settingsMountedRef.current) setLoomCacheSnapshot(next);
    } catch (error) {
      pushAppToast({
        level: "error",
        text: error instanceof Error ? error.message : String(error),
      });
    } finally {
      if (settingsMountedRef.current) setLoomCacheLoading(false);
    }
  }, []);

  const refreshHookCache = useCallback(async () => {
    setHookCacheLoading(true);
    try {
      const next = await invoke<HookCacheSnapshotInfo>("get_hook_cache_snapshot");
      if (settingsMountedRef.current) setHookCacheSnapshot(next);
    } catch (error) {
      pushAppToast({
        level: "error",
        text: error instanceof Error ? error.message : String(error),
      });
    } finally {
      if (settingsMountedRef.current) setHookCacheLoading(false);
    }
  }, []);

  useEffect(() => {
    applyLoomGeneralSettings(draft.general);
  }, [draft.general.language, draft.general.minimize_to_tray, draft.general.theme]);

  useEffect(() => {
    let cancelled = false;
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
        const nextShortcuts = loadedShortcuts.length
          ? loadedShortcuts
          : Object.values(DEFAULT_LOOM_SETTINGS.shortcuts);
        const hydratedSettings = {
          ...loadedSettings,
          general: {
            ...DEFAULT_LOOM_SETTINGS.general,
            ...loadedSettings.general,
          },
          hook_general: {
            ...DEFAULT_LOOM_SETTINGS.hook_general,
            ...loadedSettings.hook_general,
          },
          system: {
            ...DEFAULT_LOOM_SETTINGS.system,
            ...loadedSettings.system,
          },
          network: {
            loom: {
              ...DEFAULT_LOOM_SETTINGS.network.loom,
              ...loadedSettings.network?.loom,
            },
            hook: {
              ...DEFAULT_LOOM_SETTINGS.network.hook,
              ...loadedSettings.network?.hook,
            },
          },
          mcp: {
            ...DEFAULT_LOOM_SETTINGS.mcp,
            ...loadedSettings.mcp,
          },
          art_store: {
            ...DEFAULT_LOOM_SETTINGS.art_store,
            ...loadedSettings.art_store,
          },
          loom_cache: loomCacheSettingsForUi(loadedSettings.loom_cache),
          hook_cache: hookCacheSettingsForUi(loadedSettings.hook_cache),
          shortcuts: Object.fromEntries(nextShortcuts.map((shortcut) => [shortcut.id, shortcut])),
        };
        lastSavedSettingsRef.current = hydratedSettings;
        shortcutsRef.current = nextShortcuts;
        suppressNextSettingsSaveRef.current = true;
        setDraft(hydratedSettings);
        setShortcuts(nextShortcuts);
        setAppPaths(loadedPaths);
        settingsHydratedRef.current = true;
      } catch (error) {
        if (cancelled) return;
        const fallbackShortcuts = Object.values(DEFAULT_LOOM_SETTINGS.shortcuts);
        lastSavedSettingsRef.current = DEFAULT_LOOM_SETTINGS;
        shortcutsRef.current = fallbackShortcuts;
        suppressNextSettingsSaveRef.current = true;
        setDraft(DEFAULT_LOOM_SETTINGS);
        setShortcuts(fallbackShortcuts);
        settingsHydratedRef.current = true;
        pushAppToast({
          level: "error",
          text: error instanceof Error
            ? `使用 Loom 默认设置：${error.message}`
            : "使用 Loom 默认设置。",
        });
      }
    };
    void loadSettings();
    return () => {
      cancelled = true;
    };
  }, [snapshot.baseUrl]);

  useEffect(() => {
    let cancelled = false;
    const loadDiagnostics = async () => {
      const results = await Promise.allSettled(
        (["loom", "hook"] as const).map((app) => invoke<ApplicationDiagnosticsInfo>(
          "resolve_application_diagnostics",
          { app },
        )),
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
      void refreshHookCache();
    }
  }, [activeSettingsApp, openSettingsSection, refreshHookCache, refreshLoomCache]);

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
    if (!settingsHydratedRef.current) return;
    if (suppressNextSettingsSaveRef.current) {
      suppressNextSettingsSaveRef.current = false;
      return;
    }
    pendingSettingsRef.current = draft;
    if (settingsSaveTimerRef.current !== null) {
      window.clearTimeout(settingsSaveTimerRef.current);
    }
    settingsSaveTimerRef.current = window.setTimeout(() => {
      settingsSaveTimerRef.current = null;
      void flushSettingsQueue();
    }, 360);
    return () => {
      if (settingsSaveTimerRef.current !== null) {
        window.clearTimeout(settingsSaveTimerRef.current);
        settingsSaveTimerRef.current = null;
      }
    };
  }, [draft, flushSettingsQueue]);

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
    const existing = shortcutsRef.current.find((shortcut) => shortcut.id === id);
    const updated: LoomShortcutConfig = {
      id,
      label: existing?.label || label,
      keys,
      enabled: existing?.enabled ?? true,
    };
    const nextShortcuts = existing
      ? shortcutsRef.current.map((shortcut) => shortcut.id === id ? updated : shortcut)
      : [...shortcutsRef.current, updated];
    shortcutsRef.current = nextShortcuts;
    setShortcuts(nextShortcuts);
    setDraft((current) => ({
      ...current,
      shortcuts: Object.fromEntries(nextShortcuts.map((shortcut) => [shortcut.id, shortcut])),
    }));
  };

  const updateNetworkDraft = (app: SettingsAppId, patch: Partial<LoomProxySettings>) => {
    setDraft((current) => ({
      ...current,
      network: {
        ...current.network,
        [app]: { ...current.network[app], ...patch },
      },
    }));
  };

  const updateHookGeneralDraft = (patch: Partial<GeneralSettingsValue>) => {
    setDraft((current) => ({
      ...current,
      hook_general: {
        ...current.hook_general,
        ...(patch.language === undefined ? {} : { language: patch.language }),
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
      pushAppToast({ level: "info", text: "Art 安装策略已更新" });
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

  const updateHookCacheDraft = (patch: Partial<HookCacheSettings>) => {
    setDraft((current) => ({
      ...current,
      hook_cache: { ...current.hook_cache, ...patch },
    }));
  };

  const clearLoomCache = async (kind: "artRuntime" | "frameworkTemporary") => {
    const label = kind === "artRuntime" ? "Art 运行缓存" : "框架临时文件";
    const accepted = await requestAppConfirmation({
      title: `清空${label}`,
      message: kind === "artRuntime"
        ? "将删除 Art 生成的可重建运行缓存，不会卸载 Art 或删除工作流。"
        : "将删除框架执行产生的临时文件。请先等待正在运行的 Art 完成。",
      confirmLabel: "清空",
      tone: "warning",
    });
    if (!accepted) return;
    setLoomCacheBusyKind(kind);
    try {
      const result = await invoke<LoomCacheClearResult>("clear_loom_cache", { kind });
      setLoomCacheSnapshot(result.snapshot);
      pushAppToast({
        level: "info",
        text: `已清空${label}，释放 ${formatCacheBytes(result.freedBytes)}`,
      });
    } catch (error) {
      pushAppToast({
        level: "error",
        text: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setLoomCacheBusyKind(null);
    }
  };

  const clearHookCache = async (kind: "recycleBin" | "temporary" | "referenceLibrary") => {
    const labels = {
      recycleBin: "回收站",
      temporary: "临时缓存",
      referenceLibrary: "参考图",
    } as const;
    const accepted = await requestAppConfirmation({
      title: `清空${labels[kind]}`,
      message: kind === "referenceLibrary"
        ? "将移除 Hook 参考列表中的全部记录，桌面贴图不会被删除。"
        : kind === "recycleBin"
          ? "回收站中的贴图记录将被永久移除。"
          : "将移除 Hook 的临时中转文件，后续需要时会自动重新生成。",
      confirmLabel: "清空",
      tone: kind === "recycleBin" ? "danger" : "warning",
    });
    if (!accepted) return;
    setHookCacheBusyKind(kind);
    try {
      const result = await invoke<HookCacheClearResult>("clear_hook_cache", { kind });
      setHookCacheSnapshot(result.snapshot);
      pushAppToast({
        level: "info",
        text: kind === "temporary"
          ? `已清空临时缓存，释放 ${formatCacheBytes(result.freedBytes)}`
          : `已清空${labels[kind]}`,
      });
      if (kind !== "temporary") {
        window.setTimeout(() => void refreshHookCache(), 500);
      }
    } catch (error) {
      pushAppToast({
        level: "error",
        text: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setHookCacheBusyKind(null);
    }
  };

  const toggleMinimizeToTray = (enabled = !draft.general.minimize_to_tray) => {
    setDraft((current) => ({ ...current, general: { ...current.general, minimize_to_tray: enabled } }));
  };

  const toggleSettingsSection = (section: SettingsSectionId) => {
    setOpenSettingsSection((current) => current === section ? null : section);
  };

  const selectSettingsApp = (app: SettingsAppId) => {
    setActiveSettingsApp(app);
    setOpenSettingsSection(null);
  };

  const resolveShortcutKeys = (item: HookShortcutDisplayItem) => {
    if (!item.sourceId) return item.keys;
    const configured = shortcuts.find((shortcut) => shortcut.id === item.sourceId)?.keys.trim();
    const normalizedConfigured = configured?.replace(/\s+/g, "").toLocaleLowerCase();
    const contextualDefaultOverride = (
      (item.sourceId === "cancel" && normalizedConfigured === "escape")
      || (item.sourceId === "delete_unit" && normalizedConfigured === "delete/backspace")
    );
    if (contextualDefaultOverride) return item.keys;
    return configured ? splitShortcutAlternatives(configured) : item.keys;
  };

  const toggleShortcutGroup = (groupId: string) => {
    setOpenShortcutGroups((current) => {
      const next = new Set(current);
      if (next.has(groupId)) next.delete(groupId);
      else next.add(groupId);
      return next;
    });
  };

  const openShortcutEditor = (item: HookShortcutDisplayItem) => {
    if (!item.sourceId) return;
    const keys = resolveShortcutKeys(item);
    setShortcutEditor({
      item,
      keys: [keys[0] || "", keys[1] || "", keys[2] || ""],
      activeSlot: 0,
      slotCount: shortcutSlotCount(keys),
    });
  };

  const handleShortcutCapture = (event: KeyboardEvent<HTMLButtonElement>, slot: ShortcutSlot) => {
    event.preventDefault();
    event.stopPropagation();
    if (event.key === "Escape" && !event.ctrlKey && !event.altKey && !event.shiftKey && !event.metaKey) {
      setShortcutEditor(null);
      return;
    }
    const keys = shortcutFromKeyboardEvent(event.nativeEvent);
    if (!keys) return;
    setShortcutEditor((current) => current ? {
      ...current,
      activeSlot: slot,
      keys: current.keys.map((value, index) => index === slot ? keys : value) as ShortcutSlots,
    } : current);
  };

  const shortcutConflictMessage = (
    candidateKeys: ShortcutSlots,
    candidateContexts: readonly HookShortcutContext[],
    excludeSourceId?: string,
    excludeQuickBindingId?: string,
    candidateConflictFamily?: string,
  ) => {
    const normalized = candidateKeys.map((keys) => keys.trim().toLocaleLowerCase()).filter(Boolean);
    if (new Set(normalized).size !== normalized.length) return "同一事件的快捷键不能重复";
    const shortcutConflict = HOOK_SHORTCUT_GROUPS
      .flatMap((group) => group.items)
      .find((item) => (
        item.sourceId
        && item.sourceId !== excludeSourceId
        && (!candidateConflictFamily || item.conflictFamily !== candidateConflictFamily)
        && shortcutContextsOverlap(candidateContexts, item.contexts)
        && resolveShortcutKeys(item).some((keys) => normalized.includes(keys.toLocaleLowerCase()))
      ));
    if (shortcutConflict) return `与“${shortcutConflict.label}”冲突`;
    const quickBindingConflict = draft.quick_bindings.find((binding) => (
      binding.id !== excludeQuickBindingId
      && availableArtToolById.has(binding.art)
      && candidateContexts.includes("unit-selected")
      && splitShortcutAlternatives(binding.key).some((keys) => normalized.includes(keys.toLocaleLowerCase()))
    ));
    if (quickBindingConflict) {
      const art = availableArtToolById.get(quickBindingConflict.art);
      return `与“${art?.name || quickBindingConflict.art}”冲突`;
    }
    return null;
  };

  const shortcutEditorConflict = shortcutEditor
    ? shortcutConflictMessage(
      shortcutEditor.keys,
      HOOK_SHORTCUT_GROUPS
        .flatMap((group) => group.items)
        .filter((item) => item.sourceId === shortcutEditor.item.sourceId)
        .flatMap((item) => item.contexts)
        .filter((context, index, contexts) => contexts.indexOf(context) === index),
      shortcutEditor.item.sourceId,
      undefined,
      shortcutEditor.item.conflictFamily,
    )
    : null;

  const applyShortcutEditor = () => {
    if (!shortcutEditor?.item.sourceId || !shortcutEditor.keys.some((keys) => keys.trim()) || shortcutEditorConflict) return;
    const keys = shortcutEditor.keys.map((value) => value.trim()).filter(Boolean).join(" / ");
    updateShortcutDraft(
      shortcutEditor.item.sourceId,
      shortcutEditor.item.label,
      keys,
    );
    setShortcutEditor(null);
    pushAppToast({ level: "info", text: `${shortcutEditor.item.label}快捷键已更新` });
  };

  const openQuickBindingEditor = (binding?: LoomSettings["quick_bindings"][number]) => {
    const keys = binding ? splitShortcutAlternatives(binding.key) : [];
    setQuickBindingEditor({
      id: binding?.id || `${Date.now()}`,
      art: binding?.art || availableArtTools[0]?.id || "",
      keys: [keys[0] || "", keys[1] || "", keys[2] || ""],
      activeSlot: 0,
      slotCount: shortcutSlotCount(keys),
    });
  };

  const handleQuickBindingCapture = (event: KeyboardEvent<HTMLButtonElement>, slot: ShortcutSlot) => {
    event.preventDefault();
    event.stopPropagation();
    if (event.key === "Escape" && !event.ctrlKey && !event.altKey && !event.shiftKey && !event.metaKey) {
      setQuickBindingEditor(null);
      return;
    }
    const keys = shortcutFromKeyboardEvent(event.nativeEvent);
    if (!keys) return;
    setQuickBindingEditor((current) => current ? {
      ...current,
      activeSlot: slot,
      keys: current.keys.map((value, index) => index === slot ? keys : value) as ShortcutSlots,
    } : current);
  };

  const quickBindingConflict = quickBindingEditor
    ? shortcutConflictMessage(quickBindingEditor.keys, ["unit-selected"], undefined, quickBindingEditor.id)
    : null;

  const applyQuickBindingEditor = () => {
    if (!quickBindingEditor?.art || !quickBindingEditor.keys.some((keys) => keys.trim()) || quickBindingConflict) return;
    const nextBinding = {
      id: quickBindingEditor.id,
      art: quickBindingEditor.art,
      key: quickBindingEditor.keys.map((value) => value.trim()).filter(Boolean).join(" / "),
    };
    setDraft((current) => ({
      ...current,
      quick_bindings: current.quick_bindings.some((binding) => binding.id === nextBinding.id)
        ? current.quick_bindings.map((binding) => binding.id === nextBinding.id ? nextBinding : binding)
        : [...current.quick_bindings, nextBinding],
    }));
    setQuickBindingEditor(null);
    pushAppToast({ level: "info", text: "Art 快捷键已更新" });
  };

  return { activeSettingsApp, appDiagnostics, appPaths, applyQuickBindingEditor, applyShortcutEditor, artStoreTrustPolicy, artStoreTrustPolicyBusy, availableArtToolById, availableArtTools, checkApplicationUpdate, clearHookCache, clearLoomCache, draft, handleQuickBindingCapture, handleShortcutCapture, hookCacheBusyKind, hookCacheLoading, hookCacheSnapshot, loomCacheBusyKind, loomCacheLoading, loomCacheSnapshot, openApplicationLog, openQuickBindingEditor, openRepository, openSettingsSection, openShortcutEditor, openShortcutGroups, quickBindingConflict, quickBindingEditor, resolveShortcutKeys, selectSettingsApp, setDraft, setQuickBindingEditor, setShortcutEditor, shortcutEditor, shortcutEditorConflict, shortcuts, toggleMinimizeToTray, toggleSettingsSection, toggleShortcutGroup, updateArtStoreDraft, updateArtStoreTrustPolicy, updateHookCacheDraft, updateHookGeneralDraft, updateLoomCacheDraft, updateMcpDraft, updateNetworkDraft };
}
