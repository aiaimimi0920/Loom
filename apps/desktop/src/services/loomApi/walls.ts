import { getJson, postJson, putJson } from "./transport.ts";
import { parseWallLayout, parseWallState, validWallId } from "./wallValidation.ts";
import type { WallLayout, WallPresentationMode } from "./wallTypes.ts";

export async function readWallState(baseUrl: string) {
  return parseWallState(await getJson<unknown>(baseUrl, "/v1/walls/state"));
}
export async function saveWallLayout(baseUrl: string, baseRevision: number, layout: WallLayout, activationDelayMs = 0) {
  if (!Number.isSafeInteger(baseRevision) || baseRevision < 0 || baseRevision >= Number.MAX_SAFE_INTEGER) {
    throw new Error("目录版本不可写入");
  }
  const candidate = parseWallLayout({ ...layout, revision: baseRevision + 1 });
  if (!Number.isInteger(activationDelayMs) || activationDelayMs < 0 || activationDelayMs > 10_000) throw new Error("场景准备时间必须在 0 到 10000 ms 之间");
  return parseWallState(await putJson<unknown>(baseUrl, "/v1/walls/layouts", { baseRevision, layout: candidate,
    ...(activationDelayMs ? { activationDelayMs } : {}) }));
}
export async function removeWallLayout(baseUrl: string, baseRevision: number, wallId: string) {
  return parseWallState(await postJson<unknown>(baseUrl, "/v1/walls/layouts/remove", { baseRevision, wallId }));
}

export async function setWallPresentation(baseUrl: string, baseRevision: number, wallId: string, mode: WallPresentationMode) {
  if (!Number.isSafeInteger(baseRevision) || baseRevision < 0 || baseRevision >= Number.MAX_SAFE_INTEGER || !validWallId(wallId)) {
    throw new Error("显示控制目录版本或墙面无效");
  }
  return parseWallState(await putJson<unknown>(baseUrl, "/v1/walls/presentation", { baseRevision, wallId, mode }));
}

export async function identifyWallEndpoint(baseUrl: string, endpointId: string) {
  if (!validWallId(endpointId)) throw new Error("屏幕识别目标无效");
  return parseWallState(await postJson<unknown>(baseUrl, "/v1/walls/endpoints/identify", { endpointId }));
}
