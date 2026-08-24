// Owns daemon reads and refresh-key derivation for live and frozen Hook canvases.

import { getLoomDaemonJson } from "../loomApi.ts";
import type {
  CanvasWorkflowSummary,
  HookCanvasRefreshTriggerInput,
  HookCanvasSnapshot,
} from "./types.ts";

interface CanvasWorkflowListResponse {
  workflows?: CanvasWorkflowSummary[];
}

export function getHookCanvasRefreshTrigger(input: HookCanvasRefreshTriggerInput): string | null {
  if (input.connectionState !== "online") return null;
  return JSON.stringify([input.baseUrl, input.refreshVersion]);
}

export async function readHookCanvasSnapshot(baseUrl: string): Promise<HookCanvasSnapshot> {
  return await getLoomDaemonJson<HookCanvasSnapshot>(baseUrl, "/v1/hook-bridge/canvas");
}

export async function listCanvasWorkflows(baseUrl: string): Promise<CanvasWorkflowSummary[]> {
  const response = await getLoomDaemonJson<CanvasWorkflowListResponse>(
    baseUrl,
    "/v1/hook-bridge/canvas/workflows",
  );
  return Array.isArray(response.workflows) ? response.workflows : [];
}

export async function readCanvasWorkflowSnapshot(
  baseUrl: string,
  workflowId: string,
): Promise<HookCanvasSnapshot> {
  return await getLoomDaemonJson<HookCanvasSnapshot>(
    baseUrl,
    `/v1/hook-bridge/canvas/workflows/${encodeURIComponent(workflowId)}`,
  );
}
