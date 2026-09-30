import { useId } from "react";
import type { WallRect } from "../../services/loomApi/wallTypes.ts";

export function WallNumber({ label, value, onChange, integer = false }: {
  label: string; value: number; onChange: (value: number) => void; integer?: boolean;
}) {
  const id = useId();
  return <label htmlFor={id}><span>{label}</span><input id={id} className="studio-input" type="number"
    step={integer ? 1 : "any"} value={Number.isFinite(value) ? value : ""}
    onChange={(event) => onChange(event.currentTarget.valueAsNumber)} /></label>;
}
export function WallGeometryFields({ label, rect, onChange }: {
  label: string; rect: WallRect; onChange: (rect: WallRect) => void;
}) {
  return <fieldset className="wall-geometry"><legend>{label}</legend>
    {(["x", "y", "width", "height"] as const).map((key) => <WallNumber key={key}
      label={{ x: "X", y: "Y", width: "宽度", height: "高度" }[key]} value={rect[key]}
      onChange={(value) => onChange({ ...rect, [key]: value })} />)}
  </fieldset>;
}
