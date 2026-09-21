import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { setWallPresentation } from "./walls.ts";
import { parseWallState } from "./wallValidation.ts";
import { createWallDraft, draftAfterPresentation } from "../../components/devices/wallDraft.ts";
import { WALL_PROTOCOL_VERSION, type WallEndpoint, type WallLayout, type WallState } from "./wallTypes.ts";

const fixture = JSON.parse(readFileSync(new URL("../../../../../protocol/fixtures/wall-geometry.v1.json", import.meta.url), "utf8")) as { layout: WallLayout; endpoints: WallEndpoint[] };
function snapshot(): WallState {
  return { protocolVersion: WALL_PROTOCOL_VERSION, revision: 8, layouts: [fixture.layout],
    presentations: [{ wallId: fixture.layout.wallId, revision: 8, mode: "frozen" }],
    endpoints: fixture.endpoints.map((endpoint) => ({ endpoint, online: true, appliedRevision: 7,
      presentation: { revision: 8, outcome: "applied" } })) };
}

test("display control reports are versioned separately and reject unknown, stale, or incoherent outcomes", () => {
  const state = snapshot(), endpoint = state.endpoints[0], control = state.presentations![0];
  assert.equal(parseWallState(state).endpoints[0].presentation?.revision, 8);
  for (const value of [
    { ...state, presentations: null },
    { ...state, presentations: [control, control] },
    { ...state, presentations: [{ ...control, mode: "running" }] },
    { ...state, presentations: [{ ...control, wallId: "missing" }] },
    { ...state, presentations: [{ ...control, extra: true }] },
    { ...state, endpoints: [{ ...endpoint, presentation: { revision: 7, outcome: "applied" } }, state.endpoints[1]] },
    { ...state, endpoints: [{ ...endpoint, presentation: { revision: 8, outcome: "frame_unavailable" } }, state.endpoints[1]] },
    { ...state, endpoints: [{ ...endpoint, presentation: { revision: 8, outcome: "unknown" } }, state.endpoints[1]] },
    { ...state, endpoints: [{ ...endpoint, presentation: { ...endpoint.presentation, extra: true } }, state.endpoints[1]] },
    { ...state, endpoints: [{ ...endpoint, online: false, appliedRevision: null }, state.endpoints[1]] },
  ]) assert.throws(() => parseWallState(value), /无效的墙面数据/);
  assert.equal(parseWallState({ ...state, endpoints: [{ ...endpoint, appliedRevision: null,
    presentation: { revision: 8, outcome: "frame_unavailable" } }, state.endpoints[1]] }).endpoints[0].presentation?.outcome, "frame_unavailable");
});

test("display actions preserve unsaved and conflicted geometry while advancing an unchanged saved draft", () => {
  const state = snapshot(), draft = createWallDraft(7, state.layouts[0]);
  const clean = draftAfterPresentation(draft, 7, state);
  assert.equal(clean?.baseRevision, 8); assert.notEqual(clean, draft);
  draft.dirty = true; draft.layout.bounds.width = 12345;
  assert.equal(draftAfterPresentation(draft, 7, state), draft);
  assert.equal(draft.baseRevision, 7); assert.equal(draft.layout.bounds.width, 12345);
  draft.dirty = false;
  assert.equal(draftAfterPresentation(draft, 6, state), draft);
  const unsaved = createWallDraft(7);
  assert.equal(draftAfterPresentation(unsaved, 7, state), unsaved);
});

test("display commands use catalog CAS and do not retry a conflict", async (context) => {
  const original = globalThis.fetch;
  context.after(() => { globalThis.fetch = original; });
  const calls: { url: string; method: string; body: unknown }[] = [];
  globalThis.fetch = async (url, options) => {
    calls.push({ url: String(url), method: String(options?.method), body: JSON.parse(String(options?.body)) });
    return Response.json({ error: { message: "wall revision conflict" } }, { status: 409 });
  };
  await assert.rejects(setWallPresentation("http://127.0.0.1:18765", 8, "living-room", "frozen"), /HTTP 409/);
  assert.deepEqual(calls, [{ url: "http://127.0.0.1:18765/v1/walls/presentation", method: "PUT",
    body: { baseRevision: 8, wallId: "living-room", mode: "frozen" } }]);
});
