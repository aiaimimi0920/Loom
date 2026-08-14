import { invoke, isTauri } from "@tauri-apps/api/core";
import type { McpRegistryResponse } from "./mcpMarketplace";

export const DEFAULT_LOOM_DAEMON_URL = "http://127.0.0.1:8765";

export type ConnectionState = "online" | "offline";

export interface LoomHealthResponse {
  status?: string;
}

export interface LoomModuleStatus {
  name: string;
  status: string;
  detail?: string | null;
}

export interface LoomStatusResponse {
  status?: string;
  modules?: LoomModuleStatus[];
  [key: string]: unknown;
}

export interface LoomCapability {
  id: string;
  description?: string;
  mode?: string;
  input_schema?: unknown;
  [key: string]: unknown;
}

export interface LoomCapabilitiesResponse {
  capabilities?: LoomCapability[];
}

export interface LoomMcpServer {
  id: string;
  serverId?: string;
  name: string;
  description?: string;
  transport?: "stdio" | "streamable-http" | "sse";
  command: string;
  args?: string[];
  env?: Record<string, string>;
  url?: string;
  headers?: Record<string, string>;
  enabled?: boolean;
  managed?: boolean;
  source?: "art" | "user";
  ownerArtId?: string;
  toolName?: string;
  readOnly?: boolean;
  editable?: boolean;
  deletable?: boolean;
  credentialRequired?: boolean;
  credentialBound?: boolean;
}

export interface LoomMcpServersResponse {
  servers?: LoomMcpServer[];
}

export interface LoomMcpTestResult {
  success?: boolean;
  tools?: unknown[];
  error?: string;
  server_info?: unknown;
  serverInfo?: unknown;
}

export interface LoomMcpCallToolResponse {
  status?: string;
  jsonrpc?: string;
  id?: number;
  result?: unknown;
  error?: unknown;
}

export interface LoomToolExecution {
  type?: string;
  [key: string]: unknown;
}

export interface LoomToolDefinition {
  id: string;
  name: string;
  description?: string;
  enabled?: boolean;
  execution?: LoomToolExecution;
  inputs?: unknown[];
  outputs?: unknown[];
  params?: unknown[];
  metadata?: unknown;
}

export interface LoomArtRuntimeManifest {
  protocolVersion: "loom.art.runtime.v1" | string;
  entry: {
    command: string;
    args?: string[];
  };
}

export interface LoomToolsResponse {
  tools?: LoomToolDefinition[];
}

export interface LoomPythonArt {
  path: string;
  art_json_path?: string;
  art_id: string;
  label: string;
  description?: string;
  version?: string;
  definition?: unknown;
}

export interface LoomPythonArtsResponse {
  arts?: LoomPythonArt[];
}

export interface LoomPythonSourceReadResponse {
  path: string;
  content: string;
  bytes?: number;
}

export interface LoomPythonArtJsonResponse {
  artJsonPath?: string;
  artJson?: unknown;
}

export interface LoomPythonNearbyArtJsonResponse extends LoomPythonArtJsonResponse {
  found: boolean;
  pythonPath?: string;
}

export interface LoomPythonPortDefinition {
  name: string;
  label?: string;
  type?: string;
  execution_type?: string;
  executionType?: string;
  default?: string;
}

export interface LoomPythonPortInferenceResponse {
  path?: string | null;
  inputs?: LoomPythonPortDefinition[];
  outputs?: LoomPythonPortDefinition[];
}

export interface LoomWorkflowMetadata {
  id: string;
  name: string;
  description?: string;
  nodeCount?: number;
  updatedAt?: string;
}

export interface LoomWorkflowsResponse {
  workflows?: LoomWorkflowMetadata[];
}

export interface LoomWorkflowBundle extends LoomWorkflowMetadata {
  data: string;
}

export interface LoomWorkflowBundleResponse {
  workflow?: LoomWorkflowBundle;
}

export interface LoomHookBridgeStatus {
  running?: boolean;
  port?: number;
  ipcPort?: number;
  connectedClients?: number;
  subscribedClients?: number;
  protocol?: string;
  sessionMethod?: string;
  methods?: string[];
}

export type LoomDeviceKind = "computer" | "tablet" | "phone" | "other";
export type LoomDeviceApproval = "approved" | "pending";

export interface LoomManagedDevice {
  id: string;
  name: string;
  kind: LoomDeviceKind;
  address: string;
  approval: LoomDeviceApproval;
  createdAt: number;
  lastSeenAt?: number | null;
  isLocal?: boolean;
  enabled?: boolean;
}

export interface LoomDevicesResponse {
  devices: LoomManagedDevice[];
  pending: LoomManagedDevice[];
  connectedClients: number;
}

export interface HookWorkflowInstantiateResponse {
  protocolVersion?: string;
  status?: string;
  method?: string;
  broadcasted?: boolean;
  subscribedClients?: number;
  params?: unknown;
}

export interface LoomShortcutConfig {
  id: string;
  label: string;
  keys: string;
  enabled: boolean;
}

export interface LoomSettings {
  appearance_version: number;
  general: {
    theme: string;
    language: string;
    auto_start: boolean;
    minimize_to_tray: boolean;
    enable_tray_icon: boolean;
  };
  hook_general: {
    theme: string;
    language: string;
    close_to_tray: boolean;
  };
  system: {
    auto_check_updates: boolean;
    enable_run_log: boolean;
    loom_log_level: string;
    hook_log_level: string;
    run_as_admin: boolean;
    record_screenshot_history: boolean;
    history_retention: string;
  };
  network: {
    loom: LoomProxySettings;
    hook: LoomProxySettings;
  };
  mcp: LoomMcpSettings;
  art_store: LoomArtStoreSettings;
  loom_cache: LoomCacheSettings;
  hook_cache: HookCacheSettings;
  engine: {
    comfyui_url: string;
    python_interpreter: string;
    virtual_env_path: string;
    compute_device: string;
    vram_reservation_gb: number;
  };
  quick_bindings: Array<{
    id: string;
    art: string;
    key: string;
  }>;
  shortcuts: Record<string, LoomShortcutConfig>;
}

