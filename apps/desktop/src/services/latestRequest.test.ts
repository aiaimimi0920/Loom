import assert from "node:assert/strict";
import test from "node:test";

import { createLatestRequestGate } from "./latestRequest.ts";

test("only the latest request token remains current", () => {
  const gate = createLatestRequestGate();
  const first = gate.begin();
  assert.equal(gate.isCurrent(first), true);

  const second = gate.begin();
  assert.equal(gate.isCurrent(first), false);
  assert.equal(gate.isCurrent(second), true);
});

test("invalidating the gate rejects an in-flight request", () => {
  const gate = createLatestRequestGate();
  const request = gate.begin();

  gate.invalidate();

  assert.equal(gate.isCurrent(request), false);
});
