// A shared read has no UI owner: each waiting caller owns its own cancellation/token.
import type { LatestRequestGate, SingleFlightGate } from "./latestRequest.ts";

export async function refreshSharedSnapshot<T>(options: {
  latest: LatestRequestGate;
  flight: SingleFlightGate;
  read: () => Promise<T>;
  apply: (value: T) => void;
  loading: (value: boolean) => void;
  signal?: AbortSignal;
}): Promise<T> {
  const { latest, flight, read, apply, loading, signal } = options;
  if (signal?.aborted) throw new DOMException("Aborted", "AbortError");
  const token = latest.begin();
  const abort = () => {
    if (latest.isCurrent(token)) {
      latest.invalidate();
      loading(false);
    }
  };
  signal?.addEventListener("abort", abort, { once: true });
  loading(true);
  try {
    // Do not capture the first caller's signal in the flight. A native Tauri read cannot
    // be aborted; a later readiness attempt must still be able to apply its result.
    const value = await flight.run(read);
    if (!signal?.aborted && latest.isCurrent(token)) apply(value);
    return value;
  } finally {
    signal?.removeEventListener("abort", abort);
    if (latest.isCurrent(token)) loading(false);
  }
}
