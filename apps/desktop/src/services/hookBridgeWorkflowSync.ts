import { createHookBridgeBrowserClient, type HookBridgeBrowserClient } from "./hookBridgeBrowserClient.ts";

export const HOOK_LIVE_WORKFLOW_ID = "hook-live";
const LEGACY_HOOK_LIVE_WORKFLOW_ID = "arthook-live";

export interface HookBridgeWorkflowSyncOptions {
  client?: HookBridgeBrowserClient;
  refresh: () => Promise<unknown> | unknown;
  invalidateHookCanvas: () => void;
  openHookWorkflow: () => void;
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
  const client = options.client ?? createHookBridgeBrowserClient();
  const debounceMs = Math.max(0, options.debounceMs ?? 50);
  let pendingTimer: ReturnType<typeof setTimeout> | null = null;
  let pendingOpen = false;
  let disposed = false;

  const scheduleRefresh = (openWorkflow: boolean) => {
    if (disposed) return;
    pendingOpen = pendingOpen || openWorkflow;
    if (pendingTimer !== null) clearTimeout(pendingTimer);
    pendingTimer = setTimeout(() => {
      pendingTimer = null;
      const shouldOpen = pendingOpen;
      pendingOpen = false;
      void Promise.resolve()
        .then(() => options.refresh())
        .then(() => {
          options.invalidateHookCanvas();
          if (shouldOpen && !disposed) options.openHookWorkflow();
        })
        .catch(() => {
          options.invalidateHookCanvas();
        });
    }, debounceMs);
  };

  const stopInstantiate = client.subscribe("art_hook/instantiate", () => {
    scheduleRefresh(true);
  });

  const stopWorkflowUpdated = client.subscribe("art_loom/workflow_updated", (payload) => {
    const workflowId = workflowIdFromPayload(payload);
    if (workflowId !== undefined && !isHookLiveWorkflowId(workflowId)) {
      return;
    }
    scheduleRefresh(false);
  });

  const stopArtsUpdated = client.subscribe("art_loom/arts_updated", () => {
    scheduleRefresh(false);
  });

  return {
    dispose() {
      disposed = true;
      if (pendingTimer !== null) clearTimeout(pendingTimer);
      pendingTimer = null;
      pendingOpen = false;
      stopArtsUpdated();
      stopWorkflowUpdated();
      stopInstantiate();
      client.dispose();
    },
  };
}
