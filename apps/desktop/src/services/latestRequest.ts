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
