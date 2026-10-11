import {
  getOtterBridgeStatus,
  startOtterBridge,
  type LoomOtterBridgeStatus,
} from "./loomApi.ts";

const RETRY_DELAY_MS = 1_000;
const HEALTHY_POLL_DELAY_MS = 5_000;

type Timer = ReturnType<typeof setTimeout>;

interface OtterBridgeAvailabilityDependencies {
  readStatus: (baseUrl: string) => Promise<LoomOtterBridgeStatus>;
  start: (baseUrl: string, port?: number) => Promise<LoomOtterBridgeStatus>;
  schedule: (callback: () => void, delayMs: number) => Timer;
  clear: (timer: Timer) => void;
}

export interface OtterBridgeAvailabilityOptions {
  baseUrl: string;
  websocketUrl: string;
}

export interface OtterBridgeAvailabilityHandle {
  dispose(): void;
}

const defaultDependencies: OtterBridgeAvailabilityDependencies = {
  readStatus: getOtterBridgeStatus,
  start: startOtterBridge,
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

/** Keeps the daemon-owned Otter bridge available after transient startup failures. */
export function maintainOtterBridgeAvailability(
  options: OtterBridgeAvailabilityOptions,
  dependencies: OtterBridgeAvailabilityDependencies = defaultDependencies,
): OtterBridgeAvailabilityHandle {
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
