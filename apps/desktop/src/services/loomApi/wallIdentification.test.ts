import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { identifyWallEndpoint } from "./walls.ts";
import { parseWallState } from "./wallValidation.ts";
import { WALL_PROTOCOL_VERSION, type WallEndpoint, type WallLayout, type WallState } from "./wallTypes.ts";

const fixture = JSON.parse(readFileSync(new URL("../../../../../protocol/fixtures/wall-geometry.v1.json", import.meta.url), "utf8")) as { layout: WallLayout; endpoints: WallEndpoint[] };
function snapshot(): WallState {
  return { protocolVersion: WALL_PROTOCOL_VERSION, revision: 7, layouts: [fixture.layout],
    endpoints: fixture.endpoints.map((endpoint) => ({ endpoint: { ...endpoint, display: { name: "Display 1", canIdentify: true } },
      online: true, appliedRevision: 7, identification: { requestId: "request", remainingMs: 10000, applied: false } })) };
}

test("identification is independent of layout revision and fails closed on invalid status", () => {
  const state = snapshot(), row = state.endpoints[0];
  assert.equal(parseWallState(state).endpoints[0].identification?.applied, false);
  for (const value of [
    { ...row, online: false, appliedRevision: null },
    { ...row, endpoint: { ...row.endpoint, display: null } },
    { ...row, endpoint: { ...row.endpoint, display: { name: "Display 1", canIdentify: false } } },
    { ...row, endpoint: { ...row.endpoint, display: { name: "bad\nname", canIdentify: true } } },
    { ...row, identification: { ...row.identification, remainingMs: 10001 } },
    { ...row, identification: { ...row.identification, extra: true } },
  ]) assert.throws(() => parseWallState({ ...state, endpoints: [value, state.endpoints[1]] }), /无效的墙面数据/);
  assert.throws(() => parseWallState({ ...state, presentations: [{ wallId: fixture.layout.wallId, revision: 7, mode: "black" }] }), /屏幕识别/);
});

test("identification uses a volatile endpoint command without catalog CAS or retries", async (context) => {
  const original = globalThis.fetch;
  context.after(() => { globalThis.fetch = original; });
  const calls: { url: string; method: string; body: unknown }[] = [];
  globalThis.fetch = async (url, options) => {
    calls.push({ url: String(url), method: String(options?.method), body: JSON.parse(String(options?.body)) });
    return Response.json(snapshot());
  };
  const result = await identifyWallEndpoint("http://127.0.0.1:18765", "endpoint-left");
  assert.equal(result.revision, 7); assert.equal(result.layouts[0].revision, 7);
  assert.deepEqual(calls, [{ url: "http://127.0.0.1:18765/v1/walls/endpoints/identify", method: "POST", body: { endpointId: "endpoint-left" } }]);
  let failures = 0;
  globalThis.fetch = async () => { failures++; return Response.json({}, { status: 409 }); };
  await assert.rejects(identifyWallEndpoint("http://127.0.0.1:18765", "endpoint-left"), /HTTP 409/);
  assert.equal(failures, 1);
  await assert.rejects(identifyWallEndpoint("http://127.0.0.1:18765", "invalid id"), /目标无效/);
  assert.equal(failures, 1);
});