export interface LoomMcpSettings {
  request_timeout_seconds: number;
  memory_limit_bytes: number;
}

export interface LoomArtStoreSettings {
  auto_update: boolean;
  official_only: boolean;
}

export interface LoomCacheSettings {
  art_cache_max_bytes: number;
  art_cache_retention_days: number;
  framework_temp_retention_days: number;
}

export interface HookCacheSettings {
  recycle_bin_max_entries: number;
  recycle_bin_retention_days: number;
  temp_cache_max_bytes: number;
  temp_cache_retention_days: number;
}

export interface LoomProxySettings {
  mode: "system" | "custom" | "disabled";
  protocol: "http" | "https" | "socks5";
  address: string;
}

export interface LoomAppPaths {
  dataDir: string;
  configDir: string;
  logDir: string;
}

export interface HookSessionSnapshot {
  running?: boolean;
  port?: number;
  connectedClients?: number;
  subscribedClients?: number;
  protocol?: string;
  sessionPath?: string;
  available?: boolean;
  error?: string | null;
  session?: {
    stickers?: unknown[];
    links?: unknown[];
    [key: string]: unknown;
  };
}

export interface McpPackageCheckResult {
  installed?: boolean;
  module?: string;
  python?: string;
  stdout?: string;
  stderr?: string;
  error?: string;
}

export interface McpPackageInstallPlan {
  package?: string;
  sideEffect?: boolean;
  mode?: string;
  command?: string[];
  message?: string;
}

export interface SharedMemoryBufferInfo {
  handle?: string;
  handle_name?: string;
  size?: number;
  width?: number;
  height?: number;
  format?: string;
  ref_count?: number;
}

export interface SharedMemoryBufferResponse {
  handle?: string;
  handle_name?: string;
  buffer?: SharedMemoryBufferInfo;
  buffers?: SharedMemoryBufferInfo[];
  released?: boolean;
  deleted?: boolean;
}

export const DEFAULT_LOOM_SETTINGS: LoomSettings = {
  appearance_version: 1,
  general: {
    theme: "dark",
    language: "zh-Hans",
    auto_start: false,
    minimize_to_tray: true,
    enable_tray_icon: true,
  },
  hook_general: {
    theme: "dark",
    language: "zh-Hans",
    close_to_tray: true,
  },
  system: {
    auto_check_updates: true,
    enable_run_log: true,
    loom_log_level: "info",
    hook_log_level: "info",
    run_as_admin: false,
    record_screenshot_history: true,
    history_retention: "7d",
  },
  network: {
    loom: { mode: "system", protocol: "http", address: "" },
    hook: { mode: "system", protocol: "http", address: "" },
  },
  mcp: {
    request_timeout_seconds: 60,
    memory_limit_bytes: 512 * 1024 * 1024,
  },
  art_store: {
    auto_update: true,
    official_only: false,
  },
  loom_cache: {
    art_cache_max_bytes: 1024 * 1024 * 1024,
    art_cache_retention_days: 30,
    framework_temp_retention_days: 3,
  },
  hook_cache: {
    recycle_bin_max_entries: 15,
    recycle_bin_retention_days: 0,
    temp_cache_max_bytes: 256 * 1024 * 1024,
    temp_cache_retention_days: 7,
  },
  engine: {
    comfyui_url: "http://127.0.0.1:8188",
    python_interpreter: "python.exe",
    virtual_env_path: "./venv",
    compute_device: "0",
    vram_reservation_gb: 12,
  },
  quick_bindings: [],
  shortcuts: {
    cancel: { id: "cancel", label: "Cancel / Deselect", keys: "Escape", enabled: true },
    capture: { id: "capture", label: "Screenshot", keys: "Ctrl+1", enabled: true },
    copy_unit: { id: "copy_unit", label: "Copy Unit", keys: "Ctrl+C", enabled: true },
    paste_unit: { id: "paste_unit", label: "Paste Unit", keys: "Ctrl+V", enabled: true },
    save_image: { id: "save_image", label: "Save Image", keys: "Ctrl+S", enabled: true },
    toggle_ocr: { id: "toggle_ocr", label: "Toggle OCR", keys: "Alt+2", enabled: true },
    toggle_translation: { id: "toggle_translation", label: "Toggle Translation", keys: "Alt+3", enabled: true },
  },
};

export interface LoomSettingsLinks {
  root: string;
  tea: string;
  hook: string;
  talk: string;
}

export interface LoomDaemonStartResult {
  started: boolean;
  baseUrl: string;
  path: string;
  message: string;
}

export interface LoomSnapshot {
  baseUrl: string;
  connectionState: ConnectionState;
  checkedAt: string;
  health: LoomHealthResponse | null;
  status: LoomStatusResponse | null;
  capabilities: LoomCapability[];
  mcpServers: LoomMcpServer[];
  tools: LoomToolDefinition[];
  pythonArts: LoomPythonArt[];
  workflows: LoomWorkflowMetadata[];
  hookBridge: LoomHookBridgeStatus | null;
  settings: LoomSettingsLinks;
  error: string | null;
}

export interface LoomOnlineWaitOptions {
  timeoutMs?: number;
  intervalMs?: number;
  attemptTimeoutMs?: number;
  sleep?: (delayMs: number) => Promise<void>;
  now?: () => number;
  onAttemptTimeout?: () => void;
}

type TimedReadResult<T> =
  | { timedOut: false; value: T }
  | { timedOut: true };

async function readBeforeTimeout<T>(
  operation: (signal: AbortSignal) => Promise<T>,
  timeoutMs: number,
  onTimeout?: () => void,
): Promise<TimedReadResult<T>> {
  const controller = new AbortController();
  let timeoutId: ReturnType<typeof globalThis.setTimeout> | null = null;
  const timeout = new Promise<TimedReadResult<T>>((resolve) => {
    timeoutId = globalThis.setTimeout(() => {
      controller.abort();
      onTimeout?.();
      resolve({ timedOut: true });
    }, timeoutMs);
  });

  try {
    return await Promise.race([
      operation(controller.signal).then((value) => ({ timedOut: false, value }) as const),
      timeout,
    ]);
  } finally {
    if (timeoutId !== null) {
      globalThis.clearTimeout(timeoutId);
    }
  }
}

