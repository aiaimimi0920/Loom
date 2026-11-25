import { useId } from "react";
import { Field, Input } from "@neuro/ui";
import type { WallRect } from "../../services/loomApi/wallTypes.ts";

export function WallNumber({
  label,
  value,
  onChange,
  integer = false,
}: {
  label: string;
  value: number;
  onChange: (value: number) => void;
  integer?: boolean;
}) {
  const id = useId();
  return (
    <Field label={label}>
      <Input
        id={id}
        type="number"
        compact
        step={integer ? 1 : "any"}
        value={Number.isFinite(value) ? value : ""}
        onChange={(event) => onChange(event.currentTarget.valueAsNumber)}
      />
    </Field>
  );
}
export function WallGeometryFields({
  label,
  rect,
  onChange,
}: {
  label: string;
  rect: WallRect;
  onChange: (rect: WallRect) => void;
}) {
  return (
    <fieldset className="wall-geometry">
      <legend>{label}</legend>
      {(["x", "y", "width", "height"] as const).map((key) => (
        <WallNumber
          key={key}
          label={{ x: "X", y: "Y", width: "宽度", height: "高度" }[key]}
          value={rect[key]}
          onChange={(value) => onChange({ ...rect, [key]: value })}
        />
      ))}
    </fieldset>
  );
}
