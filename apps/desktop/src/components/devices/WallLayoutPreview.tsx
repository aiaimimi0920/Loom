import type { WallLayout } from "../../services/loomApi/wallTypes.ts";
import { sortWallPlacements } from "./wallDraft.ts";

export function WallLayoutPreview({ layout }: { layout: WallLayout }) {
  const b = layout.bounds;
  const valid = [b.x, b.y, b.width, b.height].every(Number.isFinite) && b.width > 0 && b.height > 0;
  if (!valid) return <p role="status">填写有效的墙面尺寸后显示预览。</p>;
  const font = Math.max(b.width, b.height) / 50;
  return <svg className="wall-preview" viewBox={`${b.x} ${b.y} ${b.width} ${b.height}`}
    role="img" aria-label={`墙面布局预览，${layout.tiles.length} 块瓷砖，${layout.placements.length} 个内容位置`}>
    <rect {...b} className="wall-preview__background" />
    {sortWallPlacements(layout).filter((p) => Object.values(p.rect).every(Number.isFinite)).map((p) =>
      <rect key={p.placementId} {...p.rect} className="wall-preview__content"><title>{p.source.kind} · {p.source.id}</title></rect>)}
    {layout.tiles.filter((t) => Object.values(t.rect).every(Number.isFinite)).map((t, index) => <g key={t.tileId}>
      <rect {...t.rect} className="wall-preview__tile" />
      <text x={t.rect.x + font / 2} y={t.rect.y + font * 1.5} fontSize={font}>#{index + 1} · {t.rotation.slice(3)}°</text>
      <title>{t.endpointId}</title>
    </g>)}
  </svg>;
}
