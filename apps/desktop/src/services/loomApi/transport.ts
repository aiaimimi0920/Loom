// Shared Tauri/HTTP transport selection, response normalization, and daemon preview access.
import { invoke, isTauri } from "@tauri-apps/api/core";
import type { LoomDaemonStartResult } from "./snapshotTypes.ts";

const MAX_DAEMON_JSON_RESPONSE_BYTES = 16 * 1024 * 1024;
const MAX_DAEMON_ERROR_DETAIL_CHARS = 2_048;

export const trimTrailingSlash = (value: string) => value.replace(/\/+$/, "");

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);

const daemonErrorMessage = (payload: unknown): string | null => {
  if (!isRecord(payload)) return null;
  const nestedError = payload.error;
  if (isRecord(nestedError)) {
    const message = nestedError.message;
    if (typeof message === "string" && message.trim().length > 0) {
      return message.trim().slice(0, MAX_DAEMON_ERROR_DETAIL_CHARS);
    }
  }
  for (const key of ["message", "detail"]) {
    const message = payload[key];
    if (typeof message === "string" && message.trim().length > 0) {
      return message.trim().slice(0, MAX_DAEMON_ERROR_DETAIL_CHARS);
    }
  }
  return null;
};

const readBoundedResponseText = async (response: Response, path: string): Promise<string> => {
  const declaredLength = response.headers.get("content-length");
  if (declaredLength && /^\d+$/.test(declaredLength)) {
    const parsedLength = Number(declaredLength);
    if (!Number.isSafeInteger(parsedLength) || parsedLength > MAX_DAEMON_JSON_RESPONSE_BYTES) {
      throw new Error(
        `Loom 本地服务请求 ${path} 的响应超过 ${MAX_DAEMON_JSON_RESPONSE_BYTES} 字节限制`,
      );
    }
  }

  if (!response.body) return "";
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let totalBytes = 0;
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      totalBytes += value.byteLength;
      if (totalBytes > MAX_DAEMON_JSON_RESPONSE_BYTES) {
        try {
          await reader.cancel();
        } catch {
          // A broken stream must not replace the deterministic size-limit error.
        }
        throw new Error(
          `Loom 本地服务请求 ${path} 的响应超过 ${MAX_DAEMON_JSON_RESPONSE_BYTES} 字节限制`,
        );
      }
      chunks.push(value);
    }
  } finally {
    reader.releaseLock();
  }

  const bytes = new Uint8Array(totalBytes);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return new TextDecoder().decode(bytes);
};

const daemonResponseError = async (response: Response, path: string): Promise<Error> => {
  let detail: string | null = null;
  try {
    detail = daemonErrorMessage(JSON.parse(await readBoundedResponseText(response, path)));
  } catch {
    // Preserve the HTTP status when an error response has no JSON body.
  }
  const suffix = detail ? `：${detail}` : "";
  return new Error(`Loom 本地服务请求 ${path} 返回 HTTP ${response.status}${suffix}`);
};

const responseJson = async <T>(response: Response, path: string): Promise<T> => {
  if (!response.ok) {
    throw await daemonResponseError(response, path);
  }
  if (response.status === 204) {
    return null as T;
  }
  return JSON.parse(await readBoundedResponseText(response, path)) as T;
};

export const readJson = async <T>(
  baseUrl: string,
  path: string,
  signal?: AbortSignal,
): Promise<T> => {
  const response = await fetch(`${trimTrailingSlash(baseUrl)}${path}`, {
    headers: {
      Accept: "application/json",
    },
    signal,
  });

  return await responseJson<T>(response, path);
};

export const errorMessage = (error: unknown): string => {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return "无法连接 Loom 本地服务";
};

export async function startLoomDaemon(): Promise<LoomDaemonStartResult> {
  return await invoke<LoomDaemonStartResult>("start_loom_daemon");
}

