import { invoke } from "@tauri-apps/api/core";
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
  name: string;
  description?: string;
  command: string;
  args?: string[];
  env?: Record<string, string>;
  enabled?: boolean;
}

export interface LoomMcpServersResponse {
  servers?: LoomMcpServer[];
}

export interface LoomMcpTestResult {
  compatCommand?: string;
  success?: boolean;
  tools?: unknown[];
  error?: string;
  server_info?: unknown;
  serverInfo?: unknown;
}

export interface ArtLoomMcpRegistryResponse extends McpRegistryResponse {
  compatCommand?: string;
}

export interface ArtLoomMcpServersResponse extends LoomMcpServersResponse {
  compatCommand?: string;
  count?: number;
  message?: string;
  server?: LoomMcpServer;
  serverId?: string;
  deleted?: boolean;
}

export interface ArtLoomMcpCallToolResponse {
  compatCommand?: string;
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

export interface ArtLoomInstalledPythonArtsResponse {
  compatCommand?: string;
  arts?: LoomPythonArt[];
  count?: number;
}

export interface ArtLoomReadPythonFileResponse extends LoomPythonSourceReadResponse {
  compatCommand?: string;
  filePath?: string;
}

export interface ArtLoomReadArtJsonResponse extends LoomPythonArtJsonResponse {
  compatCommand?: string;
}

export interface ArtLoomCheckArtJsonNearbyResponse extends LoomPythonNearbyArtJsonResponse {
  compatCommand?: string;
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

export interface ArtLoomWorkflowMetadata extends LoomWorkflowMetadata {
  compatCommand?: string;
  created_at?: string;
  createdAt?: string;
  updated_at?: string;
  status?: string;
  node_count?: number;
  last_run_at?: string | null;
  lastRunAt?: string | null;
  tags?: string[];
}

export interface ArtLoomWorkflowStoreResponse {
  compatCommand?: string;
  workflows?: ArtLoomWorkflowMetadata[];
  workflow?: ArtLoomWorkflowMetadata;
  workflowId?: string;
  data?: string;
  saved?: boolean;
  deleted?: boolean;
}

export interface LoomHookBridgeStatus {
  compatCommand?: string;
  running?: boolean;
  port?: number;
  ipcPort?: number;
  connectedClients?: number;
  subscribedClients?: number;
  protocol?: string;
  sessionMethod?: string;
  methods?: string[];
}

export interface ArtLoomInstantiateWorkflowResponse {
  compatCommand?: string;
  type?: string;
  method?: string;
  broadcasted?: boolean;
  subscribedClients?: number;
  params?: unknown;
}

export interface ArtLoomExecuteArtNodeResponse {
  compatCommand?: string;
  type?: string;
  data?: unknown;
}

export interface ArtLoomShortcutConfig {
  id: string;
  label: string;
  keys: string;
  enabled: boolean;
}

export interface ArtLoomCompatSettings {
  general: {
    theme: string;
    language: string;
    auto_start: boolean;
    minimize_to_tray: boolean;
    enable_tray_icon: boolean;
  };
  system: {
    auto_check_updates: boolean;
    enable_run_log: boolean;
    run_as_admin: boolean;
    record_screenshot_history: boolean;
    history_retention: string;
    enable_proxy: boolean;
  };
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
  shortcuts: Record<string, ArtLoomShortcutConfig>;
}

export interface ArtLoomAppPaths {
  dataDir: string;
  configDir: string;
  logDir: string;
}

export interface ArtHookSessionSnapshot {
  method?: string;
  compatCommand?: string;
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
  compatCommand?: string;
  installed?: boolean;
  module?: string;
  python?: string;
  stdout?: string;
  stderr?: string;
  error?: string;
}

export interface McpPackageInstallPlan {
  compatCommand?: string;
  package?: string;
  sideEffect?: boolean;
  mode?: string;
  command?: string[];
  message?: string;
}

export interface ArtLoomCompatArt {
  id?: string;
  art_id?: string;
  name?: string;
  label?: string;
  description?: string;
  icon?: unknown;
  enabled?: boolean;
  execution_type?: string;
  execution?: LoomToolExecution;
  inputs?: unknown[];
  outputs?: unknown[];
  params?: unknown[];
  defaults?: Record<string, unknown>;
  metadata?: unknown;
}

export interface ArtLoomCompatArtsResponse {
  compatCommand?: string;
  arts?: ArtLoomCompatArt[];
  tools?: LoomToolDefinition[];
  count?: number;
  synced?: boolean;
  sideEffect?: boolean;
  syncedCount?: number;
  message?: string;
}

export interface ArtLoomCompatArtResponse {
  compatCommand?: string;
  artId?: string;
  enabled?: boolean;
  art?: ArtLoomCompatArt;
  tool?: LoomToolDefinition;
}

export interface ArtLoomNativeProcessArtResponse {
  compatCommand?: string;
  success?: boolean;
  output_base64?: string | null;
  error?: string | null;
  processing_time_ms?: number;
}

export interface ArtLoomPythonExecuteArtResponse {
  compatCommand?: string;
  request_id?: string;
  status?: number;
  data?: unknown;
  error?: unknown;
}

export interface ArtLoomPythonProcessImageResponse {
  compatCommand?: string;
  success?: boolean;
  output_base64?: string | null;
  output_path?: string | null;
  processing_time_ms?: number;
  error?: string | null;
}

export interface PythonEngineStatus {
  compatCommand?: string;
  available?: boolean;
  python_exe?: string;
  pythonExe?: string;
  launcher_path?: string;
  launcherPath?: string | null;
  launcherAvailable?: boolean;
  arts_dir?: string;
  artsDirs?: string[];
  installedArtCount?: number;
}

export interface PythonShaderPrefetchResponse {
  compatCommand?: string;
  artId?: string;
  result?: unknown;
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
  compatCommand?: string;
  handle?: string;
  handle_name?: string;
  buffer?: SharedMemoryBufferInfo;
  buffers?: SharedMemoryBufferInfo[];
  released?: boolean;
  deleted?: boolean;
}

export const DEFAULT_ARTLOOM_COMPAT_SETTINGS: ArtLoomCompatSettings = {
  general: {
    theme: "system",
    language: "zh-Hans",
    auto_start: false,
    minimize_to_tray: true,
    enable_tray_icon: true,
  },
  system: {
    auto_check_updates: true,
    enable_run_log: true,
    run_as_admin: false,
    record_screenshot_history: true,
    history_retention: "7d",
    enable_proxy: false,
  },
  engine: {
    comfyui_url: "http://127.0.0.1:8188",
    python_interpreter: "python.exe",
    virtual_env_path: "./venv",
    compute_device: "0",
    vram_reservation_gb: 12,
  },
  quick_bindings: [{ id: "1", art: "ComfyUI Workflow", key: "Ctrl+Shift+1" }],
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

const trimTrailingSlash = (value: string) => value.replace(/\/+$/, "");

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

  if (!response.ok) {
    throw new Error(`Loom 本地服务请求 ${path} 返回 HTTP ${response.status}`);
  }

  return (await response.json()) as T;
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

const getJsonViaTauri = async <T>(baseUrl: string, path: string): Promise<T> => {
  return await invoke<T>("get_loom_daemon_json", { baseUrl, path });
};

const postJsonViaTauri = async <T>(baseUrl: string, path: string, body: unknown): Promise<T> => {
  return await invoke<T>("post_loom_daemon_json", { baseUrl, path, body });
};

const putJsonViaTauri = async <T>(baseUrl: string, path: string, body: unknown): Promise<T> => {
  return await invoke<T>("put_loom_daemon_json", { baseUrl, path, body });
};

const deleteJsonViaTauri = async <T>(baseUrl: string, path: string): Promise<T> => {
  return await invoke<T>("delete_loom_daemon_json", { baseUrl, path });
};

const getJson = async <T>(baseUrl: string, path: string): Promise<T> => {
  try {
    return await getJsonViaTauri<T>(baseUrl, path);
  } catch {
    // Browser previews do not expose Tauri commands. Fall back to direct HTTP so
    // local frontend checks can still exercise the action when CORS allows it.
  }

  return await readJson<T>(baseUrl, path);
};

export async function getLoomDaemonJson<T>(baseUrl: string, path: string): Promise<T> {
  return await getJson<T>(baseUrl, path);
}

const postJson = async <T>(baseUrl: string, path: string, body: unknown): Promise<T> => {
  try {
    return await postJsonViaTauri<T>(baseUrl, path, body);
  } catch {
    // Browser previews do not expose Tauri commands. Fall back to direct HTTP so
    // local frontend checks can still exercise the action when CORS allows it.
  }

  const response = await fetch(`${trimTrailingSlash(baseUrl)}${path}`, {
    method: "POST",
    headers: {
      Accept: "application/json",
      "Content-Type": "application/json",
    },
    body: JSON.stringify(body ?? {}),
  });

  if (!response.ok) {
    throw new Error(`Loom 本地服务请求 ${path} 返回 HTTP ${response.status}`);
  }

  return (await response.json()) as T;
};

const putJson = async <T>(baseUrl: string, path: string, body: unknown): Promise<T> => {
  try {
    return await putJsonViaTauri<T>(baseUrl, path, body);
  } catch {
    // Browser previews do not expose Tauri commands. Fall back to direct HTTP so
    // local frontend checks can still exercise the action when CORS allows it.
  }

  const response = await fetch(`${trimTrailingSlash(baseUrl)}${path}`, {
    method: "PUT",
    headers: {
      Accept: "application/json",
      "Content-Type": "application/json",
    },
    body: JSON.stringify(body ?? {}),
  });

  if (!response.ok) {
    throw new Error(`Loom 本地服务请求 ${path} 返回 HTTP ${response.status}`);
  }

  return (await response.json()) as T;
};

const deleteJson = async <T>(baseUrl: string, path: string): Promise<T> => {
  try {
    return await deleteJsonViaTauri<T>(baseUrl, path);
  } catch {
    // Browser previews do not expose Tauri commands. Fall back to direct HTTP so
    // local frontend checks can still exercise the action when CORS allows it.
  }

  const response = await fetch(`${trimTrailingSlash(baseUrl)}${path}`, {
    method: "DELETE",
    headers: {
      Accept: "application/json",
    },
  });

  if (!response.ok) {
    throw new Error(`Loom 本地服务请求 ${path} 返回 HTTP ${response.status}`);
  }

  return (await response.json()) as T;
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

export async function readArtHookSession(baseUrl = DEFAULT_LOOM_DAEMON_URL): Promise<ArtHookSessionSnapshot> {
  return await getJson<ArtHookSessionSnapshot>(baseUrl, "/v1/hook-bridge/session");
}

export async function getArtLoomCompatIpcStatus(
  baseUrl = DEFAULT_LOOM_DAEMON_URL,
): Promise<LoomHookBridgeStatus> {
  return await getJson<LoomHookBridgeStatus>(baseUrl, "/v1/artloom-compat/ipc/status");
}

export async function instantiateArtLoomWorkflow(
  baseUrl: string,
  request: {
    nodes: unknown[];
    edges: unknown[];
    mode?: string;
    workflowId?: string | null;
  },
): Promise<ArtLoomInstantiateWorkflowResponse> {
  return await postJson<ArtLoomInstantiateWorkflowResponse>(
    baseUrl,
    "/v1/artloom-compat/ipc/instantiate-workflow",
    request,
  );
}

export async function executeArtLoomArtNode(
  baseUrl: string,
  request: {
    nodeId: string;
    artId: string;
    inputBase64?: string | null;
    params?: Record<string, unknown>;
  },
): Promise<ArtLoomExecuteArtNodeResponse> {
  return await postJson<ArtLoomExecuteArtNodeResponse>(
    baseUrl,
    "/v1/artloom-compat/ipc/execute-art-node",
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

export async function listArtLoomCompatWorkflows(baseUrl: string): Promise<ArtLoomWorkflowStoreResponse> {
  return await getJson<ArtLoomWorkflowStoreResponse>(baseUrl, "/v1/artloom-compat/workflows");
}

export async function saveArtLoomCompatWorkflowMetadata(
  baseUrl: string,
  workflowId: string,
  metadata: Partial<ArtLoomWorkflowMetadata>,
): Promise<ArtLoomWorkflowStoreResponse> {
  return await putJson<ArtLoomWorkflowStoreResponse>(
    baseUrl,
    `/v1/artloom-compat/workflows/${encodeURIComponent(workflowId)}/metadata`,
    { ...metadata, id: metadata.id ?? workflowId },
  );
}

export async function saveArtLoomCompatWorkflowData(
  baseUrl: string,
  workflowId: string,
  data: string,
): Promise<ArtLoomWorkflowStoreResponse> {
  return await putJson<ArtLoomWorkflowStoreResponse>(
    baseUrl,
    `/v1/artloom-compat/workflows/${encodeURIComponent(workflowId)}/data`,
    { data },
  );
}

export async function loadArtLoomCompatWorkflowData(
  baseUrl: string,
  workflowId: string,
): Promise<ArtLoomWorkflowStoreResponse> {
  return await getJson<ArtLoomWorkflowStoreResponse>(
    baseUrl,
    `/v1/artloom-compat/workflows/${encodeURIComponent(workflowId)}/data`,
  );
}

export async function deleteArtLoomCompatWorkflowData(
  baseUrl: string,
  workflowId: string,
): Promise<ArtLoomWorkflowStoreResponse> {
  return await deleteJson<ArtLoomWorkflowStoreResponse>(
    baseUrl,
    `/v1/artloom-compat/workflows/${encodeURIComponent(workflowId)}/data`,
  );
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

export async function listArtLoomCompatArts(baseUrl: string): Promise<ArtLoomCompatArtsResponse> {
  return await getJson<ArtLoomCompatArtsResponse>(baseUrl, "/v1/artloom-compat/arts");
}

export async function listEnabledArtLoomCompatArts(baseUrl: string): Promise<ArtLoomCompatArtsResponse> {
  return await getJson<ArtLoomCompatArtsResponse>(baseUrl, "/v1/artloom-compat/arts/enabled");
}

export async function getArtLoomCompatArt(
  baseUrl: string,
  artId: string,
): Promise<ArtLoomCompatArtResponse> {
  return await getJson<ArtLoomCompatArtResponse>(
    baseUrl,
    `/v1/artloom-compat/arts/${encodeURIComponent(artId)}`,
  );
}

export async function enableArtLoomCompatArt(
  baseUrl: string,
  artId: string,
): Promise<ArtLoomCompatArtResponse> {
  return await postJson<ArtLoomCompatArtResponse>(
    baseUrl,
    `/v1/artloom-compat/arts/${encodeURIComponent(artId)}/enable`,
    {},
  );
}

export async function disableArtLoomCompatArt(
  baseUrl: string,
  artId: string,
): Promise<ArtLoomCompatArtResponse> {
  return await postJson<ArtLoomCompatArtResponse>(
    baseUrl,
    `/v1/artloom-compat/arts/${encodeURIComponent(artId)}/disable`,
    {},
  );
}

export async function updateArtLoomCompatArtDefaults(
  baseUrl: string,
  artId: string,
  defaults: Record<string, unknown>,
): Promise<ArtLoomCompatArtResponse> {
  return await putJson<ArtLoomCompatArtResponse>(
    baseUrl,
    `/v1/artloom-compat/arts/${encodeURIComponent(artId)}/defaults`,
    { defaults },
  );
}

export async function syncArtLoomCompatArts(baseUrl: string): Promise<ArtLoomCompatArtsResponse> {
  return await postJson<ArtLoomCompatArtsResponse>(baseUrl, "/v1/artloom-compat/arts/sync", {});
}

export async function nativeProcessArt(
  baseUrl: string,
  artId: string,
  inputBase64: string,
  params: Record<string, unknown> = {},
): Promise<ArtLoomNativeProcessArtResponse> {
  return await postJson<ArtLoomNativeProcessArtResponse>(
    baseUrl,
    "/v1/artloom-compat/native/process-art",
    { artId, inputBase64, params },
  );
}

export async function executePythonArt(
  baseUrl: string,
  artId: string,
  params: Record<string, unknown> = {},
  artPath?: string,
): Promise<ArtLoomPythonExecuteArtResponse> {
  return await postJson<ArtLoomPythonExecuteArtResponse>(
    baseUrl,
    "/v1/artloom-compat/python/execute-art",
    { artId, params, artPath },
  );
}

export async function pythonProcessImage(
  baseUrl: string,
  artId: string,
  inputBase64: string,
  params: Record<string, unknown> = {},
  artPath?: string,
): Promise<ArtLoomPythonProcessImageResponse> {
  return await postJson<ArtLoomPythonProcessImageResponse>(
    baseUrl,
    "/v1/artloom-compat/python/process-image",
    { artId, inputBase64, params, artPath },
  );
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

export async function listArtLoomMcpServers(baseUrl: string): Promise<ArtLoomMcpServersResponse> {
  return await getJson<ArtLoomMcpServersResponse>(baseUrl, "/v1/artloom-compat/mcp/servers");
}

export async function saveArtLoomMcpServer(
  baseUrl: string,
  server: LoomMcpServer,
): Promise<ArtLoomMcpServersResponse> {
  return await postJson<ArtLoomMcpServersResponse>(baseUrl, "/v1/artloom-compat/mcp/servers", server);
}

export async function deleteArtLoomMcpServer(
  baseUrl: string,
  serverId: string,
): Promise<ArtLoomMcpServersResponse> {
  return await deleteJson<ArtLoomMcpServersResponse>(
    baseUrl,
    `/v1/artloom-compat/mcp/servers/${encodeURIComponent(serverId)}`,
  );
}

export async function fetchMcpRegistry(
  baseUrl: string,
  options: { search?: string; limit?: number; cursor?: string | null } = {},
): Promise<McpRegistryResponse> {
  const params = new URLSearchParams();
  if (options.search?.trim()) params.set("search", options.search.trim());
  if (typeof options.limit === "number") params.set("limit", String(options.limit));
  if (options.cursor?.trim()) params.set("cursor", options.cursor.trim());
  const suffix = params.toString();
  return await getJson<McpRegistryResponse>(baseUrl, `/v1/mcp/registry${suffix ? `?${suffix}` : ""}`);
}

export async function fetchArtLoomMcpRegistry(
  baseUrl: string,
  options: { search?: string; limit?: number; cursor?: string | null } = {},
): Promise<ArtLoomMcpRegistryResponse> {
  const params = new URLSearchParams();
  if (options.search?.trim()) params.set("search", options.search.trim());
  if (typeof options.limit === "number") params.set("limit", String(options.limit));
  if (options.cursor?.trim()) params.set("cursor", options.cursor.trim());
  const suffix = params.toString();
  return await getJson<ArtLoomMcpRegistryResponse>(
    baseUrl,
    `/v1/artloom-compat/mcp/registry${suffix ? `?${suffix}` : ""}`,
  );
}

export async function testMcpConnection(
  baseUrl: string,
  server: LoomMcpServer,
): Promise<LoomMcpTestResult> {
  return await postJson<LoomMcpTestResult>(baseUrl, "/v1/mcp/test", server);
}

export async function callMcpTool(
  baseUrl: string,
  server: Pick<LoomMcpServer, "command" | "args" | "env">,
  toolName: string,
  toolArgs: Record<string, unknown> = {},
): Promise<ArtLoomMcpCallToolResponse> {
  return await postJson<ArtLoomMcpCallToolResponse>(baseUrl, "/v1/artloom-compat/mcp/call-tool", {
    command: server.command,
    args: server.args ?? [],
    env: server.env ?? {},
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

export async function getPythonEngineStatus(baseUrl: string): Promise<PythonEngineStatus> {
  return await getJson<PythonEngineStatus>(baseUrl, "/v1/python-arts/engine/status");
}

export async function prefetchPythonArtShader(
  baseUrl: string,
  artId: string,
  artPath?: string,
): Promise<PythonShaderPrefetchResponse> {
  return await postJson<PythonShaderPrefetchResponse>(baseUrl, "/v1/python-arts/shader/prefetch", {
    artId,
    artPath,
    params: { mode: "shader", output_mode: "shader" },
  });
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

export async function getArtLoomCompatSettings(
  baseUrl: string,
): Promise<ArtLoomCompatSettings> {
  const response = await getJson<{ settings?: ArtLoomCompatSettings }>(baseUrl, "/v1/artloom-compat/settings");
  return response.settings ?? DEFAULT_ARTLOOM_COMPAT_SETTINGS;
}

export async function saveArtLoomCompatSettings(
  baseUrl: string,
  settings: ArtLoomCompatSettings,
): Promise<ArtLoomCompatSettings> {
  const response = await putJson<{ settings?: ArtLoomCompatSettings }>(
    baseUrl,
    "/v1/artloom-compat/settings",
    settings,
  );
  return response.settings ?? settings;
}

export async function getArtLoomCompatShortcuts(
  baseUrl: string,
): Promise<ArtLoomShortcutConfig[]> {
  const response = await getJson<{ shortcuts?: ArtLoomShortcutConfig[] }>(
    baseUrl,
    "/v1/artloom-compat/shortcuts",
  );
  return response.shortcuts ?? Object.values(DEFAULT_ARTLOOM_COMPAT_SETTINGS.shortcuts);
}

export async function updateArtLoomCompatShortcut(
  baseUrl: string,
  shortcut: ArtLoomShortcutConfig,
): Promise<ArtLoomShortcutConfig> {
  const response = await putJson<{ shortcut?: ArtLoomShortcutConfig }>(
    baseUrl,
    `/v1/artloom-compat/shortcuts/${encodeURIComponent(shortcut.id)}`,
    shortcut,
  );
  return response.shortcut ?? shortcut;
}

export async function getArtLoomCompatAppPaths(baseUrl: string): Promise<ArtLoomAppPaths> {
  return await getJson<ArtLoomAppPaths>(baseUrl, "/v1/artloom-compat/app-paths");
}

export async function getArtLoomCompatAutostart(
  baseUrl: string,
): Promise<{ enabled?: boolean; sideEffect?: boolean; mode?: string }> {
  return await getJson<{ enabled?: boolean; sideEffect?: boolean; mode?: string }>(
    baseUrl,
    "/v1/artloom-compat/system/autostart",
  );
}

export async function setArtLoomCompatAutostart(
  baseUrl: string,
  enabled: boolean,
): Promise<{ enabled?: boolean; sideEffect?: boolean; mode?: string }> {
  return await postJson<{ enabled?: boolean; sideEffect?: boolean; mode?: string }>(
    baseUrl,
    "/v1/artloom-compat/system/autostart",
    { enabled },
  );
}

export async function enableArtLoomCompatAutostart(
  baseUrl: string,
): Promise<{ enabled?: boolean; sideEffect?: boolean; mode?: string }> {
  return await postJson<{ enabled?: boolean; sideEffect?: boolean; mode?: string }>(
    baseUrl,
    "/v1/artloom-compat/system/autostart/enable",
    {},
  );
}

export async function disableArtLoomCompatAutostart(
  baseUrl: string,
): Promise<{ enabled?: boolean; sideEffect?: boolean; mode?: string }> {
  return await postJson<{ enabled?: boolean; sideEffect?: boolean; mode?: string }>(
    baseUrl,
    "/v1/artloom-compat/system/autostart/disable",
    {},
  );
}

export async function setArtLoomCompatMinimizeToTray(
  baseUrl: string,
  enabled: boolean,
): Promise<{ enabled?: boolean; sideEffect?: boolean; mode?: string }> {
  return await postJson<{ enabled?: boolean; sideEffect?: boolean; mode?: string }>(
    baseUrl,
    "/v1/artloom-compat/system/minimize-to-tray",
    { enabled },
  );
}

export async function readPythonArtSource(
  baseUrl: string,
  path: string,
): Promise<LoomPythonSourceReadResponse> {
  return await postJson<LoomPythonSourceReadResponse>(baseUrl, "/v1/python-arts/source/read", { path });
}

export async function listArtLoomInstalledPythonArts(
  baseUrl: string,
): Promise<ArtLoomInstalledPythonArtsResponse> {
  return await getJson<ArtLoomInstalledPythonArtsResponse>(baseUrl, "/v1/artloom-compat/python/installed-arts");
}

export async function readArtLoomPythonFile(
  baseUrl: string,
  filePath: string,
): Promise<ArtLoomReadPythonFileResponse> {
  return await postJson<ArtLoomReadPythonFileResponse>(
    baseUrl,
    "/v1/artloom-compat/python/read-python-file",
    { filePath },
  );
}

export async function readPythonArtJson(
  baseUrl: string,
  artPath: string,
): Promise<LoomPythonArtJsonResponse> {
  return await postJson<LoomPythonArtJsonResponse>(baseUrl, "/v1/python-arts/source/read-art-json", { artPath });
}

export async function readArtLoomArtJson(
  baseUrl: string,
  artPath: string,
): Promise<ArtLoomReadArtJsonResponse> {
  return await postJson<ArtLoomReadArtJsonResponse>(
    baseUrl,
    "/v1/artloom-compat/python/read-art-json",
    { artPath },
  );
}

export async function checkPythonArtJsonNearby(
  baseUrl: string,
  pythonPath: string,
): Promise<LoomPythonNearbyArtJsonResponse> {
  return await postJson<LoomPythonNearbyArtJsonResponse>(
    baseUrl,
    "/v1/python-arts/source/check-art-json",
    { pythonPath },
  );
}

export async function checkArtLoomArtJsonNearby(
  baseUrl: string,
  pythonPath: string,
): Promise<ArtLoomCheckArtJsonNearbyResponse> {
  return await postJson<ArtLoomCheckArtJsonNearbyResponse>(
    baseUrl,
    "/v1/artloom-compat/python/check-art-json-nearby",
    { pythonPath },
  );
}

export async function inferPythonArtPorts(
  baseUrl: string,
  request: { path?: string; code?: string },
): Promise<LoomPythonPortInferenceResponse> {
  return await postJson<LoomPythonPortInferenceResponse>(
    baseUrl,
    "/v1/python-arts/source/infer-ports",
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
    readOptionalSnapshotArray<LoomPythonArt>(normalizedBaseUrl, "/v1/python-arts", "arts"),
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
