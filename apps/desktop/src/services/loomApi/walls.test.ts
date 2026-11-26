import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { Catalogs, setLanguage } from "../../i18n/index.ts";
import { enDevices } from "../../locales/en/devices.ts";
import { zhHansDevices } from "../../locales/zh-Hans/devices.ts";
import { saveWallLayout } from "./walls.ts";
import { parseWallLayout, parseWallState } from "./wallValidation.ts";
import { layoutImageSources, liveWallSources, surfaceWallSources } from "./wallSources.ts";
import { addWallPlacement, addWallTile, createWallDraft } from "../../components/devices/wallDraft.ts";
import { WALL_PROTOCOL_VERSION, type WallLayout, type WallState } from "./wallTypes.ts";

const fixture = JSON.parse(readFileSync(new URL("../../../../../protocol/fixtures/wall-geometry.v1.json", import.meta.url), "utf8"));
const layout: WallLayout = parseWallLayout(fixture.layout);

test("devices and wall locale shards have complete English and Simplified Chinese parity", () => {
  new Catalogs([enDevices], [zhHansDevices]).validateCompleteness();
});

test("saved wall image references remain selectable after the Art owner and local upload state disappear", () => {
  const image = { ...layout.placements[0], source: { kind: "image" as const, id: `sha256:${"a".repeat(64)}` }, interactive: false };
  const saved = { ...layout, placements: [image, image, layout.placements[0]] };
  const sources = layoutImageSources([saved]);
  assert.equal(sources.length, 1);
  assert.deepEqual(sources[0].source, image.source);
});

test("desktop accepts the public geometry fixture and rejects overlap / invalid crop / non-finite edits", (context) => {
  context.after(() => setLanguage("en"));
  assert.equal(layout.tiles.length, 2);
  assert.throws(() => parseWallLayout({ ...layout, futureField: true }), /unknown or missing field/);
  assert.throws(() => parseWallLayout({ ...layout, bounds: { ...layout.bounds, futureField: true } }), /unknown or missing field/);
  assert.throws(() => parseWallLayout({ ...layout, tiles: [{ ...layout.tiles[0], rotation: ["deg0"] }] }), /tile orientation/);
  const candidate = structuredClone(layout);
  candidate.tiles[1].rect = { ...candidate.tiles[0].rect };
  assert.throws(() => parseWallLayout(candidate), /overlapping tiles/);
  setLanguage("zh-Hans");
  assert.throws(() => parseWallLayout(candidate), /\u74f7\u7816\u91cd\u53e0/);
  setLanguage("en");
  candidate.tiles = [];
  candidate.placements[0].sourceCrop.width = 2;
  assert.throws(() => parseWallLayout(candidate), /source crop bounds/);
  candidate.placements = [];
  candidate.bounds.x = NaN;
  assert.throws(() => parseWallLayout(candidate), /rectangle bounds/);
});

test("adding a second tile preserves source identity and appends outside the existing footprint", () => {
  const endpoint = fixture.endpoints[0];
  const first = { ...layout, tiles: [layout.tiles[0]] };
  const next = addWallTile(first, { ...endpoint, endpointId: "new-endpoint" });
  assert.equal(next.tiles[1].rect.x, first.tiles[0].rect.x + first.tiles[0].rect.width);
  assert.equal(next.placements, first.placements);
  const placement = addWallPlacement(next, first.placements[0].source);
  assert.deepEqual(placement.placements.at(-1)?.source, first.placements[0].source);
  assert.notEqual(placement.placements.at(-1)?.placementId, first.placements[0].placementId);
  const draft = createWallDraft(25, next);
  draft.layout.bounds.x = 5;
  assert.notEqual(next.bounds.x, 5);
  assert.equal(draft.baseRevision, 25);
});

test("save uses global CAS, assigns next revision, and never retries a conflict", async (context) => {
  const calls: { method: string; body: { baseRevision: number; layout: WallLayout } }[] = [];
  const original = globalThis.fetch;
  context.after(() => { globalThis.fetch = original; });
  globalThis.fetch = async (_url, options) => {
    calls.push({ method: String(options?.method), body: JSON.parse(String(options?.body)) });
    return Response.json({ error: { message: "wall revision conflict" } }, { status: 409 });
  };
  await assert.rejects(saveWallLayout("http://127.0.0.1:18765", 20, layout), /HTTP 409/);
  assert.equal(calls.length, 1);
  assert.equal(calls[0].method, "PUT");
  assert.equal(calls[0].body.baseRevision, 20);
  assert.equal(calls[0].body.layout.revision, 21);
  assert.deepEqual(calls[0].body.layout.placements, layout.placements);
});

test("offline output cannot acknowledge presentation and missing outputs invalidate admin state", () => {
  const state: WallState = { protocolVersion: WALL_PROTOCOL_VERSION, revision: layout.revision,
    layouts: [layout], endpoints: fixture.endpoints.map((endpoint: unknown) => ({ endpoint, online: false, appliedRevision: null })) };
  assert.equal(parseWallState(state).layouts.length, 1);
  assert.throws(() => parseWallState({ ...state, endpoints: [{ ...state.endpoints[0],
    endpoint: { ...state.endpoints[0].endpoint, futureCapability: true } }] }), /unknown or missing field/);
  state.endpoints[0].appliedRevision = layout.revision;
  assert.throws(() => parseWallState(state), /applied revision acknowledgment/);
  state.endpoints = [];
  assert.throws(() => parseWallState(state), /unknown output/);
});

test("source inventory excludes closed Live and preview-only images; only formal resource outputs are images", () => {
  assert.deepEqual(liveWallSources({ sessions: [{ closed: true, session: { sessionId: "closed" } },
    { closed: false, sourceConnected: false, session: { sessionId: "kept" } }] }).map((s) => s.source.id), ["kept"]);
  const resource = { resourceId: `sha256:${"a".repeat(64)}`, kind: "image" };
  const instance = { descriptor: { instanceId: "art-1", artId: "image-art" },
    latestPreview: { outputs: { image: { kind: "resource", resource } } } };
  assert.equal(surfaceWallSources({ instances: [instance] }).length, 1);
  const result = surfaceWallSources({ instances: [{ ...instance, latestResult: { outputs: { image: { kind: "resource", resource } } } }] });
  assert.deepEqual(result.map((s) => s.source.kind), ["surface", "image"]);
  assert.equal(result[1].source.id, resource.resourceId);
});
