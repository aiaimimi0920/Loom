// Edits which workflow parameters remain runtime inputs versus baked constants.

import { Checkbox, Input, Select } from "@neuro/ui";
import { useUiText } from "../../i18n/index.ts";
import { mapParamUiType } from "../../services/workflowStudio.ts";
import type { NodeParamGroup } from "./otterCanvasThumbnailModel.ts";

interface OtterCanvasParamExposureProps {
  groups: NodeParamGroup[];
  exposed: Set<string>;
  values: Record<string, string>;
  onExposureChange: (key: string, exposed: boolean) => void;
  onValueChange: (key: string, value: string) => void;
}

export function OtterCanvasParamExposure({
  groups,
  exposed,
  values,
  onExposureChange,
  onValueChange,
}: OtterCanvasParamExposureProps) {
  const { t, tf } = useUiText();
  if (!groups.length) return null;
  return (
    <fieldset className="otter-canvas-param-expose">
      <legend>{t("otter.exposure.legend")}</legend>
      {groups.map((group) => (
        <div key={group.workflowNodeId} className="otter-canvas-param-expose__group">
          <p className="otter-canvas-param-expose__node">{group.label}</p>
          {group.rows.map((row) => {
            const isExposed = exposed.has(row.key);
            const uiType = mapParamUiType(row.param);
            const value = values[row.key] ?? "";
            const valueLabel = tf("otter.exposure.constantValueAria", {
              group: group.label,
              param: row.label,
            });
            return (
              <div key={row.key} className="otter-canvas-param-expose__row">
                <Checkbox
                  label={row.label}
                  aria-label={tf("otter.exposure.paramAria", {
                    group: group.label,
                    param: row.label,
                  })}
                  checked={isExposed}
                  onChange={(event) => onExposureChange(row.key, event.target.checked)}
                />
                {uiType === "boolean" ? (
                  <Select
                    className="otter-canvas-param-expose__value"
                    value={value || "false"}
                    disabled={isExposed}
                    onChange={(event) => onValueChange(row.key, event.target.value)}
                    aria-label={valueLabel}
                  >
                    <option value="true">true</option>
                    <option value="false">false</option>
                  </Select>
                ) : uiType === "int" || uiType === "float" ? (
                  <Input
                    className="otter-canvas-param-expose__value"
                    type="number"
                    value={value}
                    disabled={isExposed}
                    placeholder={isExposed ? t("otter.exposure.runtimeInput") : t("otter.exposure.constantValue")}
                    min={row.param.min}
                    max={row.param.max}
                    step={row.param.step ?? (uiType === "int" ? 1 : undefined)}
                    onChange={(event) => onValueChange(row.key, event.target.value)}
                    aria-label={valueLabel}
                  />
                ) : (
                  <Input
                    className="otter-canvas-param-expose__value"
                    value={value}
                    disabled={isExposed}
                    placeholder={isExposed ? t("otter.exposure.runtimeInput") : t("otter.exposure.constantValue")}
                    onChange={(event) => onValueChange(row.key, event.target.value)}
                    aria-label={valueLabel}
                  />
                )}
              </div>
            );
          })}
        </div>
      ))}
    </fieldset>
  );
}
