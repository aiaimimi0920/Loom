// Validate untrusted daemon snapshots before they reach layout controls or SVG.
import { uiFormat, uiText } from "../../i18n/index.ts";
import { WALL_PROTOCOL_VERSION, type WallDisplayInfo, type WallIdentification, type WallLayout, type WallPresentation, type WallPresentationReport, type WallRect, type WallState, type WallTiming, type WallSceneReport } from "./wallTypes.ts";

export function wallRecord(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error(uiText("devices.wall.validation.invalidData"));
  return value as Record<string, unknown>;
}
function check(condition: boolean, fieldKey: string): asserts condition {
  if (!condition) {
    throw new Error(uiFormat("devices.wall.validation.invalidField", { field: uiText(`devices.wall.validation.${fieldKey}`) }));
  }
}
function wireRecord(value: unknown, keys: string[], optional: string[] = []): Record<string, unknown> {
  const record = wallRecord(value);
  check(Object.keys(record).every((key) => keys.includes(key) || optional.includes(key))
    && keys.every((key) => Object.hasOwn(record, key)), "unknownOrMissingField");
  return record;
}
export function validWallId(value: unknown): value is string {
  return typeof value === "string" && /^[A-Za-z0-9_.:/-]{1,160}$/.test(value);
}
function number(value: unknown, min: number, max: number, integer = false): value is number {
  return typeof value === "number" && Number.isFinite(value) && value >= min && value <= max
    && (!integer || Number.isSafeInteger(value));
}
function list(value: unknown, max: number): unknown[] {
  check(Array.isArray(value) && value.length <= max, "countExceeded");
  return value;
}
function unique(values: unknown[]): boolean { return new Set(values).size === values.length; }
function rect(value: unknown): WallRect {
  const r = wireRecord(value, ["x", "y", "width", "height"]);
  check(number(r.x, -1e6, 1e6) && number(r.y, -1e6, 1e6)
    && number(r.width, 1 / 65536, 1e6) && number(r.height, 1 / 65536, 1e6), "rectBounds");
  check(r.x + r.width <= 1e6 && r.y + r.height <= 1e6, "rectEnd");
  return { x: r.x, y: r.y, width: r.width, height: r.height };
}
export function wallContains(a: WallRect, b: WallRect): boolean {
  return b.x >= a.x && b.y >= a.y && b.x + b.width <= a.x + a.width && b.y + b.height <= a.y + a.height;
}
export function wallsOverlap(a: WallRect, b: WallRect): boolean {
  return a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;
}
export function parseWallLayout(value: unknown): WallLayout {
  const l = wireRecord(value, ["protocolVersion", "wallId", "revision", "bounds", "tiles", "placements"]);
  check(l.protocolVersion === WALL_PROTOCOL_VERSION && validWallId(l.wallId), "wallIdOrProtocol");
  check(number(l.revision, 1, Number.MAX_SAFE_INTEGER, true), "layoutRevision");
  const bounds = rect(l.bounds);
  const tiles = list(l.tiles, 64).map((value) => {
    const t = wireRecord(value, ["tileId", "endpointId", "rect", "rotation"]);
    check(validWallId(t.tileId) && validWallId(t.endpointId), "tileId");
    check(typeof t.rotation === "string" && ["deg0", "deg90", "deg180", "deg270"].includes(t.rotation), "tileRotation");
    return { tileId: t.tileId, endpointId: t.endpointId, rotation: t.rotation as WallLayout["tiles"][number]["rotation"], rect: rect(t.rect) };
  });
  check(unique(tiles.map((t) => t.tileId)) && unique(tiles.map((t) => t.endpointId)), "duplicateTile");
  tiles.forEach((t, i) => {
    check(wallContains(bounds, t.rect), "tileOutsideWall");
    check(!tiles.slice(0, i).some((other) => wallsOverlap(t.rect, other.rect)), "tileOverlap");
  });
  const placements = list(l.placements, 256).map((value): WallLayout["placements"][number] => {
    const p = wireRecord(value, ["placementId", "source", "rect", "sourceCrop", "zIndex", "interactive"]);
    const s = wireRecord(p.source, ["kind", "id"]);
    check(validWallId(p.placementId) && validWallId(s.id), "placementId");
    check(s.kind === "live" || s.kind === "image" || s.kind === "surface", "sourceKind");
    check(s.kind !== "image" || /^sha256:[0-9a-f]{64}$/.test(s.id), "imageResourceId");
    check(typeof p.interactive === "boolean" && (s.kind !== "image" || !p.interactive), "interactivePolicy");
    check(number(p.zIndex, -2147483648, 2147483647, true), "zIndex");
    const sourceCrop = rect(p.sourceCrop);
    check(wallContains({ x: 0, y: 0, width: 1, height: 1 }, sourceCrop), "sourceCrop");
    return { placementId: p.placementId, source: { kind: s.kind, id: s.id }, rect: rect(p.rect), sourceCrop, zIndex: p.zIndex, interactive: p.interactive };
  });
  check(unique(placements.map((p) => p.placementId)), "duplicatePlacement");
  return { protocolVersion: WALL_PROTOCOL_VERSION, wallId: l.wallId, revision: l.revision, bounds, tiles, placements };
}
export function parseWallState(value: unknown): WallState {
  const s = wireRecord(value, ["protocolVersion", "revision", "endpoints", "layouts"], ["presentations", "timing"]);
  check(s.protocolVersion === WALL_PROTOCOL_VERSION && number(s.revision, 0, Number.MAX_SAFE_INTEGER, true), "catalogRevision");
  const revision = s.revision;
  const layouts = list(s.layouts, 64).map(parseWallLayout);
  check(unique(layouts.map((l) => l.wallId)) && layouts.every((l) => l.revision <= revision), "wallRevision");
  check(unique(layouts.flatMap((l) => l.tiles.map((t) => t.endpointId))), "duplicateEndpointAcrossWalls");
  const timing = Object.hasOwn(s, "timing") ? parseTiming(s.timing, layouts) : undefined;
  const presentations: WallPresentation[] = (Object.hasOwn(s, "presentations") ? list(s.presentations, 64) : []).map((value) => {
    const control = wireRecord(value, ["wallId", "revision", "mode"]);
    check(validWallId(control.wallId) && layouts.some((layout) => layout.wallId === control.wallId), "presentationWall");
    check(number(control.revision, 1, revision, true) && (control.mode === "frozen" || control.mode === "black"), "presentationRevisionOrMode");
    return { wallId: control.wallId, revision: control.revision, mode: control.mode };
  });
  check(unique(presentations.map((p) => p.wallId)), "duplicatePresentation");
  const endpoints = list(s.endpoints, 256).map((value) => {
    const status = wireRecord(value, ["endpoint", "online", "appliedRevision"], ["presentation", "identification", "scene"]);
    const e = wireRecord(status.endpoint, ["protocolVersion", "endpointId", "deviceId", "outputId", "pixelSize", "renderModes", "inputCapabilities"], ["display", "scheduledPresentation"]);
    check(!Object.hasOwn(e, "scheduledPresentation") || typeof e.scheduledPresentation === "boolean", "scheduledCapability");
    const size = wireRecord(e.pixelSize, ["width", "height"]);
    check(e.protocolVersion === WALL_PROTOCOL_VERSION && validWallId(e.endpointId)
      && validWallId(e.deviceId) && validWallId(e.outputId), "endpointIdentity");
    check(number(size.width, 1, 16384, true) && number(size.height, 1, 16384, true), "pixelSize");
    const renderModes = list(e.renderModes, 4), inputCapabilities = list(e.inputCapabilities, 6);
    check(renderModes.length > 0 && unique(renderModes) && renderModes.every((m) => typeof m === "string" && ["image", "raw_bgra", "h264", "surface_v1"].includes(m)), "renderModes");
    check(unique(inputCapabilities) && inputCapabilities.every((m) => typeof m === "string" && ["pointer", "wheel", "keyboard", "text", "touch", "pen"].includes(m)), "inputCapabilities");
    let display: WallDisplayInfo | undefined;
    if (Object.hasOwn(e, "display")) {
      const d = wireRecord(e.display, ["name", "canIdentify"]);
      check(typeof d.name === "string" && d.name.trim().length > 0 && Array.from(d.name).length <= 256
        && !Array.from(d.name).some((c) => c.charCodeAt(0) <= 31 || c === "\x7f") && typeof d.canIdentify === "boolean", "displayInfo");
      display = { name: d.name, canIdentify: d.canIdentify };
    }
    const layout = layouts.find((l) => l.tiles.some((t) => t.endpointId === e.endpointId));
    check(typeof status.online === "boolean" && (status.appliedRevision === null
      || (status.online && number(status.appliedRevision, 1, revision, true) && status.appliedRevision === layout?.revision)), "appliedAck");
    const scene = Object.hasOwn(status, "scene") ? parseSceneReport(status.scene, layout?.revision, status.appliedRevision as number | null) : undefined;
    check(!scene || (status.online && timing !== undefined), "sceneReceiptAuth");
    let presentation: WallPresentationReport | undefined;
    if (Object.hasOwn(status, "presentation")) {
      const report = wireRecord(status.presentation, ["revision", "outcome"]);
      const control = presentations.find((p) => p.wallId === layout?.wallId);
      check(status.online && control !== undefined && report.revision === control.revision, "presentationAckRevision");
      check(report.outcome === "applied" || report.outcome === "frame_unavailable", "presentationOutcome");
      check(control.mode === "frozen" ? report.outcome === "applied" ? status.appliedRevision === layout?.revision : status.appliedRevision === null
        : report.outcome === "applied" && status.appliedRevision === null, "presentationAck");
      presentation = { revision: control.revision, outcome: report.outcome };
    }
    let identification: WallIdentification | undefined;
    if (Object.hasOwn(status, "identification")) {
      const value = wireRecord(status.identification, ["requestId", "remainingMs", "applied"]);
      check(status.online && display?.canIdentify === true && validWallId(value.requestId)
        && number(value.remainingMs, 1, 10_000, true) && typeof value.applied === "boolean"
        && !presentations.some((p) => p.wallId === layout?.wallId), "identificationReceipt");
      identification = { requestId: value.requestId, remainingMs: value.remainingMs, applied: value.applied };
    }
    return { endpoint: { protocolVersion: WALL_PROTOCOL_VERSION, endpointId: e.endpointId, deviceId: e.deviceId,
      outputId: e.outputId, pixelSize: { width: size.width, height: size.height }, renderModes, inputCapabilities, ...(display ? { display } : {}),
      ...(Object.hasOwn(e, "scheduledPresentation") ? { scheduledPresentation: e.scheduledPresentation } : {}) },
    online: status.online, appliedRevision: status.appliedRevision, ...(presentation ? { presentation } : {}), ...(identification ? { identification } : {}),
    ...(scene ? { scene } : {}) } as WallState["endpoints"][number];
  });
  check(unique(endpoints.map((e) => e.endpoint.endpointId))
    && unique(endpoints.map((e) => `${e.endpoint.deviceId}\n${e.endpoint.outputId}`)), "duplicateEndpointIdentity");
  check(layouts.every((l) => l.tiles.every((t) => endpoints.some((e) => e.endpoint.endpointId === t.endpointId))), "unknownEndpoint");
  return { protocolVersion: WALL_PROTOCOL_VERSION, revision, endpoints, layouts, ...(Object.hasOwn(s, "presentations") ? { presentations } : {}), ...(timing ? { timing } : {}) };
}

