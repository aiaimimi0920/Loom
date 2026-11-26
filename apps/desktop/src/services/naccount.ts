import { uiText } from "../i18n/index.ts";
import { postJson } from "./loomApi/transport.ts";

export type NAccountView =
  | { status: "signed_out"; configured: boolean; remoteRevoked?: boolean }
  | { status: "pending"; requestId: string; authorizationUrl: string }
  | { status: "signed_in"; principal: string; name: string };

export function parseNAccount(value: unknown): NAccountView {
  if (!value || typeof value !== "object") throw new Error("naccount_response_invalid");
  const v = value as Record<string, unknown>;
  if (v.status === "signed_out" && typeof v.configured === "boolean")
    return { status: v.status, configured: v.configured, remoteRevoked: v.remoteRevoked === false ? false : undefined };
  if (v.status === "signed_in" && typeof v.principal === "string" && v.principal.length > 0
      && v.principal.length <= 200 && typeof v.name === "string" && v.name.length <= 400)
    return { status: v.status, principal: v.principal, name: v.name };
  if (v.status === "pending" && typeof v.requestId === "string" && /^[A-Za-z0-9_-]{43}$/.test(v.requestId)
      && typeof v.authorizationUrl === "string" && v.authorizationUrl.length <= 4096) {
    const url = new URL(v.authorizationUrl);
    if (url.origin === "https://naccount-auth.aiaimimi.com" && url.pathname === "/oauth2/v1/authorize"
        && !url.username && !url.password && !url.hash && url.searchParams.getAll("state").length === 1
        && url.searchParams.get("state") === v.requestId)
      return { status: v.status, requestId: v.requestId, authorizationUrl: url.href };
  }
  throw new Error("naccount_response_invalid");
}

export async function naccountRequest(baseUrl: string, action: "status" | "start" | "refresh" | "logout") {
  return parseNAccount(await postJson<unknown>(baseUrl, `/v1/naccount/${action}`, {}));
}

const CLOUD_ERROR_MESSAGE_IDS: ReadonlyArray<readonly [string, string]> = [
  ["naccount_not_configured", "settings.cloud.error.naccountNotConfigured"],
  ["naccount_callback_port_required", "settings.cloud.error.callbackPortRequired"],
  ["naccount_login_required", "settings.cloud.error.loginRequired"],
  ["naccount_signed_out", "settings.cloud.error.signedOut"],
  ["naccount_logout_required", "settings.cloud.error.logoutRequired"],
  ["naccount_network_unavailable", "settings.cloud.error.naccountUnavailable"],
  ["asset_library_native_client_not_configured", "settings.cloud.error.assetClientNotConfigured"],
  ["asset_library_distribution_unavailable", "settings.cloud.error.distributionUnavailable"],
  ["asset_library_unavailable", "settings.cloud.error.assetUnavailable"],
  ["asset_library_response_invalid", "settings.cloud.error.responseInvalid"],
  ["asset_library_download_invalid", "settings.cloud.error.downloadInvalid"],
  ["asset_library_download_interrupted", "settings.cloud.error.downloadInterrupted"],
  ["asset_library_download_cancelled", "settings.cloud.error.downloadCancelled"],
  ["asset_library_task_not_found", "settings.cloud.error.taskNotFound"],
  ["asset_library_engine_busy", "settings.cloud.error.engineBusy"],
  ["asset_library_download_rejected", "settings.cloud.error.downloadRejected"],
  ["asset_library_save_failed", "settings.cloud.error.saveFailed"],
  ["asset_library_install_selection_invalid", "settings.cloud.error.installSelectionInvalid"],
  ["asset_library_install_selection_changed", "settings.cloud.error.installSelectionChanged"],
  ["asset_library_install_file_changed", "settings.cloud.error.installFileChanged"],
  ["asset_library_framework_required", "settings.cloud.error.frameworkRequired"],
  ["asset_library_host_install_rejected", "settings.cloud.error.hostInstallRejected"],
];

export function cloudError(error: unknown): string {
  const message = error instanceof Error ? error.message : String(error);
  const known = CLOUD_ERROR_MESSAGE_IDS.find(([code]) => message.includes(code));
  if (known) return uiText(known[1]);
  if (/401|403/.test(message)) return uiText("settings.cloud.error.localAuthFailed");
  if (/invalid args|missing required key/.test(message)) return uiText("settings.cloud.error.invalidDesktopArgs");
  if (/not found|not allowed|not permitted/.test(message)) return uiText("settings.cloud.error.loginBridgeUnavailable");
  if (/\u65e0\u6cd5\u8fde\u63a5|Failed to fetch|NetworkError|unable to connect|connection refused/i.test(message)) {
    return uiText("settings.cloud.error.localServiceUnreachable");
  }
  if (message.includes("naccount_response_invalid")) return uiText("settings.cloud.error.invalidLoginState");
  return uiText("settings.cloud.error.generic");
}
