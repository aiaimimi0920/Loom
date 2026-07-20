import { createHookBridgeBrowserClient, type HookBridgeBrowserClient } from "./hookBridgeBrowserClient.ts";

export const HOOK_LIVE_WORKFLOW_ID = "hook-live";
const LEGACY_HOOK_LIVE_WORKFLOW_ID = "arthook-live";

export interface HookBridgeWorkflowSyncOptions {
  client?: HookBridgeBrowserClient;
  refresh: () => Promise<unknown> | unknown;
  openHookWorkflow: () => void;
}

export interface HookBridgeWorkflowSyncHandle {
  dispose(): void;
}

function isHookLiveWorkflowId(value: unknown): boolean {
  return value === HOOK_LIVE_WORKFLOW_ID || value === LEGACY_HOOK_LIVE_WORKFLOW_ID;
}

export function startHookBridgeWorkflowSync(
  options: HookBridgeWorkflowSyncOptions,
): HookBridgeWorkflowSyncHandle {
  const client = options.client ?? createHookBridgeBrowserClient();

  const refreshHookLiveWorkflow = () => {
    void Promise.resolve(options.refresh()).then(() => {
      options.openHookWorkflow();
    });
  };

  const stopInstantiate = client.subscribe("art_hook/instantiate", () => {
    refreshHookLiveWorkflow();
  });

  const stopWorkflowUpdated = client.subscribe("art_loom/workflow_updated", (payload) => {
    const workflowId =
      payload && typeof payload === "object" && "workflowId" in payload
        ? (payload as { workflowId?: unknown }).workflowId
        : undefined;
    if (workflowId !== undefined && !isHookLiveWorkflowId(workflowId)) {
      return;
    }
    refreshHookLiveWorkflow();
  });

  const stopArtsUpdated = client.subscribe("art_loom/arts_updated", () => {
    refreshHookLiveWorkflow();
  });

  return {
    dispose() {
      stopArtsUpdated();
      stopWorkflowUpdated();
      stopInstantiate();
      client.dispose();
    },
  };
}