export async function waitForLoomOnline(
  readSnapshot: (signal: AbortSignal) => Promise<LoomSnapshot>,
  options: LoomOnlineWaitOptions = {},
): Promise<LoomSnapshot | null> {
  const timeoutMs = Number.isFinite(options.timeoutMs)
    ? Math.max(0, options.timeoutMs ?? 0)
    : 15_000;
  const intervalMs = Number.isFinite(options.intervalMs)
    ? Math.max(1, options.intervalMs ?? 1)
    : 250;
  const attemptTimeoutMs = Number.isFinite(options.attemptTimeoutMs)
    ? Math.max(1, options.attemptTimeoutMs ?? 1)
    : 3_000;
  const now = options.now ?? Date.now;
  const sleep = options.sleep ?? ((delayMs: number) => new Promise<void>((resolve) => {
    globalThis.setTimeout(resolve, delayMs);
  }));
  const deadline = now() + timeoutMs;
  let lastSnapshot: LoomSnapshot | null = null;

  while (now() < deadline) {
    const remainingMs = Math.max(1, deadline - now());
    const result = await readBeforeTimeout(
      readSnapshot,
      Math.min(attemptTimeoutMs, remainingMs),
      options.onAttemptTimeout,
    );
    if (!result.timedOut) {
      lastSnapshot = result.value;
      if (lastSnapshot.connectionState === "online") {
        return lastSnapshot;
      }
    }

    const remainingAfterRead = deadline - now();
    if (remainingAfterRead <= 0) {
      break;
    }
    await sleep(Math.min(intervalMs, remainingAfterRead));
  }

  return lastSnapshot;
}

const trimTrailingSlash = (value: string) => value.replace(/\/+$/, "");

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);

export function retainAvailableSnapshotData(previous: LoomSnapshot, next: LoomSnapshot): LoomSnapshot {
  if (previous.connectionState !== "online") return next;
  if (next.connectionState === "offline") {
    return {
      ...next,
      capabilities: previous.capabilities,
      mcpServers: previous.mcpServers,
      tools: previous.tools,
      pythonArts: previous.pythonArts,
      workflows: previous.workflows,
      hookBridge: previous.hookBridge,
    };
  }
  if (!next.error) return next;

  const moduleFailed = (path: string) => next.error?.includes(path) === true;
  return {
    ...next,
    capabilities: moduleFailed("/v1/capabilities") ? previous.capabilities : next.capabilities,
    mcpServers: moduleFailed("/v1/mcp/servers") ? previous.mcpServers : next.mcpServers,
    tools: moduleFailed("/v1/tools") ? previous.tools : next.tools,
    pythonArts: moduleFailed("/v1/art-authoring/python/arts") ? previous.pythonArts : next.pythonArts,
    workflows: moduleFailed("/v1/workflows") ? previous.workflows : next.workflows,
    hookBridge: moduleFailed("/v1/hook-bridge/status") ? previous.hookBridge : next.hookBridge,
  };
}

const daemonErrorMessage = (payload: unknown): string | null => {
  if (!isRecord(payload)) return null;
  const nestedError = payload.error;
  if (isRecord(nestedError)) {
    const message = nestedError.message;
    if (typeof message === "string" && message.trim().length > 0) {
      return message.trim();
    }
  }
  for (const key of ["message", "detail"]) {
    const message = payload[key];
    if (typeof message === "string" && message.trim().length > 0) {
      return message.trim();
    }
  }
  return null;
};

