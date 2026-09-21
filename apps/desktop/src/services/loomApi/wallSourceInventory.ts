import { readWallSources } from "./wallSources.ts";
import { errorMessage } from "./transport.ts";

export type WallSourceInventory = Awaited<ReturnType<typeof readWallSources>>;

// Retry only this read-only inventory; never reload/rebase the user's layout draft.
export function loadWallSourceInventory(
  baseUrl: string,
  publish: (inventory: WallSourceInventory, pending: boolean) => void,
  read: typeof readWallSources = readWallSources,
): () => void {
  let stopped = false, attempts = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;
  async function load() {
    const next = await read(baseUrl).catch((error: unknown) => ({ sources: [], errors: [errorMessage(error)] }));
    if (stopped) return;
    const retry = next.errors.length > 0 && ++attempts < 3;
    publish(next, retry);
    if (retry && !stopped) timer = setTimeout(() => void load(), 2000);
  }
  void load();
  return () => { stopped = true; clearTimeout(timer); };
}
