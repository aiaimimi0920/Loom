import { uiText } from "../i18n/index.ts";
import { postJson } from "./loomApi/transport.ts";

export type AccountView =
  | { status: "signed_out"; remoteRevoked?: boolean }
  | { status: "pending"; origin: string; requestId: string; authorizationUrl: string; fingerprint: string; expiresAtMs: number }
  | { status: "signed_in"; origin: string; session: { accountId: string; deviceId: string; username: string; deviceName: string; expiresAtMs: number } };

const record = (value: unknown): value is Record<string, unknown> => typeof value === "object" && value !== null && !Array.isArray(value);
const text = (value: unknown, max = 256): value is string => typeof value === "string" && value.length > 0 && value.length <= max;

export function parseAccountView(value: unknown): AccountView {
  const fail = () => new Error(uiText("settings.cloud.account.error.invalidState"));
  if (!record(value)) throw fail();
  if (value.status === "signed_out") return { status: "signed_out", remoteRevoked: value.remoteRevoked === false ? false : undefined };
  if (!text(value.origin)) throw fail();
  if (value.status === "pending" && text(value.requestId, 64) && text(value.authorizationUrl, 2048)
      && text(value.fingerprint, 16) && Number.isSafeInteger(value.expiresAtMs)) {
    const url = new URL(value.authorizationUrl);
    if (url.origin !== value.origin || url.pathname !== "/loom/authorize" || url.username || url.password || url.hash) throw fail();
    if (url.protocol !== "https:" && !(url.protocol === "http:" && ["localhost", "127.0.0.1", "[::1]"].includes(url.hostname))) throw fail();
    return { status: "pending", origin: value.origin, requestId: value.requestId,
      authorizationUrl: url.toString(), fingerprint: value.fingerprint, expiresAtMs: value.expiresAtMs as number };
  }
  const session = value.session;
  if (value.status === "signed_in" && record(session) && text(session.accountId, 160)
      && text(session.deviceId, 36) && typeof session.username === "string" && session.username.length <= 640
      && text(session.deviceName, 320) && Number.isSafeInteger(session.expiresAtMs)) {
    return { status: "signed_in", origin: value.origin, session: {
      accountId: session.accountId, deviceId: session.deviceId, username: session.username,
      deviceName: session.deviceName, expiresAtMs: session.expiresAtMs as number,
    } };
  }
  throw fail();
}

export async function accountRequest(baseUrl: string, action: "status" | "start" | "poll" | "refresh" | "logout", body: unknown = {}) {
  return parseAccountView(await postJson<unknown>(baseUrl, `/v1/account/${action}`, body));
}

const ACCOUNT_ERROR_MESSAGE_IDS: ReadonlyArray<readonly [string, string]> = [
  ["account_origin_invalid", "settings.cloud.account.error.originInvalid"],
  ["account_network_unavailable", "settings.cloud.account.error.networkUnavailable"],
  ["account_clock_skew", "settings.cloud.account.error.clockSkew"],
  ["account_rate_limited", "settings.cloud.account.error.rateLimited"],
  ["account_secure_storage_unavailable", "settings.cloud.account.error.secureStorageUnavailable"],
  ["account_logout_required", "settings.cloud.account.error.logoutRequired"],
  ["account_signed_out", "settings.cloud.account.error.signedOut"],
];

export function accountError(error: unknown): string {
  const message = error instanceof Error ? error.message : String(error);
  const match = ACCOUNT_ERROR_MESSAGE_IDS.find(([code]) => message.includes(code));
  return uiText(match ? match[1] : "settings.cloud.account.error.generic");
}
