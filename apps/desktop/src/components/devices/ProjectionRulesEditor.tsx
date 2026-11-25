import { useId, useState } from "react";
import { Button, CheckboxGroup, EmptyState, Field, MultiSelect, Notice, SegmentedControl, Select } from "@neuro/ui";
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
  const policyId = useId();
  const [selected, setSelected] = useState("");
  const targets = new Map(
    props.devices.filter((device) => !device.peerId).map((device) => [device.deviceId, device.name]),
  );
  for (const rule of props.rules)
    if (!targets.has(rule.deviceId)) targets.set(rule.deviceId, rule.deviceId + "（不可用）");
  const rule = props.rules.find((rule) => rule.deviceId === selected);
  const change = (next: ProjectionReceiveRule) =>
    props.onChange(props.rules.map((item) => (item.deviceId === next.deviceId ? next : item)));
  return (
    <section className="loom-device-settings__section">
      <h3>投射内容处理</h3>
      <Field label="接收设备">
        <Select disabled={props.disabled} value={selected} onChange={(event) => setSelected(event.target.value)}>
          <option value="">选择本 Loom 的接收设备</option>
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
            配置此设备
          </Button>
        </div>
      )}
      {rule && (
        <>
          <SegmentedControl<ProjectionReceiveDecision>
            label="通用规则"
            name={policyId}
            disabled={props.disabled}
            value={rule.policy}
            onValueChange={(policy) => change({ ...rule, policy })}
            options={[
              { value: "auto", label: "接受" },
              { value: "confirm", label: "询问" },
              { value: "reject", label: "拒绝" },
            ]}
          />
          <ProjectionDeviceChecklist
            key={rule.deviceId + ":whitelist"}
            label="白名单设备"
            devices={props.devices}
            value={rule.whitelist.devices}
            disabled={props.disabled}
            onChange={(devices) => change({ ...rule, whitelist: { ...rule.whitelist, devices } })}
          />
          {props.groups.length > 6 ? (
            <MultiSelect
              label="白名单设备组"
              disabled={props.disabled}
              values={rule.whitelist.groups}
              options={props.groups.map((group) => ({ value: group.groupId, label: group.name }))}
              search={{ label: "搜索白名单设备组", emptyMessage: "没有匹配设备组" }}
              onValuesChange={(groups) => change({ ...rule, whitelist: { ...rule.whitelist, groups } })}
            />
          ) : props.groups.length ? (
            <CheckboxGroup
              label="白名单设备组"
              disabled={props.disabled}
              values={rule.whitelist.groups}
              options={props.groups.map((group) => ({ value: group.groupId, label: group.name }))}
              onValuesChange={(groups) => change({ ...rule, whitelist: { ...rule.whitelist, groups } })}
            />
          ) : (
            <EmptyState title="暂无设备组" />
          )}
          {rule.whitelist.users.length > 0 && (
            <Notice tone="warning">用户白名单：{rule.whitelist.users.length} 项，当前不可用。</Notice>
          )}
          <ProjectionDeviceChecklist
            key={rule.deviceId + ":blacklist"}
            label="黑名单设备"
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
              恢复此设备默认规则
            </Button>
          </div>
        </>
      )}
    </section>
  );
}
