export interface LatestRequestGate {
  begin(): number;
  invalidate(): void;
  isCurrent(token: number): boolean;
}

export function createLatestRequestGate(): LatestRequestGate {
  let latestToken = 0;

  return {
    begin() {
      latestToken += 1;
      return latestToken;
    },
    invalidate() {
      latestToken += 1;
    },
    isCurrent(token) {
      return token === latestToken;
    },
  };
}

export interface SingleFlightGate {
  run<T>(operation: () => Promise<T>): Promise<T>;
  invalidate(): void;
  isRunning(): boolean;
}

export function createSingleFlightGate(): SingleFlightGate {
  let inFlight: Promise<unknown> | null = null;

  return {
    run<T>(operation: () => Promise<T>): Promise<T> {
      if (inFlight) return inFlight as Promise<T>;

      const operationPromise = Promise.resolve().then(operation);
      const request = operationPromise.finally(() => {
        if (inFlight === request) inFlight = null;
      });
      inFlight = request;
      return request;
    },
    invalidate() {
      inFlight = null;
    },
    isRunning() {
      return inFlight !== null;
    },
  };
}
