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
export function cloudError(error: unknown): string {
  const message = error instanceof Error ? error.message : String(error);
  const errors: Record<string, string> = {
    naccount_not_configured: "尚未配置 Loom 的 NAccount 客户端。",
    naccount_callback_port_required: "NAccount 登录需要本机 Loom 服务运行于 127.0.0.1:8765。",
    naccount_login_required: "登录已失效，请检查登录状态或重新登录。",
    naccount_signed_out: "请先登录 NAccount。",
    naccount_logout_required: "请先取消当前登录或退出账号。",
    naccount_network_unavailable: "NAccount 暂时无法连接。",
    asset_library_native_client_not_configured: "AssetLibrary 尚未配置 Loom 客户端。",
    asset_library_distribution_unavailable: "AssetLibrary 暂未开放分发。",
    asset_library_unavailable: "AssetLibrary 暂时无法连接。",
  };
  return Object.entries(errors).find(([code]) => message.includes(code))?.[1] ?? "请求未完成，请重试。";
}
