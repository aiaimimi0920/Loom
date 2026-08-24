// Edits which workflow parameters remain runtime inputs versus baked constants.

import { mapParamUiType } from "../../services/workflowStudio.ts";
import type { NodeParamGroup } from "./hookCanvasThumbnailModel.ts";

interface HookCanvasParamExposureProps {
  groups: NodeParamGroup[];
  exposed: Set<string>;
  values: Record<string, string>;
  onExposureChange: (key: string, exposed: boolean) => void;
  onValueChange: (key: string, value: string) => void;
}

export function HookCanvasParamExposure({
  groups,
  exposed,
  values,
  onExposureChange,
  onValueChange,
}: HookCanvasParamExposureProps) {
  if (!groups.length) return null;
  return (
    <div className="hook-canvas-param-expose">
      <p className="hook-canvas-param-expose__hint">
        勾选要外露的参数，未勾选的将以当前值固定。外露的参数会成为封装节点的输入。
      </p>
      {groups.map((group) => (
        <div key={group.workflowNodeId} className="hook-canvas-param-expose__group">
          <p className="hook-canvas-param-expose__node">{group.label}</p>
          {group.rows.map((row) => {
            const isExposed = exposed.has(row.key);
            const uiType = mapParamUiType(row.param);
            const value = values[row.key] ?? "";
            const valueLabel = `${group.label} / ${row.label} 常量值`;
            return (
              <div key={row.key} className="hook-canvas-param-expose__row">
                <label className="hook-canvas-param-expose__toggle">
                  <input
                    type="checkbox"
                    checked={isExposed}
                    onChange={(event) => onExposureChange(row.key, event.target.checked)}
                  />
                  <span>{row.label}</span>
                </label>
                {uiType === "boolean" ? (
                  <select
                    className="hook-canvas-param-expose__value"
                    value={value || "false"}
                    disabled={isExposed}
                    onChange={(event) => onValueChange(row.key, event.target.value)}
                    aria-label={valueLabel}
                  >
                    <option value="true">true</option>
                    <option value="false">false</option>
                  </select>
                ) : uiType === "int" || uiType === "float" ? (
                  <input
                    className="hook-canvas-param-expose__value"
                    type="number"
                    value={value}
                    disabled={isExposed}
                    placeholder={isExposed ? "运行时输入" : "常量值"}
                    min={row.param.min}
                    max={row.param.max}
                    step={row.param.step ?? (uiType === "int" ? 1 : undefined)}
                    onChange={(event) => onValueChange(row.key, event.target.value)}
                    aria-label={valueLabel}
                  />
                ) : (
                  <input
                    className="hook-canvas-param-expose__value"
                    value={value}
                    disabled={isExposed}
                    placeholder={isExposed ? "运行时输入" : "常量值"}
                    onChange={(event) => onValueChange(row.key, event.target.value)}
                    aria-label={valueLabel}
                  />
                )}
              </div>
            );
          })}
        </div>
      ))}
    </div>
  );
}
