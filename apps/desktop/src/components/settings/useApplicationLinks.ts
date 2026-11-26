// Owns validated external navigation and native log-location actions.
import { invoke } from "@tauri-apps/api/core";
import { normalizeHttpsExternalUrl } from "../../services/externalUrl";
import { isDesktopTauriShell } from "../../services/loomRuntimeTarget";
import { pushAppToast } from "../feedback/AppFeedback";
import type { ApplicationDiagnosticsInfo, SettingsAppId } from "./settingsModel";

export function useApplicationLinks(
  diagnostics: Record<SettingsAppId, ApplicationDiagnosticsInfo>,
) {
  const reportError = (error: unknown) => {
    pushAppToast({ level: "error", text: error instanceof Error ? error.message : String(error) });
  };

  const openApplicationLog = async (app: SettingsAppId, target: "directory" | "file") => {
    if (!isDesktopTauriShell()) {
      pushAppToast({ level: "info", text: "请在宿主机本地查看服务端日志目录。" });
      return;
    }
    try {
      await invoke("open_application_log_location", { app, target });
    } catch (error) {
      reportError(error);
    }
  };

  const openRepository = async (url: string) => {
    try {
      const normalized = normalizeHttpsExternalUrl(url);
      if (!isDesktopTauriShell()) {
        window.open(normalized, "_blank", "noopener,noreferrer");
        return;
      }
      await invoke("open_external_url", { url: normalized });
    } catch (error) {
      reportError(error);
    }
  };

  const checkApplicationUpdate = (app: SettingsAppId) => {
    const repositoryUrl = diagnostics[app].repositoryUrl?.replace(/\/$/, "");
    if (!repositoryUrl) {
      pushAppToast({ level: "warning", text: `${diagnostics[app].appName} 暂无更新地址` });
      return;
    }
    void openRepository(`${repositoryUrl}/releases/latest`);
  };

  return { checkApplicationUpdate, openApplicationLog, openRepository };
}
