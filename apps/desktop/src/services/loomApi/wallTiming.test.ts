import assert from "node:assert/strict";
import { test } from "node:test";
import { parseWallState } from "./wallValidation.ts";

test("wall timing rejects unrecognized fields and foreign scene coverage", () => {
  const state = { protocolVersion: "loom.wall.v1", revision: 0, endpoints: [], layouts: [],
    timing: { clockId: "boot-a", serverTimeMs: 100, scenes: [] } };
  assert.equal(parseWallState(state).timing?.clockId, "boot-a");
  for (const timing of [null, { ...state.timing, extra: true }, { ...state.timing, serverTimeMs: -1 },
    { ...state.timing, scenes: [{ wallId: "missing", revision: 1, preparedAtMs: 1, activateAtMs: 50 }] }]) {
    assert.throws(() => parseWallState({ ...state, timing }));
  }
});
