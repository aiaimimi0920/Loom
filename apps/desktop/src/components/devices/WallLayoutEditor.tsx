import { useState } from "react";
import type { WallLayout, WallSourceOption, WallState, WallRotation } from "../../services/loomApi/wallTypes.ts";
import { addWallPlacement, addWallTile, boundingWall } from "./wallDraft.ts";
import { WallGeometryFields, WallNumber } from "./WallGeometryFields.tsx";
import { WallLayoutPreview } from "./WallLayoutPreview.tsx";
import { WallImageImport } from "./WallImageImport.tsx";
import { layoutImageSources } from "../../services/loomApi/wallSources.ts";

export function WallLayoutEditor({ baseUrl, layout, state, sources: inventory, persisted, onChange }: {
  baseUrl: string; layout: WallLayout; state: WallState; sources: WallSourceOption[]; persisted: boolean;
  onChange: (layout: WallLayout) => void;
}) {
  const [sourceKey, setSourceKey] = useState("");
  const [imported, setImported] = useState<WallSourceOption[]>([]);
  const sources = [...new Map([...layoutImageSources([...state.layouts, layout]), ...inventory, ...imported].map((entry) =>
    [`${entry.source.kind}:${entry.source.id}`, entry])).values()];
  const available = state.endpoints.filter(({ endpoint }) => !state.layouts.some((wall) => wall.wallId !== layout.wallId
    && wall.tiles.some((t) => t.endpointId === endpoint.endpointId)) && !layout.tiles.some((t) => t.endpointId === endpoint.endpointId));
  const source = sources.find((entry) => `${entry.source.kind}:${entry.source.id}` === sourceKey);
  return <div className="wall-editor">
    <label><span>墙面 ID</span><input className="studio-input" value={layout.wallId} disabled={persisted}
      maxLength={160} onChange={(event) => onChange({ ...layout, wallId: event.target.value })} /></label>
    <WallGeometryFields label="墙面范围（逻辑坐标）" rect={layout.bounds} onChange={(bounds) => onChange({ ...layout, bounds })} />
    <WallLayoutPreview layout={layout} />
    <section aria-label="排列瓷砖">
      <div className="wall-toolbar"><h3>瓷砖 · {layout.tiles.length} / 64</h3>
        <button type="button" className="ghost-button" disabled={!layout.tiles.length}
          onClick={() => onChange({ ...layout, bounds: boundingWall(layout.tiles.map((t) => t.rect)) })}>墙面适应瓷砖</button>
      </div>
      <label><span>添加已注册输出（排列在右侧）</span><select className="studio-input" value="" disabled={layout.tiles.length >= 64}
        onChange={(event) => { const selected = available.find((e) => e.endpoint.endpointId === event.target.value);
          if (selected) onChange(addWallTile(layout, selected.endpoint)); }}>
        <option value="">{available.length ? "选择输出" : "没有未分配的输出"}</option>
        {available.map(({ endpoint, online }) => <option key={endpoint.endpointId} value={endpoint.endpointId}>
          {endpoint.deviceId} / {endpoint.outputId} · {online ? "在线" : "离线"}</option>)}
      </select></label>
      {layout.tiles.map((tile, index) => <details className="wall-row" key={tile.tileId} open>
        <summary>#{index + 1} · {tile.endpointId}</summary>
        <WallGeometryFields label="瓷砖占地（旋转后的宽高）" rect={tile.rect} onChange={(rect) => onChange({ ...layout,
          tiles: layout.tiles.map((t) => t.tileId === tile.tileId ? { ...t, rect } : t) })} />
        <div className="wall-toolbar"><label><span>顺时针朝向</span><select className="studio-input" value={tile.rotation}
          onChange={(event) => onChange({ ...layout, tiles: layout.tiles.map((t) => t.tileId === tile.tileId
            ? { ...t, rotation: event.target.value as WallRotation } : t) })}>
          {[0, 90, 180, 270].map((rotation) => <option key={rotation} value={`deg${rotation}`}>{rotation}°</option>)}
        </select></label><button type="button" className="ghost-button" onClick={() => onChange({ ...layout,
          tiles: layout.tiles.filter((t) => t.tileId !== tile.tileId) })}>从草稿移除</button></div>
      </details>)}
    </section>
    <section aria-label="放置内容"><h3>内容 · {layout.placements.length} / 256</h3>
      <p className="wall-muted">复用已有来源与 Art 正式图片输出，也可导入静态图片。添加位置不会启动新来源，交互默认关闭。</p>
      <WallImageImport baseUrl={baseUrl} onImported={(entry) => {
        setImported((current) => [...current.filter((item) => item.source.id !== entry.source.id), entry].slice(-64));
        setSourceKey(`image:${entry.source.id}`);
      }} />
      <div className="wall-toolbar"><label><span>已有内容</span><select className="studio-input" value={sourceKey}
        onChange={(event) => setSourceKey(event.target.value)}><option value="">选择来源</option>
        {sources.map((entry) => <option key={`${entry.source.kind}:${entry.source.id}`} value={`${entry.source.kind}:${entry.source.id}`}>
          {entry.label} · {entry.status}</option>)}
      </select></label><button type="button" className="ghost-button" disabled={!source || layout.placements.length >= 256}
        onClick={() => { if (source) onChange(addWallPlacement(layout, source.source)); }}>放置到墙面</button></div>
      {layout.placements.map((p) => {
        const update = (patch: Partial<typeof p>) => onChange({ ...layout,
          placements: layout.placements.map((entry) => entry.placementId === p.placementId ? { ...entry, ...patch } : entry) });
        const listed = sources.find((s) => s.source.kind === p.source.kind && s.source.id === p.source.id);
        return <details className="wall-row" key={p.placementId} open>
          <summary>{p.source.kind} · {p.source.id}</summary>
          {!listed && <p className="wall-warning">当前目录未列出来源，原有引用仍保留。请先刷新内容来源；仍在运行的 Hook 采集可重新连接，已关闭的来源需要重新发布并选择。</p>}
          {listed && p.source.kind === "live" && <p className="wall-muted" role="status">{listed.status}</p>}
          <WallGeometryFields label="墙面位置" rect={p.rect} onChange={(rect) => update({ rect })} />
          <WallGeometryFields label="源裁剪（0 到 1）" rect={p.sourceCrop} onChange={(sourceCrop) => update({ sourceCrop })} />
          <div className="wall-toolbar"><WallNumber label="层级（较大在上）" value={p.zIndex} integer onChange={(zIndex) => update({ zIndex })} />
            <label className="wall-checkbox"><input type="checkbox" disabled={p.source.kind === "image"} checked={p.interactive}
              onChange={(event) => update({ interactive: event.target.checked })} />允许交互（仍需来源授权）</label>
            <button type="button" className="ghost-button" onClick={() => onChange({ ...layout,
              placements: layout.placements.filter((entry) => entry.placementId !== p.placementId) })}>从草稿移除</button>
          </div>
        </details>;
      })}
    </section>
  </div>;
}
