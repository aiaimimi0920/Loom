import { invoke, isTauri } from "@tauri-apps/api/core";

type Handler = (payload: unknown) => void;
type Timer = number | ReturnType<typeof setTimeout>;
export interface HookBridgeBrowserClient {
  subscribe(event: string, handler: Handler): () => void;
  dispose(): void;
}

export interface HookBridgeSubscriptionState {
  connected: boolean;
  epoch: string;
  workflowRevision: string;
  capabilitiesRevision: string;
}

interface Options {
  readState?: () => Promise<HookBridgeSubscriptionState>;
  schedule?: (callback: () => void, delayMs: number) => Timer;
  cancel?: (handle: Timer) => void;
}

const WORKFLOW = "loom.hook.workflow.updated";
const CAPABILITIES = "loom.hook.capabilities.updated";

// The historical facade now polls a fixed native subscription. No URL, credentials,
// raw messages or arbitrary methods cross the WebView boundary.
export function createHookBridgeBrowserClient(options: Options = {}): HookBridgeBrowserClient {
  const read = options.readState ?? (typeof window !== "undefined" && isTauri()
    ? () => invoke<HookBridgeSubscriptionState>("read_hook_bridge_subscription_state")
    : null);
  const schedule = options.schedule ?? setTimeout;
  const cancel = options.cancel ?? clearTimeout;
  const handlers = new Map<string, Set<Handler>>();
  let timer: Timer | null = null;
  let previous: HookBridgeSubscriptionState | null = null;
  let disposed = false;
  let generation = 0;
  let inFlight = false;

  const emit = (method: string, ownGeneration: number) => {
    for (const handler of [...(handlers.get(method) ?? [])]) {
      if (disposed || ownGeneration !== generation || !handlers.get(method)?.has(handler)) continue;
      try { handler(method === WORKFLOW ? { workflowId: "hook-live" } : {}); }
      catch { /* One UI listener must not stop subscription recovery. */ }
    }
  };
  const poll = async () => {
    if (!read || disposed || handlers.size === 0 || inFlight) return;
    inFlight = true;
    const ownGeneration = generation;
    try {
      const current = await read();
      if (disposed || ownGeneration !== generation) return;
      if (current.connected) {
        // A new connection refreshes both snapshots to cover its disconnected gap.
        const reset = !previous?.connected || previous.epoch !== current.epoch;
        if (reset || previous?.workflowRevision !== current.workflowRevision) emit(WORKFLOW, ownGeneration);
        if (reset || previous?.capabilitiesRevision !== current.capabilitiesRevision) emit(CAPABILITIES, ownGeneration);
      }
      if (ownGeneration === generation) previous = current;
    } catch {
      if (ownGeneration === generation) previous = null;
    } finally {
      inFlight = false;
      if (!disposed && handlers.size > 0) {
        timer = schedule(() => { timer = null; void poll(); }, 250);
      }
    }
  };
  const stopPolling = () => {
    generation += 1;
    previous = null;
    if (timer !== null) cancel(timer);
    timer = null;
  };
  return {
    subscribe(event, handler) {
      if (disposed || (event !== WORKFLOW && event !== CAPABILITIES)) return () => {};
      const listeners = handlers.get(event) ?? new Set<Handler>();
      listeners.add(handler);
      handlers.set(event, listeners);
      if (timer === null) void poll();
      let stopped = false;
      return () => {
        if (stopped) return;
        stopped = true;
        listeners.delete(handler);
        if (listeners.size === 0 && handlers.get(event) === listeners) handlers.delete(event);
        if (handlers.size === 0) stopPolling();
      };
    },
    dispose() {
      disposed = true;
      handlers.clear();
      stopPolling();
    },
  };
}
