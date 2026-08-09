import { createHookBridgeBrowserClient, type HookBridgeBrowserClient } from "./hookBridgeBrowserClient.ts";

export const HOOK_LIVE_WORKFLOW_ID = "hook-live";
const LEGACY_HOOK_LIVE_WORKFLOW_ID = "arthook-live";

export interface HookBridgeWorkflowSyncOptions {
  client?: HookBridgeBrowserClient;
  websocketUrl?: string;
  refresh: () => Promise<unknown> | unknown;
  invalidateHookCanvas: () => void;
  debounceMs?: number;
}

export interface HookBridgeWorkflowSyncHandle {
  dispose(): void;
}

function isHookLiveWorkflowId(value: unknown): boolean {
  return value === HOOK_LIVE_WORKFLOW_ID || value === LEGACY_HOOK_LIVE_WORKFLOW_ID;
}

function workflowIdFromPayload(payload: unknown): unknown {
  if (!payload || typeof payload !== "object") return undefined;
  if ("workflowId" in payload) return (payload as { workflowId?: unknown }).workflowId;
  if ("workflow_id" in payload) return (payload as { workflow_id?: unknown }).workflow_id;
  return undefined;
}

export function startHookBridgeWorkflowSync(
  options: HookBridgeWorkflowSyncOptions,
): HookBridgeWorkflowSyncHandle {
  const client = options.client ?? createHookBridgeBrowserClient({ url: options.websocketUrl });
  const debounceMs = Math.max(0, options.debounceMs ?? 50);
  let pendingTimer: ReturnType<typeof setTimeout> | null = null;
  let disposed = false;

  const scheduleRefresh = () => {
    if (disposed) return;
    if (pendingTimer !== null) clearTimeout(pendingTimer);
    pendingTimer = setTimeout(() => {
      pendingTimer = null;
      void Promise.resolve()
        .then(() => options.refresh())
        .then(() => {
          options.invalidateHookCanvas();
        })
        .catch(() => {
          options.invalidateHookCanvas();
        });
    }, debounceMs);
  };

  const stopWorkflowUpdated = client.subscribe("art_loom/workflow_updated", (payload) => {
    const workflowId = workflowIdFromPayload(payload);
    if (workflowId !== undefined && !isHookLiveWorkflowId(workflowId)) {
      return;
    }
    scheduleRefresh();
  });

  const stopArtsUpdated = client.subscribe("art_loom/arts_updated", () => {
    scheduleRefresh();
  });

  return {
    dispose() {
      disposed = true;
      if (pendingTimer !== null) clearTimeout(pendingTimer);
      pendingTimer = null;
      stopArtsUpdated();
      stopWorkflowUpdated();
      client.dispose();
    },
  };
}
