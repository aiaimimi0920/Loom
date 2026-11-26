import { uiText } from "../../i18n/index.ts";
import { getJson, postJson, putJson } from "./transport.ts";
import { parseWallLayout, parseWallState, validWallId } from "./wallValidation.ts";
import type { WallLayout, WallPresentationMode } from "./wallTypes.ts";

export async function readWallState(baseUrl: string) {
  return parseWallState(await getJson<unknown>(baseUrl, "/v1/walls/state"));
}
export async function saveWallLayout(baseUrl: string, baseRevision: number, layout: WallLayout, activationDelayMs = 0) {
  if (!Number.isSafeInteger(baseRevision) || baseRevision < 0 || baseRevision >= Number.MAX_SAFE_INTEGER) {
    throw new Error(uiText("devices.wall.error.unwritableRevision"));
  }
  const candidate = parseWallLayout({ ...layout, revision: baseRevision + 1 });
  if (!Number.isInteger(activationDelayMs) || activationDelayMs < 0 || activationDelayMs > 10_000) {
    throw new Error(uiText("devices.wall.error.invalidActivationDelay"));
  }
  return parseWallState(await putJson<unknown>(baseUrl, "/v1/walls/layouts", { baseRevision, layout: candidate,
    ...(activationDelayMs ? { activationDelayMs } : {}) }));
}
export async function removeWallLayout(baseUrl: string, baseRevision: number, wallId: string) {
  return parseWallState(await postJson<unknown>(baseUrl, "/v1/walls/layouts/remove", { baseRevision, wallId }));
}

export async function setWallPresentation(baseUrl: string, baseRevision: number, wallId: string, mode: WallPresentationMode) {
  if (!Number.isSafeInteger(baseRevision) || baseRevision < 0 || baseRevision >= Number.MAX_SAFE_INTEGER || !validWallId(wallId)) {
    throw new Error(uiText("devices.wall.error.invalidPresentationTarget"));
  }
  return parseWallState(await putJson<unknown>(baseUrl, "/v1/walls/presentation", { baseRevision, wallId, mode }));
}

export async function identifyWallEndpoint(baseUrl: string, endpointId: string) {
  if (!validWallId(endpointId)) throw new Error(uiText("devices.wall.error.invalidIdentifyTarget"));
  return parseWallState(await postJson<unknown>(baseUrl, "/v1/walls/endpoints/identify", { endpointId }));
}
