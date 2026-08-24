// Online polling, partial-data retention, and aggregate daemon snapshot orchestration.
import { invoke } from "@tauri-apps/api/core";
import { DEFAULT_LOOM_DAEMON_URL } from "./defaults.ts";
import type {
  LoomCapability,
  LoomHealthResponse,
  LoomStatusResponse,
  LoomToolDefinition,
  LoomWorkflowMetadata,
} from "./coreTypes.ts";
import type { LoomHookBridgeStatus } from "./hookTypes.ts";
import type { LoomMcpServer } from "./mcpTypes.ts";
import type { LoomPythonArt } from "./pythonTypes.ts";
import type { LoomOnlineWaitOptions, LoomSettingsLinks, LoomSnapshot } from "./snapshotTypes.ts";
import { errorMessage, readJson, trimTrailingSlash } from "./transport.ts";

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
      try {
        onTimeout?.();
      } catch {
        // Observability callbacks must not break the timeout control path.
      }
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

const buildSettingsLinks = (baseUrl: string): LoomSettingsLinks => {
  const root = `${trimTrailingSlash(baseUrl)}/settings`;
  return {
    root,
    tea: `${trimTrailingSlash(baseUrl)}/settings/tea`,
    hook: `${trimTrailingSlash(baseUrl)}/settings/hook`,
    talk: `${trimTrailingSlash(baseUrl)}/settings/talk`,
  };
};

interface OptionalSnapshotValue<T> {
  value: T;
  error: string | null;
}

const readOptionalSnapshotArray = async <T>(
  baseUrl: string,
  path: string,
  key: string,
  signal?: AbortSignal,
): Promise<OptionalSnapshotValue<T[]>> => {
  try {
    const response = await readJson<Record<string, unknown>>(baseUrl, path, signal);
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
  signal?: AbortSignal,
): Promise<OptionalSnapshotValue<T | null>> => {
  try {
    const response = await readJson<unknown>(baseUrl, path, signal);
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

export async function readLoomSnapshot(
  baseUrl = DEFAULT_LOOM_DAEMON_URL,
  signal?: AbortSignal,
): Promise<LoomSnapshot> {
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
      readJson<LoomHealthResponse>(normalizedBaseUrl, "/health", signal),
      readJson<LoomStatusResponse>(normalizedBaseUrl, "/status", signal),
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
    readOptionalSnapshotArray<LoomCapability>(
      normalizedBaseUrl,
      "/v1/capabilities",
      "capabilities",
      signal,
    ),
    readOptionalSnapshotArray<LoomMcpServer>(
      normalizedBaseUrl,
      "/v1/mcp/servers",
      "servers",
      signal,
    ),
    readOptionalSnapshotArray<LoomToolDefinition>(normalizedBaseUrl, "/v1/tools", "tools", signal),
    readOptionalSnapshotArray<LoomPythonArt>(
      normalizedBaseUrl,
      "/v1/art-authoring/python/arts",
      "arts",
      signal,
    ),
    readOptionalSnapshotArray<LoomWorkflowMetadata>(
      normalizedBaseUrl,
      "/v1/workflows",
      "workflows",
      signal,
    ),
    readOptionalSnapshotObject<LoomHookBridgeStatus>(
      normalizedBaseUrl,
      "/v1/hook-bridge/status",
      signal,
    ),
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
      await readJson<LoomHealthResponse>(normalizedBaseUrl, "/health", signal);
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
