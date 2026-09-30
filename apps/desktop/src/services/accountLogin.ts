import { postJson } from "./loomApi/transport.ts";

export type AccountView =
  | { status: "signed_out"; remoteRevoked?: boolean }
  | { status: "pending"; origin: string; requestId: string; authorizationUrl: string; fingerprint: string; expiresAtMs: number }
  | { status: "signed_in"; origin: string; session: { accountId: string; deviceId: string; username: string; deviceName: string; expiresAtMs: number } };

const record = (value: unknown): value is Record<string, unknown> => typeof value === "object" && value !== null && !Array.isArray(value);
const text = (value: unknown, max = 256): value is string => typeof value === "string" && value.length > 0 && value.length <= max;

export function parseAccountView(value: unknown): AccountView {
  const fail = () => new Error("Loom 返回的账号状态无效。");
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

export function accountError(error: unknown): string {
  const message = error instanceof Error ? error.message : String(error);
  const messages: Record<string, string> = {
    account_origin_invalid: "请填写有效的 HTTPS 账号服务地址。",
    account_network_unavailable: "账号服务暂时不可达，请检查网络后重试。",
    account_clock_skew: "本机时间与账号服务不一致，请校准时间后重试。",
    account_rate_limited: "请求过于频繁，请稍后重试。",
    account_secure_storage_unavailable: "当前系统尚不支持账号凭据的安全存储。",
    account_logout_required: "请先取消当前登录或退出账号。",
    account_signed_out: "登录请求已失效，请重新登录。",
  };
  return Object.entries(messages).find(([code]) => message.includes(code))?.[1] || "账号操作未完成，请重试。";
}
