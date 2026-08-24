// Aggregated daemon snapshot, startup, settings-link, and online-wait contracts.
import type {
  ConnectionState,
  LoomCapability,
  LoomHealthResponse,
  LoomStatusResponse,
  LoomToolDefinition,
  LoomWorkflowMetadata,
} from "./coreTypes.ts";
import type { LoomHookBridgeStatus } from "./hookTypes.ts";
import type { LoomMcpServer } from "./mcpTypes.ts";
import type { LoomPythonArt } from "./pythonTypes.ts";

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
