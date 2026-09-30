import { projectionDeviceKey, type ProjectionDeviceChoice, type ProjectionDeviceRef } from "../../services/loomApi/projectionSettings.ts";

export function ProjectionDeviceChecklist(props: {
  label: string; devices: ProjectionDeviceChoice[]; value: ProjectionDeviceRef[];
  onChange: (value: ProjectionDeviceRef[]) => void; disabled: boolean;
}) {
  const options = new Map(props.devices.map((device) => [projectionDeviceKey(device), device]));
  for (const device of props.value) {
    const key = projectionDeviceKey(device);
    if (!options.has(key)) options.set(key, { ...device, name: device.deviceId + "（目录暂不可用，保留配置）" });
  }
  const checked = new Set(props.value.map(projectionDeviceKey));
  return <fieldset disabled={props.disabled} className="projection-device-checklist">
    <legend>{props.label} · {checked.size}</legend>
    {!options.size && <p className="wall-muted">暂无可选设备</p>}
    <div>{[...options].map(([key, device]) => <label key={key} title={device.peerId ? device.peerId + " / " + device.deviceId : device.deviceId}>
      <input type="checkbox" checked={checked.has(key)} disabled={!checked.has(key) && checked.size >= 64}
        onChange={(event) => props.onChange(event.target.checked
          ? [...props.value, { deviceId: device.deviceId, ...(device.peerId ? { peerId: device.peerId } : {}) }]
          : props.value.filter((entry) => projectionDeviceKey(entry) !== key))} />
      <span>{device.name}</span>
    </label>)}</div>
  </fieldset>;
}
