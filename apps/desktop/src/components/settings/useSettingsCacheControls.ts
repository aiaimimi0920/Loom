// Settings cache controls keep native commands and confirmations in Loom.
import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState, type RefObject } from "react";
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
    const label = kind === "artRuntime" ? "Art 运行缓存" : "框架临时文件";
    const accepted = await requestAppConfirmation({
      title: `清空${label}`,
      message:
        kind === "artRuntime"
          ? "将删除 Art 生成的可重建运行缓存，不会卸载 Art 或删除工作流。"
          : "将删除框架执行产生的临时文件。请先等待正在运行的 Art 完成。",
      confirmLabel: "清空",
      tone: "warning",
    });
    if (!accepted || !settingsMountedRef.current) return;
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

  const clearOtterCache = async (kind: "recycleBin" | "temporary" | "referenceLibrary") => {
    const labels = {
      recycleBin: "回收站",
      temporary: "临时缓存",
      referenceLibrary: "参考图",
    } as const;
    const accepted = await requestAppConfirmation({
      title: `清空${labels[kind]}`,
      message:
        kind === "referenceLibrary"
          ? "将移除 Otter 参考列表中的全部记录，桌面贴图不会被删除。"
          : kind === "recycleBin"
            ? "回收站中的贴图记录将被永久移除。"
            : "将移除 Otter 的临时中转文件，后续需要时会自动重新生成。",
      confirmLabel: "清空",
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
            ? `已清空临时缓存，释放 ${formatCacheBytes(result.freedBytes)}`
            : `已清空${labels[kind]}`,
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
