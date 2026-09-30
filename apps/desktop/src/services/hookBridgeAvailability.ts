import {
  getHookBridgeStatus,
  startHookBridge,
  type LoomHookBridgeStatus,
} from "./loomApi.ts";

const RETRY_DELAY_MS = 1_000;
const HEALTHY_POLL_DELAY_MS = 5_000;

type Timer = ReturnType<typeof setTimeout>;

interface HookBridgeAvailabilityDependencies {
  readStatus: (baseUrl: string) => Promise<LoomHookBridgeStatus>;
  start: (baseUrl: string, port?: number) => Promise<LoomHookBridgeStatus>;
  schedule: (callback: () => void, delayMs: number) => Timer;
  clear: (timer: Timer) => void;
}

export interface HookBridgeAvailabilityOptions {
  baseUrl: string;
  websocketUrl: string;
}

export interface HookBridgeAvailabilityHandle {
  dispose(): void;
}

const defaultDependencies: HookBridgeAvailabilityDependencies = {
  readStatus: getHookBridgeStatus,
  start: startHookBridge,
  schedule: (callback, delayMs) => setTimeout(callback, delayMs),
  clear: (timer) => clearTimeout(timer),
};

const bridgePortFromUrl = (url: string): number | undefined => {
  try {
    const parsed = Number(new URL(url).port);
    return Number.isInteger(parsed) && parsed > 0 ? parsed : undefined;
  } catch {
    return undefined;
  }
};

/** Keeps the daemon-owned Hook bridge available after transient startup failures. */
export function maintainHookBridgeAvailability(
  options: HookBridgeAvailabilityOptions,
  dependencies: HookBridgeAvailabilityDependencies = defaultDependencies,
): HookBridgeAvailabilityHandle {
  const port = bridgePortFromUrl(options.websocketUrl);
  let disposed = false;
  let timer: Timer | null = null;

  const scheduleCheck = (delayMs: number) => {
    if (disposed) return;
    timer = dependencies.schedule(() => {
      timer = null;
      void check();
    }, delayMs);
  };

  const check = async () => {
    try {
      const status = await dependencies.readStatus(options.baseUrl);
      if (disposed) return;
      if (!status.running) await dependencies.start(options.baseUrl, port);
      scheduleCheck(HEALTHY_POLL_DELAY_MS);
    } catch {
      scheduleCheck(RETRY_DELAY_MS);
    }
  };

  void check();
  return {
    dispose() {
      disposed = true;
      if (timer !== null) dependencies.clear(timer);
      timer = null;
    },
  };
}
