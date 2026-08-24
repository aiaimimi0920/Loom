// Shared-memory, settings, application-path, and Python authoring clients.
import { DEFAULT_LOOM_SETTINGS } from "./defaults.ts";
import type { SharedMemoryBufferResponse } from "./mcpTypes.ts";
import type {
  LoomPythonArtJsonResponse,
  LoomPythonNearbyArtJsonResponse,
  LoomPythonPortInferenceResponse,
  LoomPythonSourceReadResponse,
} from "./pythonTypes.ts";
import type { LoomAppPaths, LoomSettings, LoomShortcutConfig } from "./settingsTypes.ts";
import { deleteJson, getJson, postJson, putJson } from "./transport.ts";

export async function createSharedMemoryBuffer(
  baseUrl: string,
  request: { width: number; height: number; channels?: number },
): Promise<SharedMemoryBufferResponse> {
  return await postJson<SharedMemoryBufferResponse>(baseUrl, "/v1/shared-memory/buffers", request);
}

export async function listSharedMemoryBuffers(baseUrl: string): Promise<SharedMemoryBufferResponse> {
  return await getJson<SharedMemoryBufferResponse>(baseUrl, "/v1/shared-memory/buffers");
}

export async function getSharedMemoryBufferInfo(
  baseUrl: string,
  handle: string,
): Promise<SharedMemoryBufferResponse> {
  return await getJson<SharedMemoryBufferResponse>(
    baseUrl,
    `/v1/shared-memory/buffers/${encodeURIComponent(handle)}`,
  );
}

export async function releaseSharedMemoryBuffer(
  baseUrl: string,
  handle: string,
): Promise<SharedMemoryBufferResponse> {
  return await deleteJson<SharedMemoryBufferResponse>(
    baseUrl,
    `/v1/shared-memory/buffers/${encodeURIComponent(handle)}`,
  );
}

export async function getLoomSettings(baseUrl: string): Promise<LoomSettings> {
  const response = await getJson<{ settings?: LoomSettings }>(baseUrl, "/v1/settings");
  return response.settings ?? DEFAULT_LOOM_SETTINGS;
}

export async function saveLoomSettings(
  baseUrl: string,
  settings: LoomSettings,
): Promise<LoomSettings> {
  const response = await putJson<{ settings?: LoomSettings }>(
    baseUrl,
    "/v1/settings",
    settings,
  );
  return response.settings ?? settings;
}

export async function getLoomShortcuts(baseUrl: string): Promise<LoomShortcutConfig[]> {
  const response = await getJson<{ shortcuts?: LoomShortcutConfig[] }>(
    baseUrl,
    "/v1/settings/shortcuts",
  );
  return response.shortcuts ?? Object.values(DEFAULT_LOOM_SETTINGS.shortcuts);
}

export async function getLoomAppPaths(baseUrl: string): Promise<LoomAppPaths> {
  return await getJson<LoomAppPaths>(baseUrl, "/v1/runtime/app-paths");
}

export async function readPythonArtSource(
  baseUrl: string,
  path: string,
): Promise<LoomPythonSourceReadResponse> {
  return await postJson<LoomPythonSourceReadResponse>(baseUrl, "/v1/art-authoring/source/read", { path });
}

export async function readPythonArtJson(
  baseUrl: string,
  artPath: string,
): Promise<LoomPythonArtJsonResponse> {
  return await postJson<LoomPythonArtJsonResponse>(baseUrl, "/v1/art-authoring/source/read-art-json", { artPath });
}

export async function checkPythonArtJsonNearby(
  baseUrl: string,
  pythonPath: string,
): Promise<LoomPythonNearbyArtJsonResponse> {
  return await postJson<LoomPythonNearbyArtJsonResponse>(
    baseUrl,
    "/v1/art-authoring/source/check-art-json",
    { pythonPath },
  );
}

export async function inferPythonArtPorts(
  baseUrl: string,
  request: { path?: string; code?: string },
): Promise<LoomPythonPortInferenceResponse> {
  return await postJson<LoomPythonPortInferenceResponse>(
    baseUrl,
    "/v1/art-authoring/source/infer-ports",
    request,
  );
}
