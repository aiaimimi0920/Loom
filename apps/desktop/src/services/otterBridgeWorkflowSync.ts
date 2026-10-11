import { createOtterBridgeBrowserClient, type OtterBridgeBrowserClient } from "./otterBridgeBrowserClient.ts";

export const HOOK_LIVE_WORKFLOW_ID = "hook-live";

export interface OtterBridgeWorkflowSyncOptions {
  client?: OtterBridgeBrowserClient;
  refresh: () => Promise<unknown> | unknown;
  invalidateOtterCanvas: () => void;
  debounceMs?: number;
}

export interface OtterBridgeWorkflowSyncHandle {
  dispose(): void;
}

function isOtterLiveWorkflowId(value: unknown): boolean {
  return value === HOOK_LIVE_WORKFLOW_ID;
}

function workflowIdFromPayload(payload: unknown): unknown {
  if (!payload || typeof payload !== "object") return undefined;
  if ("workflowId" in payload) return (payload as { workflowId?: unknown }).workflowId;
  return undefined;
}

export function startOtterBridgeWorkflowSync(
  options: OtterBridgeWorkflowSyncOptions,
): OtterBridgeWorkflowSyncHandle {
  const client = options.client ?? createOtterBridgeBrowserClient();
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
          if (!disposed) options.invalidateOtterCanvas();
        })
        .catch(() => {
          if (!disposed) options.invalidateOtterCanvas();
        });
    }, debounceMs);
  };

  const stopWorkflowUpdated = client.subscribe("loom.hook.workflow.updated", (payload) => {
    const workflowId = workflowIdFromPayload(payload);
    if (workflowId !== undefined && !isOtterLiveWorkflowId(workflowId)) {
      return;
    }
    scheduleRefresh();
  });

  const stopArtsUpdated = client.subscribe("loom.hook.capabilities.updated", () => {
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
