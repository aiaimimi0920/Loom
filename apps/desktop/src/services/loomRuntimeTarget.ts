// Dual-target runtime adapter (`tauri` desktop shell vs `/ui/` browser web console)
// plus desktop connection profile IPC (`local` bundled core vs `remote` LAN server).
import { invoke, isTauri } from "@tauri-apps/api/core";
import { DEFAULT_LOOM_DAEMON_URL } from "./loomApi/defaults.ts";

export type LoomConnectionMode = "local" | "remote";

export interface LoomConnectionStatus {
  mode: LoomConnectionMode;
  activeUrl: string;
  remoteUrl: string | null;
  serverLabel: string | null;
  otterBridgeUrl: string | null;
  hasRemoteToken: boolean;
}

export interface LoomConnectionUpdate {
  mode: LoomConnectionMode;
  remoteUrl?: string | null;
  serverLabel?: string | null;
  otterBridgeUrl?: string | null;
  remoteToken?: string | null;
  clearRemoteToken?: boolean;
}

export function isDesktopTauriShell(): boolean {
  try {
    return typeof window !== "undefined" && isTauri();
  } catch {
    return false;
  }
}

export function resolveBrowserDaemonBaseUrl(): string {
  if (typeof window !== "undefined" && window.location && /^https?:$/i.test(window.location.protocol)) {
    return window.location.origin;
  }
  return DEFAULT_LOOM_DAEMON_URL;
}

export async function getLoomConnectionProfile(): Promise<LoomConnectionStatus> {
  if (!isDesktopTauriShell()) {
    const activeUrl = resolveBrowserDaemonBaseUrl();
    return {
      mode: "remote",
      activeUrl,
      remoteUrl: activeUrl,
      serverLabel: "Web 管理控制台",
      otterBridgeUrl: null,
      hasRemoteToken: true,
    };
  }
  return await invoke<LoomConnectionStatus>("get_loom_connection_profile");
}

export async function saveLoomConnectionProfile(
  update: LoomConnectionUpdate,
): Promise<LoomConnectionStatus> {
  if (!isDesktopTauriShell()) {
    throw new Error("浏览器 Web 管理端直接连接当前服务端，无需切换桌面本地进程模式。");
  }
  return await invoke<LoomConnectionStatus>("save_loom_connection_profile", { update });
}
