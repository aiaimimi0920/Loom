import { WALL_PROTOCOL_VERSION, type WallEndpoint, type WallLayout, type WallRect, type WallSource, type WallState } from "../../services/loomApi/wallTypes.ts";

export interface WallDraft { layout: WallLayout; baseRevision: number; persisted: boolean; dirty: boolean }
export function createWallDraft(revision: number, layout?: WallLayout): WallDraft {
  return { baseRevision: revision, persisted: Boolean(layout), dirty: !layout,
    layout: layout ? structuredClone(layout) : { protocolVersion: WALL_PROTOCOL_VERSION,
      wallId: `wall-${crypto.randomUUID()}`, revision: revision + 1,
      bounds: { x: 0, y: 0, width: 1920, height: 1080 }, tiles: [], placements: [] } };
}
export function draftAfterPresentation(draft: WallDraft | null, baseRevision: number, state: WallState): WallDraft | null {
  // Control mutations must not silently rebase unsaved or already-conflicted geometry.
  if (!draft?.persisted || draft.dirty || draft.baseRevision !== baseRevision) return draft;
  const layout = state.layouts.find((layout) => layout.wallId === draft.layout.wallId);
  return layout ? createWallDraft(state.revision, layout) : draft;
}
export function boundingWall(rects: WallRect[]): WallRect {
  const x = Math.min(...rects.map((r) => r.x)), y = Math.min(...rects.map((r) => r.y));
  return { x, y, width: Math.max(...rects.map((r) => r.x + r.width)) - x,
    height: Math.max(...rects.map((r) => r.y + r.height)) - y };
}
export function addWallTile(layout: WallLayout, endpoint: WallEndpoint): WallLayout {
  if (layout.tiles.length >= 64 || layout.tiles.some((t) => t.endpointId === endpoint.endpointId)) return layout;
  const x = layout.tiles.length ? Math.max(...layout.tiles.map((t) => t.rect.x + t.rect.width)) : layout.bounds.x;
  const rect = { x, y: layout.bounds.y, ...endpoint.pixelSize };
  const tiles = [...layout.tiles, { tileId: `tile-${crypto.randomUUID()}`, endpointId: endpoint.endpointId, rect, rotation: "deg0" as const }];
  return { ...layout, tiles, bounds: boundingWall([layout.bounds, ...tiles.map((t) => t.rect)]) };
}
export function addWallPlacement(layout: WallLayout, source: WallSource): WallLayout {
  if (layout.placements.length >= 256) return layout;
  return { ...layout, placements: [...layout.placements, { placementId: `placement-${crypto.randomUUID()}`,
    source: { ...source }, rect: { ...layout.bounds }, sourceCrop: { x: 0, y: 0, width: 1, height: 1 },
    zIndex: Math.min(2147483647, Math.max(-1, ...layout.placements.map((p) => p.zIndex)) + 1), interactive: false }] };
}
export function sortWallPlacements(layout: WallLayout) {
  return [...layout.placements].sort((a, b) => a.zIndex - b.zIndex
    || (a.placementId < b.placementId ? -1 : a.placementId > b.placementId ? 1 : 0));
}