function parseTiming(value: unknown, layouts: WallLayout[]): WallTiming {
  const timing = wireRecord(value, ["clockId", "serverTimeMs", "scenes"]);
  check(validWallId(timing.clockId) && number(timing.serverTimeMs, 1, Number.MAX_SAFE_INTEGER, true), "clockIdentity");
  const serverTimeMs = timing.serverTimeMs;
  const scenes = list(timing.scenes, 64).map((value) => {
    const scene = wireRecord(value, ["wallId", "revision", "preparedAtMs", "activateAtMs"]);
    const layout = layouts.find((layout) => layout.wallId === scene.wallId);
    check(layout !== undefined && scene.revision === layout.revision, "sceneRevision");
    check(number(scene.preparedAtMs, 1, serverTimeMs, true)
      && number(scene.activateAtMs, scene.preparedAtMs, scene.preparedAtMs + 10_000, true), "sceneActivateAt");
    return { wallId: layout.wallId, revision: layout.revision, preparedAtMs: scene.preparedAtMs, activateAtMs: scene.activateAtMs };
  });
  check(unique(scenes.map((scene) => scene.wallId)) && scenes.length === layouts.length, "sceneCoverage");
  return { clockId: timing.clockId, serverTimeMs, scenes };
}

function parseSceneReport(value: unknown, revision: number | undefined, applied: number | null): WallSceneReport {
  const report = wireRecord(value, ["revision", "prepared", "appliedAtMs", "clockUncertaintyMs"]);
  check(revision !== undefined && report.revision === revision && typeof report.prepared === "boolean", "sceneReceiptRevision");
  check(report.appliedAtMs === null || number(report.appliedAtMs, 1, Number.MAX_SAFE_INTEGER, true), "sceneAppliedAt");
  check(report.clockUncertaintyMs === null || number(report.clockUncertaintyMs, 0, 1000, true), "clockUncertainty");
  check((report.appliedAtMs !== null) === (applied !== null)
    && (applied === null || (report.prepared && report.clockUncertaintyMs !== null)), "sceneReceiptOutcome");
  return { revision, prepared: report.prepared, appliedAtMs: report.appliedAtMs, clockUncertaintyMs: report.clockUncertaintyMs };
}
