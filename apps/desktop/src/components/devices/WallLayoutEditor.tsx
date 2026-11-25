import { useState } from "react";
import { Badge, Button, Checkbox, Disclosure, Field, Input, Notice, Select } from "@neuro/ui";
import type { WallLayout, WallSourceOption, WallState, WallRotation } from "../../services/loomApi/wallTypes.ts";
import { addWallPlacement, addWallTile, boundingWall } from "./wallDraft.ts";
import { WallGeometryFields, WallNumber } from "./WallGeometryFields.tsx";
import { WallLayoutPreview } from "./WallLayoutPreview.tsx";
import { WallImageImport } from "./WallImageImport.tsx";
import { layoutImageSources } from "../../services/loomApi/wallSources.ts";

export function WallLayoutEditor({
  baseUrl,
  layout,
  state,
  sources: inventory,
  persisted,
  onChange,
}: {
  baseUrl: string;
  layout: WallLayout;
  state: WallState;
  sources: WallSourceOption[];
  persisted: boolean;
  onChange: (layout: WallLayout) => void;
}) {
  const [sourceKey, setSourceKey] = useState("");
  const [imported, setImported] = useState<WallSourceOption[]>([]);
  const sources = [
    ...new Map(
      [...layoutImageSources([...state.layouts, layout]), ...inventory, ...imported].map((entry) => [
        `${entry.source.kind}:${entry.source.id}`,
        entry,
      ]),
    ).values(),
  ];
  const available = state.endpoints.filter(
    ({ endpoint }) =>
      !state.layouts.some(
        (wall) => wall.wallId !== layout.wallId && wall.tiles.some((t) => t.endpointId === endpoint.endpointId),
      ) && !layout.tiles.some((t) => t.endpointId === endpoint.endpointId),
  );
  const source = sources.find((entry) => `${entry.source.kind}:${entry.source.id}` === sourceKey);
  return (
    <div className="wall-editor">
      <Field label="墙面 ID">
        <Input
          value={layout.wallId}
          disabled={persisted}
          maxLength={160}
          onChange={(event) => onChange({ ...layout, wallId: event.target.value })}
        />
      </Field>
      <WallGeometryFields
        label="墙面范围（逻辑坐标）"
        rect={layout.bounds}
        onChange={(bounds) => onChange({ ...layout, bounds })}
      />
      <WallLayoutPreview layout={layout} />
      <section aria-label="排列瓷砖">
        <div className="wall-toolbar">
          <div className="wall-section-heading">
            <h3>瓷砖</h3>
            <Badge>{layout.tiles.length} / 64</Badge>
          </div>
          <Button
            variant="quiet"
            disabled={!layout.tiles.length}
            onClick={() => onChange({ ...layout, bounds: boundingWall(layout.tiles.map((t) => t.rect)) })}
          >
            墙面适应瓷砖
          </Button>
        </div>
        <Field label="添加输出">
          <Select
            value=""
            disabled={layout.tiles.length >= 64}
            onChange={(event) => {
              const selected = available.find((e) => e.endpoint.endpointId === event.target.value);
              if (selected) onChange(addWallTile(layout, selected.endpoint));
            }}
          >
            <option value="">{available.length ? "选择输出" : "没有未分配的输出"}</option>
            {available.map(({ endpoint, online }) => (
              <option key={endpoint.endpointId} value={endpoint.endpointId}>
                {endpoint.deviceId} / {endpoint.outputId} · {online ? "在线" : "离线"}
              </option>
            ))}
          </Select>
        </Field>
        {layout.tiles.map((tile, index) => (
          <Disclosure
            className="wall-row"
            key={tile.tileId}
            defaultOpen
            title={
              <span className="wall-row-title" title={tile.endpointId}>
                #{index + 1} · {tile.endpointId}
              </span>
            }
          >
            <WallGeometryFields
              label="瓷砖占地（旋转后的宽高）"
              rect={tile.rect}
              onChange={(rect) =>
                onChange({ ...layout, tiles: layout.tiles.map((t) => (t.tileId === tile.tileId ? { ...t, rect } : t)) })
              }
            />
            <div className="wall-toolbar">
              <Field label="顺时针朝向">
                <Select
                  value={tile.rotation}
                  onChange={(event) =>
                    onChange({
                      ...layout,
                      tiles: layout.tiles.map((t) =>
                        t.tileId === tile.tileId ? { ...t, rotation: event.target.value as WallRotation } : t,
                      ),
                    })
                  }
                >
                  {[0, 90, 180, 270].map((rotation) => (
                    <option key={rotation} value={`deg${rotation}`}>
                      {rotation}°
                    </option>
                  ))}
                </Select>
              </Field>
              <Button
                variant="quiet"
                aria-label={`移除瓷砖 ${tile.tileId}`}
                onClick={() => onChange({ ...layout, tiles: layout.tiles.filter((t) => t.tileId !== tile.tileId) })}
              >
                从草稿移除
              </Button>
            </div>
          </Disclosure>
        ))}
      </section>
      <section aria-label="放置内容">
        <div className="wall-section-heading">
          <h3>内容</h3>
          <Badge>{layout.placements.length} / 256</Badge>
        </div>
        <WallImageImport
          baseUrl={baseUrl}
          onImported={(entry) => {
            setImported((current) =>
              [...current.filter((item) => item.source.id !== entry.source.id), entry].slice(-64),
            );
            setSourceKey(`image:${entry.source.id}`);
          }}
        />
        <div className="wall-toolbar">
          <Field label="已有内容">
            <Select value={sourceKey} onChange={(event) => setSourceKey(event.target.value)}>
              <option value="">选择来源</option>
              {sources.map((entry) => (
                <option
                  key={`${entry.source.kind}:${entry.source.id}`}
                  value={`${entry.source.kind}:${entry.source.id}`}
                >
                  {entry.label} · {entry.status}
                </option>
              ))}
            </Select>
          </Field>
          <Button
            disabled={!source || layout.placements.length >= 256}
            onClick={() => {
              if (source) onChange(addWallPlacement(layout, source.source));
            }}
          >
            放置到墙面
          </Button>
        </div>
        {layout.placements.map((p) => {
          const update = (patch: Partial<typeof p>) =>
            onChange({
              ...layout,
              placements: layout.placements.map((entry) =>
                entry.placementId === p.placementId ? { ...entry, ...patch } : entry,
              ),
            });
          const listed = sources.find((s) => s.source.kind === p.source.kind && s.source.id === p.source.id);
          return (
            <Disclosure
              className="wall-row"
              key={p.placementId}
              defaultOpen
              title={
                <span className="wall-row-title" title={p.source.id}>
                  {p.source.kind} · {p.source.id}
                </span>
              }
            >
              {!listed && (
                <Notice tone="warning">
                  当前目录未列出来源，原有引用仍保留。请先刷新内容来源；仍在运行的 Otter
                  采集可重新连接，已关闭的来源需要重新发布并选择。
                </Notice>
              )}
              {listed && p.source.kind === "live" && (
                <div role="status">
                  <Badge tone="info">{listed.status}</Badge>
                </div>
              )}
              <WallGeometryFields label="墙面位置" rect={p.rect} onChange={(rect) => update({ rect })} />
              <WallGeometryFields
                label="源裁剪（0 到 1）"
                rect={p.sourceCrop}
                onChange={(sourceCrop) => update({ sourceCrop })}
              />
              <div className="wall-toolbar">
                <WallNumber
                  label="层级（较大在上）"
                  value={p.zIndex}
                  integer
                  onChange={(zIndex) => update({ zIndex })}
                />
                <Checkbox
                  label="允许交互（仍需来源授权）"
                  disabled={p.source.kind === "image"}
                  checked={p.interactive}
                  onChange={(event) => update({ interactive: event.target.checked })}
                />
                <Button
                  variant="quiet"
                  aria-label={`移除内容 ${p.placementId}`}
                  onClick={() =>
                    onChange({
                      ...layout,
                      placements: layout.placements.filter((entry) => entry.placementId !== p.placementId),
                    })
                  }
                >
                  从草稿移除
                </Button>
              </div>
            </Disclosure>
          );
        })}
      </section>
    </div>
  );
}
