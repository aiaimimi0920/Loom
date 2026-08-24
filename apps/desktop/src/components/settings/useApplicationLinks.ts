// Owns validated external navigation and native log-location actions.
import { invoke } from "@tauri-apps/api/core";
import { normalizeHttpsExternalUrl } from "../../services/externalUrl";
import { pushAppToast } from "../feedback/AppFeedback";
import type { ApplicationDiagnosticsInfo, SettingsAppId } from "./settingsModel";

export function useApplicationLinks(
  diagnostics: Record<SettingsAppId, ApplicationDiagnosticsInfo>,
) {
  const reportError = (error: unknown) => {
    pushAppToast({ level: "error", text: error instanceof Error ? error.message : String(error) });
  };

  const openApplicationLog = async (app: SettingsAppId, target: "directory" | "file") => {
    try {
      await invoke("open_application_log_location", { app, target });
    } catch (error) {
      reportError(error);
    }
  };

  const openRepository = async (url: string) => {
    try {
      await invoke("open_external_url", { url: normalizeHttpsExternalUrl(url) });
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
