// Settings cache controls keep native commands and confirmations in Loom.
import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState, type RefObject } from "react";
import { uiFormat, uiText } from "../../i18n/index.ts";
import { isDesktopTauriShell } from "../../services/loomRuntimeTarget";
import { pushAppToast, requestAppConfirmation } from "../feedback/AppFeedback";
import { formatCacheBytes } from "./SettingsPanels";
import type {
  LoomCacheClearResult,
  LoomCacheSnapshotInfo,
  OtterCacheClearResult,
  OtterCacheSnapshotInfo,
} from "./settingsModel";

export function useSettingsCacheControls(settingsMountedRef: RefObject<boolean>) {
  const refreshTimers = useRef(new Set<number>());
  useEffect(
    () => () => {
      for (const timer of refreshTimers.current) window.clearTimeout(timer);
      refreshTimers.current.clear();
    },
    [],
  );
  const [loomCacheSnapshot, setLoomCacheSnapshot] = useState<LoomCacheSnapshotInfo | null>(null);

  const [loomCacheLoading, setLoomCacheLoading] = useState(false);

  const [loomCacheBusyKind, setLoomCacheBusyKind] = useState<string | null>(null);

  const [otterCacheSnapshot, setOtterCacheSnapshot] = useState<OtterCacheSnapshotInfo | null>(null);

  const [otterCacheLoading, setOtterCacheLoading] = useState(false);

  const [otterCacheBusyKind, setOtterCacheBusyKind] = useState<string | null>(null);

  const refreshLoomCache = useCallback(async () => {
    if (!isDesktopTauriShell()) return;
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

  const refreshOtterCache = useCallback(async () => {
    if (!isDesktopTauriShell()) return;
    setOtterCacheLoading(true);
    try {
      const next = await invoke<OtterCacheSnapshotInfo>("get_otter_cache_snapshot");
      if (settingsMountedRef.current) setOtterCacheSnapshot(next);
    } catch (error) {
      pushAppToast({
        level: "error",
        text: error instanceof Error ? error.message : String(error),
      });
    } finally {
      if (settingsMountedRef.current) setOtterCacheLoading(false);
    }
  }, []);

  const clearLoomCache = async (kind: "artRuntime" | "frameworkTemporary") => {
    const label = kind === "artRuntime"
      ? uiText("settings.cache.artRuntimeLabel")
      : uiText("settings.cache.frameworkTempLabel");
    const accepted = await requestAppConfirmation({
      title: uiFormat("settings.cache.confirmClearTitle", { label }),
      message:
        kind === "artRuntime"
          ? uiText("settings.cache.confirmClearArtRuntimeBody")
          : uiText("settings.cache.confirmClearFrameworkTempBody"),
      confirmLabel: uiText("settings.cache.clearAction"),
      tone: "warning",
    });
    if (!accepted || !settingsMountedRef.current) return;
    setLoomCacheBusyKind(kind);
    try {
      const result = await invoke<LoomCacheClearResult>("clear_loom_cache", { kind });
      setLoomCacheSnapshot(result.snapshot);
      pushAppToast({
        level: "info",
        text: uiFormat("settings.cache.clearedWithFreed", {
          label,
          size: formatCacheBytes(result.freedBytes),
        }),
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

  const clearOtterCache = async (kind: "recycleBin" | "temporary" | "referenceLibrary") => {
    const labels = {
      recycleBin: uiText("settings.cache.recycleBinLabel"),
      temporary: uiText("settings.cache.temporaryLabel"),
      referenceLibrary: uiText("settings.cache.referenceLibraryLabel"),
    } as const;
    const accepted = await requestAppConfirmation({
      title: uiFormat("settings.cache.confirmClearTitle", { label: labels[kind] }),
      message:
        kind === "referenceLibrary"
          ? uiText("settings.cache.confirmClearReferenceBody")
          : kind === "recycleBin"
            ? uiText("settings.cache.confirmClearRecycleBinBody")
            : uiText("settings.cache.confirmClearTemporaryBody"),
      confirmLabel: uiText("settings.cache.clearAction"),
      tone: kind === "recycleBin" ? "danger" : "warning",
    });
    if (!accepted || !settingsMountedRef.current) return;
    setOtterCacheBusyKind(kind);
    try {
      const result = await invoke<OtterCacheClearResult>("clear_otter_cache", { kind });
      setOtterCacheSnapshot(result.snapshot);
      pushAppToast({
        level: "info",
        text:
          kind === "temporary"
            ? uiFormat("settings.cache.clearedWithFreed", {
                label: labels.temporary,
                size: formatCacheBytes(result.freedBytes),
              })
            : uiFormat("settings.cache.clearedSimple", { label: labels[kind] }),
      });
      if (kind !== "temporary" && settingsMountedRef.current) {
        const timer = window.setTimeout(() => {
          refreshTimers.current.delete(timer);
          void refreshOtterCache();
        }, 500);
        refreshTimers.current.add(timer);
      }
    } catch (error) {
      pushAppToast({
        level: "error",
        text: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setOtterCacheBusyKind(null);
    }
  };
  return {
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
  };
}
