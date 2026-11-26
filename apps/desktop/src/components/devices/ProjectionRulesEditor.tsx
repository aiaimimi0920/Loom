import { useId, useState } from "react";
import { Button, CheckboxGroup, EmptyState, Field, MultiSelect, Notice, SegmentedControl, Select } from "@neuro/ui";
import { useUiText } from "../../i18n/index.ts";
import type {
  ProjectionDeviceChoice,
  ProjectionReceiveRule,
  ProjectionGroup,
  ProjectionReceiveDecision,
} from "../../services/loomApi/projectionSettings.ts";
import { ProjectionDeviceChecklist } from "./ProjectionDeviceChecklist";

export function ProjectionRulesEditor(props: {
  devices: ProjectionDeviceChoice[];
  groups: ProjectionGroup[];
  rules: ProjectionReceiveRule[];
  disabled: boolean;
  onChange: (rules: ProjectionReceiveRule[]) => void;
}) {
  const { t, tf } = useUiText();
  const policyId = useId();
  const [selected, setSelected] = useState("");
  const targets = new Map(
    props.devices.filter((device) => !device.peerId).map((device) => [device.deviceId, device.name]),
  );
  for (const rule of props.rules)
    if (!targets.has(rule.deviceId))
      targets.set(rule.deviceId, tf("devices.projection.unavailableDevice", { id: rule.deviceId }));
  const rule = props.rules.find((rule) => rule.deviceId === selected);
  const change = (next: ProjectionReceiveRule) =>
    props.onChange(props.rules.map((item) => (item.deviceId === next.deviceId ? next : item)));
  return (
    <section className="loom-device-settings__section">
      <h3>{t("devices.projectionRules.heading")}</h3>
      <Field label={t("devices.projectionRules.receiverLabel")}>
        <Select disabled={props.disabled} value={selected} onChange={(event) => setSelected(event.target.value)}>
          <option value="">{t("devices.projectionRules.receiverPlaceholder")}</option>
          {[...targets].map(([id, name]) => (
            <option key={id} value={id}>
              {name}
            </option>
          ))}
        </Select>
      </Field>
      {selected && !rule && (
        <div className="loom-device-settings__actions">
          <Button
            disabled={props.disabled || props.rules.length >= 64}
            onClick={() =>
              props.onChange([
                ...props.rules,
                {
                  deviceId: selected,
                  policy: "confirm",
                  whitelist: { devices: [], groups: [], users: [] },
                  blacklist: [],
                },
              ])
            }
          >
            {t("devices.projectionRules.configureDeviceAction")}
          </Button>
        </div>
      )}
      {rule && (
        <>
          <SegmentedControl<ProjectionReceiveDecision>
            label={t("devices.projectionRules.defaultPolicyLabel")}
            name={policyId}
            disabled={props.disabled}
            value={rule.policy}
            onValueChange={(policy) => change({ ...rule, policy })}
            options={[
              { value: "auto", label: t("devices.projectionRules.policy.auto") },
              { value: "confirm", label: t("devices.projectionRules.policy.confirm") },
              { value: "reject", label: t("devices.projectionRules.policy.reject") },
            ]}
          />
          <ProjectionDeviceChecklist
            key={rule.deviceId + ":whitelist"}
            label={t("devices.projectionRules.whitelistDevicesLabel")}
            devices={props.devices}
            value={rule.whitelist.devices}
            disabled={props.disabled}
            onChange={(devices) => change({ ...rule, whitelist: { ...rule.whitelist, devices } })}
          />
          {props.groups.length > 6 ? (
            <MultiSelect
              label={t("devices.projectionRules.whitelistGroupsLabel")}
              disabled={props.disabled}
              values={rule.whitelist.groups}
              options={props.groups.map((group) => ({ value: group.groupId, label: group.name }))}
              search={{
                label: t("devices.projectionRules.searchWhitelistGroupsLabel"),
                emptyMessage: t("devices.projectionRules.noMatchingGroups"),
              }}
              onValuesChange={(groups) => change({ ...rule, whitelist: { ...rule.whitelist, groups } })}
            />
          ) : props.groups.length ? (
            <CheckboxGroup
              label={t("devices.projectionRules.whitelistGroupsLabel")}
              disabled={props.disabled}
              values={rule.whitelist.groups}
              options={props.groups.map((group) => ({ value: group.groupId, label: group.name }))}
              onValuesChange={(groups) => change({ ...rule, whitelist: { ...rule.whitelist, groups } })}
            />
          ) : (
            <EmptyState title={t("devices.projectionRules.noGroups")} />
          )}
          {rule.whitelist.users.length > 0 && (
            <Notice tone="warning">
              {tf("devices.projectionRules.userWhitelistUnavailable", { count: rule.whitelist.users.length })}
            </Notice>
          )}
          <ProjectionDeviceChecklist
            key={rule.deviceId + ":blacklist"}
            label={t("devices.projectionRules.blacklistDevicesLabel")}
            devices={props.devices}
            value={rule.blacklist}
            disabled={props.disabled}
            onChange={(blacklist) => change({ ...rule, blacklist })}
          />
          <div className="loom-device-settings__actions">
            <Button
              variant="quiet"
              disabled={props.disabled}
              onClick={() => props.onChange(props.rules.filter((item) => item.deviceId !== selected))}
            >
              {t("devices.projectionRules.resetDeviceAction")}
            </Button>
          </div>
        </>
      )}
    </section>
  );
}
