// Bound actual native reads, including reads whose UI subscriber has gone away.
export function createHookCanvasPreviewLoader<T>(
  read: (baseUrl: string, path: string) => Promise<T>,
  options: { concurrency?: number; maxPending?: number; sleep?: (ms: number, signal?: AbortSignal) => Promise<void> } = {},
) {
  const concurrency = options.concurrency ?? 4;
  const maxPending = options.maxPending ?? 4096;
  if (!Number.isInteger(concurrency) || concurrency < 1 || !Number.isInteger(maxPending) || maxPending < 1) {
    throw new Error("Invalid preview queue limits");
  }
  const sleep = options.sleep ?? abortableDelay;
  interface Job {
    baseUrl: string;
    path: string;
    signal?: AbortSignal;
    resolve: (value: T) => void;
    reject: (error: unknown) => void;
    detach: () => void;
  }
  const queued: Job[] = [];
  let active = 0;

  async function execute(job: Job) {
    try {
      for (let attempt = 0; ; attempt++) {
        if (job.signal?.aborted) throw aborted();
        try {
          const value = await read(job.baseUrl, job.path);
          if (job.signal?.aborted) throw aborted();
          job.resolve(value);
          return;
        } catch (error) {
          if (job.signal?.aborted || attempt >= 2 || !isTransientPreviewError(error)) throw error;
          await sleep(150 * (attempt + 1), job.signal);
        }
      }
    } catch (error) {
      job.reject(error);
    } finally {
      job.detach();
      active--;
      pump();
    }
  }

  function pump() {
    while (active < concurrency && queued.length) {
      const job = queued.shift()!;
      if (job.signal?.aborted) { job.detach(); job.reject(aborted()); continue; }
      active++;
      void execute(job);
    }
  }

  return {
    load(baseUrl: string, path: string, signal?: AbortSignal): Promise<T> {
      if (signal?.aborted) return Promise.reject(aborted());
      if (queued.length >= maxPending) return Promise.reject(new Error("Hook preview queue is full"));
      return new Promise<T>((resolve, reject) => {
        const job: Job = { baseUrl, path, signal, resolve, reject, detach: () => signal?.removeEventListener("abort", cancel) };
        function cancel() {
          const index = queued.indexOf(job);
          if (index >= 0) { queued.splice(index, 1); job.detach(); }
          // An active native invoke cannot be cancelled. Its slot is retained until it ends.
          reject(aborted());
        }
        signal?.addEventListener("abort", cancel, { once: true });
        queued.push(job);
        pump();
      });
    },
  };
}

function aborted() { return new DOMException("Aborted", "AbortError"); }

function isTransientPreviewError(error: unknown): boolean {
  const message = error instanceof Error ? error.message : String(error);
  return /\bHTTP(?:\/\d(?:\.\d)?)?\s+(?:429|502|503|504)\b|timed? out|timeout|connection (?:reset|refused)|无法连接/i.test(message);
}

function abortableDelay(ms: number, signal?: AbortSignal): Promise<void> {
  if (signal?.aborted) return Promise.reject(aborted());
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => { signal?.removeEventListener("abort", cancel); resolve(); }, ms);
    function cancel() { clearTimeout(timer); reject(aborted()); }
    signal?.addEventListener("abort", cancel, { once: true });
  });
}
