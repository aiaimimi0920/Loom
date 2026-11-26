import {
  projectionDeviceKey,
  type ProjectionDeviceChoice,
  type ProjectionDeviceRef,
} from "../../services/loomApi/projectionSettings.ts";
import { CheckboxGroup, EmptyState, MultiSelect } from "@neuro/ui";
import { useEffect, useState } from "react";
import { useUiText } from "../../i18n/index.ts";

export function ProjectionDeviceChecklist(props: {
  label: string;
  devices: ProjectionDeviceChoice[];
  value: ProjectionDeviceRef[];
  onChange: (value: ProjectionDeviceRef[]) => void;
  disabled: boolean;
}) {
  const { t, tf } = useUiText();
  const [dismissed, setDismissed] = useState<ProjectionDeviceRef[]>([]);
  // Explicit catalog reloads may replace candidates; a local uncheck must not remove its own DOM.
  useEffect(() => setDismissed((current) => (current.length ? [] : current)), [props.devices]);
  const options = new Map(props.devices.map((device) => [projectionDeviceKey(device), device]));
  const available = new Set(options.keys());
  for (const device of [...props.value, ...dismissed]) {
    const key = projectionDeviceKey(device);
    if (!options.has(key)) options.set(key, { ...device, name: tf("devices.projection.unavailableDevice", { id: device.deviceId }) });
  }
  const checked = new Set(props.value.map(projectionDeviceKey));
  const change = (values: string[]) => {
    const selected = new Set(values);
    const removed = props.value.filter(
      (device) => !selected.has(projectionDeviceKey(device)) && !available.has(projectionDeviceKey(device)),
    );
    if (removed.length)
      setDismissed((old) => [
        ...new Map([...old, ...removed].map((device) => [projectionDeviceKey(device), device])).values(),
      ]);
    const retained = props.value.filter((device) => selected.has(projectionDeviceKey(device)));
    const added = values
      .filter((key) => !checked.has(key))
      .map((key) => {
        const device = options.get(key)!;
        return { deviceId: device.deviceId, ...(device.peerId ? { peerId: device.peerId } : {}) };
      });
    props.onChange([...retained, ...added]);
  };
  // The protocol caps serialized entries, including retained duplicates, rather than distinct identities.
  const choices = [...options].map(([key, device]) => ({
    value: key,
    label: <span title={device.peerId ? `${device.peerId} / ${device.deviceId}` : device.deviceId}>{device.name}</span>,
    searchText: `${device.name} ${device.peerId ?? ""} ${device.deviceId}`,
    disabled: !checked.has(key) && (!available.has(key) || props.value.length >= 64),
  }));
  return (
    <div className="projection-device-checklist">
      {!choices.length ? (
        <EmptyState title={t("devices.projection.noSelectableDevices")} />
      ) : choices.length > 6 ? (
        <MultiSelect
          label={props.label}
          disabled={props.disabled}
          values={[...checked]}
          options={choices}
          onValuesChange={change}
          search={{
            label: tf("devices.projection.searchLabel", { label: props.label }),
            emptyMessage: t("devices.common.noMatchingDevices"),
          }}
        />
      ) : (
        <CheckboxGroup
          label={props.label}
          disabled={props.disabled}
          values={[...checked]}
          options={choices}
          onValuesChange={change}
          orientation="vertical"
        />
      )}
    </div>
  );
}
