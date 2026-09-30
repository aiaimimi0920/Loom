import { useState } from "react";
import type { ProjectionDeviceChoice, ProjectionReceiveRule, ProjectionGroup, ProjectionReceiveDecision } from "../../services/loomApi/projectionSettings.ts";
import { ProjectionDeviceChecklist } from "./ProjectionDeviceChecklist";

export function ProjectionRulesEditor(props: {
  devices: ProjectionDeviceChoice[]; groups: ProjectionGroup[]; rules: ProjectionReceiveRule[];
  disabled: boolean; onChange: (rules: ProjectionReceiveRule[]) => void;
}) {
  const [selected, setSelected] = useState("");
  const targets = new Map(props.devices.filter((device) => !device.peerId).map((device) => [device.deviceId, device.name]));
  for (const rule of props.rules) if (!targets.has(rule.deviceId)) targets.set(rule.deviceId, rule.deviceId + "（不可用）");
  const rule = props.rules.find((rule) => rule.deviceId === selected);
  const change = (next: ProjectionReceiveRule) => props.onChange(props.rules.map((item) => item.deviceId === next.deviceId ? next : item));
  return <section><h3>投射内容处理</h3>
    <p className="wall-muted">白名单优先于黑名单；均未命中时应用通用规则。未配置的设备默认提示确认。</p>
    <label>接收设备<select aria-label="接收设备" className="studio-input" disabled={props.disabled} value={selected} onChange={(event) => setSelected(event.target.value)}>
      <option value="">选择本 Loom 的接收设备</option>
      {[...targets].map(([id, name]) => <option key={id} value={id}>{name}</option>)}
    </select></label>
    {selected && !rule && <button type="button" className="ghost-button" disabled={props.disabled || props.rules.length >= 64}
      onClick={() => props.onChange([...props.rules, { deviceId: selected, policy: "confirm", whitelist: { devices: [], groups: [], users: [] }, blacklist: [] }])}>配置此设备</button>}
    {rule && <>
      <label>通用规则<select aria-label="通用规则" className="studio-input" disabled={props.disabled} value={rule.policy}
        onChange={(event) => change({ ...rule, policy: event.target.value as ProjectionReceiveDecision })}>
        <option value="auto">自动接受投射</option><option value="confirm">弹出提示框由用户选择是否接受</option><option value="reject">自动拒绝所有投射</option>
      </select></label>
      <ProjectionDeviceChecklist label="白名单设备" devices={props.devices} value={rule.whitelist.devices} disabled={props.disabled}
        onChange={(devices) => change({ ...rule, whitelist: { ...rule.whitelist, devices } })} />
      <fieldset disabled={props.disabled} className="projection-device-checklist"><legend>白名单设备组</legend><div>
        {props.groups.map((group) => <label key={group.groupId}><input type="checkbox" checked={rule.whitelist.groups.includes(group.groupId)}
          onChange={(event) => change({ ...rule, whitelist: { ...rule.whitelist, groups: event.target.checked
            ? [...rule.whitelist.groups, group.groupId] : rule.whitelist.groups.filter((id) => id !== group.groupId) } })} />{group.name}</label>)}
        {!props.groups.length && <p className="wall-muted">请先添加设备组</p>}
      </div></fieldset>
      <p className="wall-muted">用户白名单：{rule.whitelist.users.length} 项。官方账号认证尚未接入，此类规则暂不匹配。</p>
      <ProjectionDeviceChecklist label="黑名单设备" devices={props.devices} value={rule.blacklist} disabled={props.disabled}
        onChange={(blacklist) => change({ ...rule, blacklist })} />
      <button type="button" className="ghost-button" disabled={props.disabled} onClick={() => props.onChange(props.rules.filter((item) => item.deviceId !== selected))}>恢复此设备默认规则</button>
    </>}
  </section>;
}