export const invokeJsonViaTauri = async <T>(
  command: string,
  args: Record<string, unknown>,
): Promise<T> => {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw new Error(errorMessage(error));
  }
};

const getJsonViaTauri = async <T>(baseUrl: string, path: string): Promise<T> => {
  return await invokeJsonViaTauri<T>("get_loom_daemon_json", { baseUrl, path });
};

const postJsonViaTauri = async <T>(baseUrl: string, path: string, body: unknown): Promise<T> => {
  return await invokeJsonViaTauri<T>("post_loom_daemon_json", { baseUrl, path, body });
};

const putJsonViaTauri = async <T>(baseUrl: string, path: string, body: unknown): Promise<T> => {
  return await invokeJsonViaTauri<T>("put_loom_daemon_json", { baseUrl, path, body });
};

const deleteJsonViaTauri = async <T>(baseUrl: string, path: string): Promise<T> => {
  return await invokeJsonViaTauri<T>("delete_loom_daemon_json", { baseUrl, path });
};

export const getJson = async <T>(baseUrl: string, path: string): Promise<T> => {
  if (isTauri()) {
    return await getJsonViaTauri<T>(baseUrl, path);
  }
  return await readJson<T>(baseUrl, path);
};

export async function getLoomDaemonJson<T>(baseUrl: string, path: string): Promise<T> {
  return await getJson<T>(baseUrl, path);
}

// Load a Hook canvas preview image. The WebView cannot reliably fetch daemon
// images through a direct `http://127.0.0.1` `<img src>`, so prefer the native
// Tauri command that returns a base64 `data:` URL. Fall back to the direct
// daemon URL only for browser previews where the Tauri command is unavailable.
export async function loadHookCanvasPreview(baseUrl: string, path: string): Promise<string> {
  if (isTauri()) {
    return await invoke<string>("read_hook_canvas_preview", { baseUrl, path });
  }

  // Browser previews may use a direct URL, but only for the daemon-owned preview route.
  if (!path.startsWith("/v1/hook-bridge/canvas/nodes/")) {
    throw new Error("Hook canvas preview path is outside the preview route");
  }
  const normalizedBaseUrl = baseUrl.endsWith("/") ? baseUrl : `${baseUrl}/`;
  const base = new URL(normalizedBaseUrl);
  const preview = new URL(path, base);
  if (
    preview.origin !== base.origin
    || !preview.pathname.startsWith("/v1/hook-bridge/canvas/nodes/")
    || !preview.pathname.endsWith("/preview")
  ) {
    throw new Error("Hook canvas preview path is outside the preview route");
  }
  return preview.toString();
}

export const postJson = async <T>(baseUrl: string, path: string, body: unknown): Promise<T> => {
  if (isTauri()) {
    return await postJsonViaTauri<T>(baseUrl, path, body);
  }
  const response = await fetch(`${trimTrailingSlash(baseUrl)}${path}`, {
    method: "POST",
    headers: {
      Accept: "application/json",
      "Content-Type": "application/json",
    },
    body: JSON.stringify(body ?? {}),
  });

  return await responseJson<T>(response, path);
};

export const putJson = async <T>(baseUrl: string, path: string, body: unknown): Promise<T> => {
  if (isTauri()) {
    return await putJsonViaTauri<T>(baseUrl, path, body);
  }
  const response = await fetch(`${trimTrailingSlash(baseUrl)}${path}`, {
    method: "PUT",
    headers: {
      Accept: "application/json",
      "Content-Type": "application/json",
    },
    body: JSON.stringify(body ?? {}),
  });

  return await responseJson<T>(response, path);
};

export const deleteJson = async <T>(baseUrl: string, path: string): Promise<T> => {
  if (isTauri()) {
    return await deleteJsonViaTauri<T>(baseUrl, path);
  }
  const response = await fetch(`${trimTrailingSlash(baseUrl)}${path}`, {
    method: "DELETE",
    headers: {
      Accept: "application/json",
    },
  });

  return await responseJson<T>(response, path);
};