const daemonResponseError = async (response: Response, path: string): Promise<Error> => {
  let detail: string | null = null;
  try {
    detail = daemonErrorMessage(await response.json());
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
  return (await response.json()) as T;
};

const buildSettingsLinks = (baseUrl: string): LoomSettingsLinks => {
  const root = `${trimTrailingSlash(baseUrl)}/settings`;
  return {
    root,
    tea: `${trimTrailingSlash(baseUrl)}/settings/tea`,
    hook: `${trimTrailingSlash(baseUrl)}/settings/hook`,
    talk: `${trimTrailingSlash(baseUrl)}/settings/talk`,
  };
};

const readJson = async <T>(baseUrl: string, path: string): Promise<T> => {
  const response = await fetch(`${trimTrailingSlash(baseUrl)}${path}`, {
    headers: {
      Accept: "application/json",
    },
  });

  return await responseJson<T>(response, path);
};

const errorMessage = (error: unknown): string => {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return "无法连接 Loom 本地服务";
};

interface OptionalSnapshotValue<T> {
  value: T;
  error: string | null;
}

const readOptionalSnapshotArray = async <T>(
  baseUrl: string,
  path: string,
  key: string,
): Promise<OptionalSnapshotValue<T[]>> => {
  try {
    const response = await readJson<Record<string, unknown>>(baseUrl, path);
    const value = response[key];
    if (!Array.isArray(value)) {
      return {
        value: [],
        error: `Loom 本地服务模块 ${path} 响应字段 ${key} 必须是数组`,
      };
    }
    return { value: value as T[], error: null };
  } catch (error) {
    return { value: [], error: errorMessage(error) };
  }
};

const readOptionalSnapshotObject = async <T extends object>(
  baseUrl: string,
  path: string,
): Promise<OptionalSnapshotValue<T | null>> => {
  try {
    const response = await readJson<unknown>(baseUrl, path);
    if (typeof response !== "object" || response === null || Array.isArray(response)) {
      return {
        value: null,
        error: `Loom 本地服务模块 ${path} 响应必须是对象`,
      };
    }
    return { value: response as T, error: null };
  } catch (error) {
    return { value: null, error: errorMessage(error) };
  }
};

const readSnapshotViaTauri = async (baseUrl: string): Promise<LoomSnapshot> => {
  return await invoke<LoomSnapshot>("read_loom_snapshot", { baseUrl });
};

export async function startLoomDaemon(): Promise<LoomDaemonStartResult> {
  return await invoke<LoomDaemonStartResult>("start_loom_daemon");
}

const invokeJsonViaTauri = async <T>(command: string, args: Record<string, unknown>): Promise<T> => {
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

const getJson = async <T>(baseUrl: string, path: string): Promise<T> => {
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
  try {
    return await invoke<string>("read_hook_canvas_preview", { baseUrl, path });
  } catch {
    // Browser previews do not expose Tauri commands. Fall back to a direct URL
    // so local frontend checks can still render the preview when CORS allows it.
    const normalizedBaseUrl = baseUrl.endsWith("/") ? baseUrl : `${baseUrl}/`;
    return new URL(path.replace(/^\//, ""), normalizedBaseUrl).toString();
  }
}

const postJson = async <T>(baseUrl: string, path: string, body: unknown): Promise<T> => {
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

const putJson = async <T>(baseUrl: string, path: string, body: unknown): Promise<T> => {
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

const deleteJson = async <T>(baseUrl: string, path: string): Promise<T> => {
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

export async function startHookBridge(
  baseUrl = DEFAULT_LOOM_DAEMON_URL,
  port?: number,
): Promise<LoomHookBridgeStatus> {
  const body = typeof port === "number" ? { port } : {};
  return await postJson<LoomHookBridgeStatus>(baseUrl, "/v1/hook-bridge/start", body);
}

export async function stopHookBridge(baseUrl = DEFAULT_LOOM_DAEMON_URL): Promise<LoomHookBridgeStatus> {
  return await postJson<LoomHookBridgeStatus>(baseUrl, "/v1/hook-bridge/stop", {});
}

export async function listManagedDevices(
  baseUrl = DEFAULT_LOOM_DAEMON_URL,
): Promise<LoomDevicesResponse> {
  return await getJson<LoomDevicesResponse>(baseUrl, "/v1/devices");
}

export async function addManagedDevice(
  baseUrl: string,
  input: { name: string; kind: LoomDeviceKind; address: string },
): Promise<LoomDevicesResponse> {
  return await postJson<LoomDevicesResponse>(baseUrl, "/v1/devices", input);
}

export async function approveManagedDevice(
  baseUrl: string,
  deviceId: string,
): Promise<LoomDevicesResponse> {
  return await postJson<LoomDevicesResponse>(
    baseUrl,
    `/v1/devices/${encodeURIComponent(deviceId)}/approve`,
    {},
  );
}

export async function updateManagedDevice(
  baseUrl: string,
  deviceId: string,
  input: { name: string; kind: LoomDeviceKind; address: string; enabled: boolean },
): Promise<LoomDevicesResponse> {
  return await putJson<LoomDevicesResponse>(
    baseUrl,
    `/v1/devices/${encodeURIComponent(deviceId)}`,
    input,
  );
}

export async function removeManagedDevice(
  baseUrl: string,
  deviceId: string,
): Promise<LoomDevicesResponse> {
  return await deleteJson<LoomDevicesResponse>(
    baseUrl,
    `/v1/devices/${encodeURIComponent(deviceId)}`,
  );
}

export async function readHookSession(baseUrl = DEFAULT_LOOM_DAEMON_URL): Promise<HookSessionSnapshot> {
  return await getJson<HookSessionSnapshot>(baseUrl, "/v1/hook-bridge/session");
}

export async function instantiateHookWorkflow(
  baseUrl: string,
  request: {
    nodes: unknown[];
    edges: unknown[];
    mode?: string;
    workflowId?: string | null;
  },
): Promise<HookWorkflowInstantiateResponse> {
  return await postJson<HookWorkflowInstantiateResponse>(
    baseUrl,
    "/v1/hook-bridge/workflows/instantiate",
    request,
  );
}

export async function updateHookWorkflowNode(
  baseUrl: string,
  request: {
    workflowId: string;
    nodeId: string;
    param: string;
    value: unknown;
  },
): Promise<Record<string, unknown>> {
  return await postJson<Record<string, unknown>>(
    baseUrl,
    "/v1/hook-bridge/workflows/nodes/update",
    request,
  );
}

export async function saveWorkflowBundle(
  baseUrl: string,
  workflow: Pick<LoomWorkflowMetadata, "id">,
  data: string,
): Promise<void> {
  await putJson(baseUrl, `/v1/workflows/${encodeURIComponent(workflow.id)}`, { data });
}

export async function saveHookCanvasWorkflow(
  baseUrl: string,
  request: {
    workflowId: string;
    selectedNodeId: string;
    workflowName?: string;
  },
): Promise<void> {
  await putJson(
    baseUrl,
    `/v1/hook-bridge/canvas/workflows/${encodeURIComponent(request.workflowId)}`,
    {
      selectedNodeId: request.selectedNodeId,
      workflowName: request.workflowName ?? request.workflowId,
    },
  );
}

export interface LoomFrameworkPublisher {
  id: string;
  name?: string | null;
  website?: string | null;
  keyId?: string | null;
}

export interface LoomFrameworkAuthoringOption {
  value: unknown;
  label: string;
}

export interface LoomFrameworkAuthoringField {
  id: string;
  label: string;
  type: "string" | "number" | "boolean" | "enum" | "path" | "secret" | "json" | string;
  required?: boolean;
  default?: unknown;
  options?: LoomFrameworkAuthoringOption[];
  placeholder?: string | null;
  minimum?: number | null;
  maximum?: number | null;
  step?: number | null;
  secret?: boolean;
}

export interface LoomFrameworkAuthoringPort {
  name: string;
  label: string;
  type: string;
  executionType: string;
  required?: boolean;
  exposePort?: boolean;
}

export interface LoomFrameworkAuthoringSchema {
  schemaVersion: number;
  title: string;
  description?: string | null;
  fields?: LoomFrameworkAuthoringField[];
  inputs?: LoomFrameworkAuthoringPort[];
  outputs?: LoomFrameworkAuthoringPort[];
}

export interface LoomFrameworkPermissionPolicy {
  network?: {
    domains?: string[];
    allowLocalhost?: boolean;
    allowPrivateNetworks?: boolean;
  };
  filesystem?: { read?: string[]; write?: string[] };
  process?: { spawn?: boolean; maxProcesses?: number | null };
  gpu?: boolean;
  clipboard?: boolean;
  credentials?: string[];
}

export interface LoomFrameworkResourceLimits {
  timeoutSeconds?: number | null;
  memoryMiB?: number | null;
  maxProcesses?: number | null;
  stdoutMiB?: number | null;
  stderrMiB?: number | null;
}

// One Art execution framework's package, authoring, and readiness status.
export interface LoomFramework {
  id: string;
  qualifiedId?: string;
  name: string;
  description: string;
  installed: boolean;
  enabled: boolean;
  ready: boolean;
  readyDetail: string;
  version?: string | null;
  runtimeDir?: string | null;
  publisher?: LoomFrameworkPublisher | null;
  trustStatus?: "trusted" | "verified" | "unsigned" | "invalid" | "revoked" | string;
  declaredPermissions?: string[];
  permissionPolicy?: LoomFrameworkPermissionPolicy;
  resources?: LoomFrameworkResourceLimits;
  authoringSchema?: LoomFrameworkAuthoringSchema | null;
}

interface LoomFrameworksResponse {
  frameworks?: LoomFramework[];
}

interface LoomFrameworkResponse {
  framework?: LoomFramework;
}

const frameworkStatusWeight = (framework: LoomFramework): number => (
  (framework.installed ? 16 : 0)
  + (framework.ready ? 8 : 0)
  + (framework.enabled ? 4 : 0)
  + (framework.version ? 2 : 0)
  + (framework.runtimeDir ? 1 : 0)
);

const uniqueFrameworks = (frameworks: LoomFramework[]): LoomFramework[] => {
  const byIdentity = new Map<string, LoomFramework>();
  for (const framework of frameworks) {
    const identity = framework.qualifiedId?.trim() || framework.id;
    const existing = byIdentity.get(identity);
    if (!existing || frameworkStatusWeight(framework) > frameworkStatusWeight(existing)) {
      byIdentity.set(identity, framework);
    }
  }
  return [...byIdentity.values()];
};

export async function listFrameworks(baseUrl: string): Promise<LoomFramework[]> {
  const response = await getJson<LoomFrameworksResponse>(baseUrl, "/v1/frameworks");
  return Array.isArray(response.frameworks) ? uniqueFrameworks(response.frameworks) : [];
}

export async function installFramework(baseUrl: string, id: string): Promise<LoomFramework | null> {
  const response = isTauri()
    ? await invokeJsonViaTauri<LoomFrameworkResponse>("install_packaged_framework", { baseUrl, id })
    : await postJson<LoomFrameworkResponse>(
      baseUrl,
      `/v1/frameworks/${encodeURIComponent(id)}/install`,
      {},
    );
  return response.framework ?? null;
}

export interface LoomPackagedArtBootstrapResult {
  available: boolean;
  applied: boolean;
  catalogHash?: string | null;
  frameworkIds: string[];
  artIds: string[];
}

export async function bootstrapPackagedArts(
  baseUrl: string,
): Promise<LoomPackagedArtBootstrapResult> {
  if (!isTauri()) {
    return {
      available: false,
      applied: false,
      catalogHash: null,
      frameworkIds: [],
      artIds: [],
    };
  }
  return await invokeJsonViaTauri<LoomPackagedArtBootstrapResult>("bootstrap_packaged_arts", {
    baseUrl,
  });
}

export async function uninstallFramework(baseUrl: string, id: string): Promise<LoomFramework | null> {
  const response = await postJson<LoomFrameworkResponse>(
    baseUrl,
    `/v1/frameworks/${encodeURIComponent(id)}/uninstall`,
    {},
  );
  return response.framework ?? null;
}

export async function enableFramework(baseUrl: string, id: string): Promise<LoomFramework | null> {
  const response = await postJson<LoomFrameworkResponse>(
    baseUrl,
    `/v1/frameworks/${encodeURIComponent(id)}/enable`,
    {},
  );
  return response.framework ?? null;
}

export async function disableFramework(baseUrl: string, id: string): Promise<LoomFramework | null> {
  const response = await postJson<LoomFrameworkResponse>(
    baseUrl,
    `/v1/frameworks/${encodeURIComponent(id)}/disable`,
    {},
  );
  return response.framework ?? null;
}

export async function installFrameworkPackage(
  baseUrl: string,
  zipBase64: string,
): Promise<LoomFramework | null> {
  const response = await postJson<LoomFrameworkResponse>(baseUrl, "/v1/frameworks/install", {
    zipBase64,
  });
  return response.framework ?? null;
}

export async function upgradeFrameworkPackage(
  baseUrl: string,
  id: string,
  zipBase64: string,
): Promise<LoomFramework | null> {
  const response = await postJson<LoomFrameworkResponse>(
    baseUrl,
    `/v1/frameworks/${encodeURIComponent(id)}/upgrade`,
    { zipBase64 },
  );
  return response.framework ?? null;
}

export interface LoomPublisherTrustRecord {
  publisherId: string;
  keyId: string;
  publicKey: string;
  revoked: boolean;
}

export type LoomPluginTrustPolicy = "allow_unsigned" | "require_signed" | "require_trusted";

export interface LoomPluginTrustStore {
  schemaVersion?: number;
  publishers: LoomPublisherTrustRecord[];
  policy: LoomPluginTrustPolicy;
  trustedPublishers: string[];
}

function normalizePluginTrustStore(response: Partial<LoomPluginTrustStore>): LoomPluginTrustStore {
  return {
    schemaVersion: response.schemaVersion,
    publishers: Array.isArray(response.publishers) ? response.publishers : [],
    policy: response.policy ?? "allow_unsigned",
    trustedPublishers: Array.isArray(response.trustedPublishers) ? response.trustedPublishers : [],
  };
}

export async function listPluginTrust(baseUrl: string): Promise<LoomPluginTrustStore> {
  const response = await getJson<LoomPluginTrustStore>(baseUrl, "/v1/plugin-trust");
  return normalizePluginTrustStore(response);
}

export async function trustPluginPublisher(
  baseUrl: string,
  record: Omit<LoomPublisherTrustRecord, "revoked"> & { revoked?: boolean },
): Promise<LoomPluginTrustStore> {
  const response = await postJson<LoomPluginTrustStore>(baseUrl, "/v1/plugin-trust/publishers", {
    ...record,
    revoked: record.revoked ?? false,
  });
  return normalizePluginTrustStore(response);
}

export async function revokePluginPublisher(
  baseUrl: string,
  publisherId: string,
  keyId: string,
): Promise<LoomPluginTrustStore> {
  const response = await postJson<LoomPluginTrustStore>(baseUrl, "/v1/plugin-trust/revoke", {
    publisherId,
    keyId,
  });
  return normalizePluginTrustStore(response);
}

export async function setPluginTrustPolicy(
  baseUrl: string,
  policy: LoomPluginTrustPolicy,
): Promise<LoomPluginTrustStore> {
  return normalizePluginTrustStore(await postJson<LoomPluginTrustStore>(
    baseUrl,
    "/v1/plugin-trust/policy",
    { policy },
  ));
}

export async function trustPluginUser(
  baseUrl: string,
  userId: string,
): Promise<LoomPluginTrustStore> {
  return normalizePluginTrustStore(await postJson<LoomPluginTrustStore>(
    baseUrl,
    "/v1/plugin-trust/users",
    { userId },
  ));
}

export async function untrustPluginUser(
  baseUrl: string,
  userId: string,
): Promise<LoomPluginTrustStore> {
  return normalizePluginTrustStore(await postJson<LoomPluginTrustStore>(
    baseUrl,
    "/v1/plugin-trust/users/remove",
    { userId },
  ));
}

export interface LoomCredentialScope {
  frameworkId?: string;
  artId?: string;
}

export type LoomCredentialValueType = "string" | "number" | "integer" | "boolean" | "json";

export interface LoomCredentialSummary {
  name: string;
  valueType: LoomCredentialValueType;
  scope: LoomCredentialScope;
  expiresAt?: string | null;
  protection: string;
}

export interface LoomCredentialDetails extends LoomCredentialSummary {
  value: string;
}

export interface LoomCredentialInput {
  name: string;
  value: string;
  valueType: LoomCredentialValueType;
  scope?: LoomCredentialScope;
  expiresAt?: string | null;
}

interface LoomCredentialsResponse {
  credentials?: LoomCredentialSummary[];
}

interface LoomCredentialResponse {
  credential?: LoomCredentialSummary;
}

export async function listPluginCredentials(baseUrl: string): Promise<LoomCredentialSummary[]> {
  const response = await getJson<LoomCredentialsResponse>(baseUrl, "/v1/plugin-credentials");
  return Array.isArray(response.credentials) ? response.credentials : [];
}

export async function savePluginCredential(
  baseUrl: string,
  input: LoomCredentialInput,
): Promise<LoomCredentialSummary | null> {
  const response = await postJson<LoomCredentialResponse>(baseUrl, "/v1/plugin-credentials", input);
  return response.credential ?? null;
}

export async function deletePluginCredential(
  baseUrl: string,
  name: string,
  scope: LoomCredentialScope = {},
): Promise<void> {
  await postJson(baseUrl, "/v1/plugin-credentials/delete", { name, scope });
}

export async function revealPluginCredential(
  baseUrl: string,
  name: string,
  scope: LoomCredentialScope = {},
): Promise<LoomCredentialDetails | null> {
  const response = await postJson<{ credential?: LoomCredentialDetails }>(
    baseUrl,
    "/v1/plugin-credentials/reveal",
    { name, scope },
  );
  return response.credential ?? null;
}

export interface LoomPublisherIdentity {
  schemaVersion: number;
  userId: string;
  currentKeyId: string;
  publicKey: string;
}

export interface LoomPublisherIdentityState {
  identity: LoomPublisherIdentity | null;
  hasPrivateKey: boolean;
}

export async function getPublisherIdentity(baseUrl: string): Promise<LoomPublisherIdentityState> {
  const response = await getJson<Partial<LoomPublisherIdentityState>>(baseUrl, "/v1/publisher-identity");
  return {
    identity: response.identity ?? null,
    hasPrivateKey: response.hasPrivateKey === true,
  };
}

export async function registerPublisherIdentity(baseUrl: string): Promise<LoomPublisherIdentityState> {
  return await postJson<LoomPublisherIdentityState>(baseUrl, "/v1/publisher-identity/register", {});
}

export async function rotatePublisherIdentity(baseUrl: string): Promise<LoomPublisherIdentityState> {
  return await postJson<LoomPublisherIdentityState>(baseUrl, "/v1/publisher-identity/rotate", {});
}

export async function revealPublisherPrivateKey(baseUrl: string): Promise<{
  keyId: string;
  privateKey: string;
  publicKey: string;
}> {
  return await postJson(baseUrl, "/v1/publisher-identity/private-key", {});
}

// A remote art-store catalog entry.
export interface ArtStoreEntry {
  id: string;
  qualifiedId?: string;
  globalId?: string;
  name?: string;
  description?: string;
  framework?: string;
  latestVersion?: string;
  versions?: Array<{ version: string; sha256?: string }>;
  official?: boolean;
}

interface ArtStoreCatalogResponse {
  arts?: ArtStoreEntry[];
}

export async function fetchArtStoreCatalog(baseUrl: string): Promise<ArtStoreEntry[]> {
  const response = await getJson<ArtStoreCatalogResponse>(baseUrl, "/v1/arts/store/catalog");
  return Array.isArray(response.arts) ? response.arts : [];
}

export async function installArtFromStore(
  baseUrl: string,
  artId: string,
  version?: string,
): Promise<void> {
  await postJson(baseUrl, "/v1/arts/store/install", { artId, version });
}

export interface LoomArtManagementParameter {
  id: string;
  label: string;
  parameterType: string;
  required: boolean;
  secret: boolean;
  default?: unknown;
  options?: unknown;
  minimum?: unknown;
  maximum?: unknown;
  step?: unknown;
}

export interface LoomArtInstalledVersion {
  version: string;
  digest: string;
  active: boolean;
}

export interface LoomArtManagement {
  artId: string;
  name: string;
  description: string;
  locallyAuthored: boolean;
  canEditIdentity: boolean;
  currentVersion: string;
  highestVersion: string;
  autoUpdate: boolean;
  installedVersions: LoomArtInstalledVersion[];
  availableVersions: string[];
  parameters: LoomArtManagementParameter[];
  defaults: Record<string, unknown>;
  valueBindings: Record<string, string>;
  credentialBindings: Record<string, string>;
  availableCredentials: LoomCredentialSummary[];
  updateAvailable: boolean;
}

export interface LoomArtManagementSettingsInput {
  name?: string;
  description?: string;
  autoUpdate: boolean;
  defaults: Record<string, unknown>;
  valueBindings: Record<string, string>;
  credentialBindings: Record<string, string>;
  secretValues?: Record<string, string>;
}

export async function getArtManagement(
  baseUrl: string,
  artId: string,
): Promise<LoomArtManagement> {
  return await getJson<LoomArtManagement>(
    baseUrl,
    `/v1/arts/${encodeURIComponent(artId)}/management`,
  );
}

export async function saveArtManagementSettings(
  baseUrl: string,
  artId: string,
  input: LoomArtManagementSettingsInput,
): Promise<LoomArtManagement> {
  return await putJson<LoomArtManagement>(
    baseUrl,
    `/v1/arts/${encodeURIComponent(artId)}/settings`,
    input,
  );
}

export async function updateArtToVersion(
  baseUrl: string,
  artId: string,
  version: string,
): Promise<LoomArtManagement> {
  return await postJson<LoomArtManagement>(
    baseUrl,
    `/v1/arts/${encodeURIComponent(artId)}/update`,
    { version },
  );
}

export async function autoUpdateArts(
  baseUrl: string,
): Promise<{ updated?: unknown[]; errors?: unknown[] }> {
  return await postJson(baseUrl, "/v1/arts/auto-update", {});
}

export async function installArtPackage(baseUrl: string, zipBase64: string): Promise<void> {
  await postJson(baseUrl, "/v1/arts/install", { zipBase64 });
}

export async function createAuthoredArtPackage(
  baseUrl: string,
  tool: LoomToolDefinition,
  runtime?: LoomArtRuntimeManifest,
  options?: {
    files?: Array<{ path: string; content: string }>;
    sourceDirectory?: string;
    sourceDirectoryTarget?: string;
  },
): Promise<LoomToolDefinition | null> {
  const response = await postJson<{ tool?: LoomToolDefinition }>(baseUrl, "/v1/arts/create", {
    tool,
    runtime,
    files: options?.files ?? [],
    sourceDirectory: options?.sourceDirectory,
    sourceDirectoryTarget: options?.sourceDirectoryTarget,
  });
  return response.tool ?? null;
}

export async function publishArt(
  baseUrl: string,
  artId: string,
): Promise<{ artId: string; globalId: string; sha256: string; published: boolean }> {
  return await postJson(baseUrl, "/v1/arts/store/publish", { artId });
}

export async function deleteCanvasWorkflow(baseUrl: string, workflowId: string): Promise<void> {
  await deleteJson(baseUrl, `/v1/hook-bridge/canvas/workflows/${encodeURIComponent(workflowId)}`);
}

export async function renameCanvasWorkflow(
  baseUrl: string,
  workflowId: string,
  name: string,
): Promise<void> {
  await putJson(
    baseUrl,
    `/v1/hook-bridge/canvas/workflows/${encodeURIComponent(workflowId)}/rename`,
    { name },
  );
}

export async function getWorkflowBundle(baseUrl: string, workflowId: string): Promise<LoomWorkflowBundle> {
  const response = await getJson<LoomWorkflowBundleResponse>(
    baseUrl,
    `/v1/workflows/${encodeURIComponent(workflowId)}`,
  );
  if (!response.workflow) {
    throw new Error(`Loom 本地服务没有返回工作流 ${workflowId}。`);
  }
  return response.workflow;
}

export async function deleteWorkflowBundle(baseUrl: string, workflowId: string): Promise<void> {
  await deleteJson(baseUrl, `/v1/workflows/${encodeURIComponent(workflowId)}`);
}

export async function saveToolDefinition(
  baseUrl: string,
  tool: LoomToolDefinition,
): Promise<LoomToolDefinition> {
  const response = await putJson<{ tool?: LoomToolDefinition }>(
    baseUrl,
    `/v1/tools/${encodeURIComponent(tool.id)}`,
    tool,
  );
  return response.tool ?? tool;
}

export async function deleteToolDefinition(baseUrl: string, toolId: string): Promise<void> {
  await deleteJson(baseUrl, `/v1/tools/${encodeURIComponent(toolId)}`);
}

export async function uninstallArtPackage(baseUrl: string, artIdentity: string): Promise<void> {
  await postJson(baseUrl, `/v1/arts/${encodeURIComponent(artIdentity)}/uninstall`, {});
}

export async function deleteMcpServer(baseUrl: string, serverId: string): Promise<void> {
  await deleteJson(baseUrl, `/v1/mcp/servers/${encodeURIComponent(serverId)}`);
}

export async function saveMcpServer(baseUrl: string, server: LoomMcpServer): Promise<LoomMcpServer> {
  const response = await putJson<{ server?: LoomMcpServer }>(
    baseUrl,
    `/v1/mcp/servers/${encodeURIComponent(server.id)}`,
    server,
  );
  return response.server ?? server;
}

export async function fetchMcpRegistry(
  baseUrl: string,
  options: {
    search?: string;
    limit?: number;
    cursor?: string | null;
    updatedSince?: string;
    includeDeleted?: boolean;
    refresh?: boolean;
  } = {},
): Promise<McpRegistryResponse> {
  const params = new URLSearchParams();
  if (options.search?.trim()) params.set("search", options.search.trim());
  if (typeof options.limit === "number") params.set("limit", String(options.limit));
  if (options.cursor?.trim()) params.set("cursor", options.cursor.trim());
  params.set("version", "latest");
  if (options.updatedSince?.trim()) params.set("updated_since", options.updatedSince.trim());
  if (options.includeDeleted) params.set("include_deleted", "true");
  if (options.refresh) params.set("refresh", "true");
  const suffix = params.toString();
  return await getJson<McpRegistryResponse>(baseUrl, `/v1/mcp/registry${suffix ? `?${suffix}` : ""}`);
}

export async function testMcpConnection(
  baseUrl: string,
  server: LoomMcpServer,
): Promise<LoomMcpTestResult> {
  return await postJson<LoomMcpTestResult>(baseUrl, "/v1/mcp/test", server);
}

export async function callMcpTool(
  baseUrl: string,
  server: Pick<LoomMcpServer, "transport" | "command" | "args" | "env" | "url" | "headers">,
  toolName: string,
  toolArgs: Record<string, unknown> = {},
): Promise<LoomMcpCallToolResponse> {
  return await postJson<LoomMcpCallToolResponse>(baseUrl, "/v1/mcp/call", {
    transport: server.transport ?? "stdio",
    command: server.command,
    args: server.args ?? [],
    env: server.env ?? {},
    url: server.url ?? "",
    headers: server.headers ?? {},
    toolName,
    toolArgs,
  });
}

export async function checkMcpPackageInstalled(
  baseUrl: string,
  moduleName: string,
): Promise<McpPackageCheckResult> {
  return await postJson<McpPackageCheckResult>(baseUrl, "/v1/mcp/package/check", { moduleName });
}

export async function buildMcpPackageInstallPlan(
  baseUrl: string,
  packageName: string,
): Promise<McpPackageInstallPlan> {
  return await postJson<McpPackageInstallPlan>(baseUrl, "/v1/mcp/package/install-plan", { packageName });
}

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

export async function readLoomSnapshot(baseUrl = DEFAULT_LOOM_DAEMON_URL): Promise<LoomSnapshot> {
  const normalizedBaseUrl = trimTrailingSlash(baseUrl || DEFAULT_LOOM_DAEMON_URL);
  const settings = buildSettingsLinks(normalizedBaseUrl);
  const checkedAt = new Date().toISOString();

  try {
    return await readSnapshotViaTauri(normalizedBaseUrl);
  } catch {
    // Browser previews do not expose Tauri commands. Fall back to direct HTTP so
    // local frontend checks still show the daemon state when CORS allows it.
  }

  let health: LoomHealthResponse;
  let status: LoomStatusResponse;
  try {
    [health, status] = await Promise.all([
      readJson<LoomHealthResponse>(normalizedBaseUrl, "/health"),
      readJson<LoomStatusResponse>(normalizedBaseUrl, "/status"),
    ]);
  } catch (error) {
    return {
      baseUrl: normalizedBaseUrl,
      connectionState: "offline",
      checkedAt,
      health: null,
      status: null,
      capabilities: [],
      mcpServers: [],
      tools: [],
      pythonArts: [],
      workflows: [],
      hookBridge: null,
      settings,
      error: errorMessage(error),
    };
  }

  const [capabilities, mcpServers, tools, pythonArts, workflows, hookBridge] = await Promise.all([
    readOptionalSnapshotArray<LoomCapability>(normalizedBaseUrl, "/v1/capabilities", "capabilities"),
    readOptionalSnapshotArray<LoomMcpServer>(normalizedBaseUrl, "/v1/mcp/servers", "servers"),
    readOptionalSnapshotArray<LoomToolDefinition>(normalizedBaseUrl, "/v1/tools", "tools"),
    readOptionalSnapshotArray<LoomPythonArt>(normalizedBaseUrl, "/v1/art-authoring/python/arts", "arts"),
    readOptionalSnapshotArray<LoomWorkflowMetadata>(normalizedBaseUrl, "/v1/workflows", "workflows"),
    readOptionalSnapshotObject<LoomHookBridgeStatus>(normalizedBaseUrl, "/v1/hook-bridge/status"),
  ]);
  const degradedErrors = [
    capabilities.error,
    mcpServers.error,
    tools.error,
    pythonArts.error,
    workflows.error,
    hookBridge.error,
  ].filter((error): error is string => error !== null);

  if (degradedErrors.length > 0) {
    try {
      await readJson<LoomHealthResponse>(normalizedBaseUrl, "/health");
    } catch (error) {
      return {
        baseUrl: normalizedBaseUrl,
        connectionState: "offline",
        checkedAt,
        health: null,
        status: null,
        capabilities: [],
        mcpServers: [],
        tools: [],
        pythonArts: [],
        workflows: [],
        hookBridge: null,
        settings,
        error: `Loom 本地服务在读取模块状态期间离线：${errorMessage(error)}`,
      };
    }
  }

  return {
    baseUrl: normalizedBaseUrl,
    connectionState: "online",
    checkedAt,
    health,
    status,
    capabilities: capabilities.value,
    mcpServers: mcpServers.value,
    tools: tools.value,
    pythonArts: pythonArts.value,
    workflows: workflows.value,
    hookBridge: hookBridge.value,
    settings,
    error: degradedErrors.length > 0
      ? `Loom 本地服务在线，但部分模块暂不可用：${degradedErrors.join("；")}`
      : null,
  };
}
