import { useState } from "react";
import type { ProjectionDeviceChoice, ProjectionGroup } from "../../services/loomApi/projectionSettings.ts";
import { projectionGroupNameError } from "../../services/loomApi/projectionSettings.ts";
import { ProjectionDeviceChecklist } from "./ProjectionDeviceChecklist";

export function ProjectionGroupsEditor(props: {
  groups: ProjectionGroup[]; devices: ProjectionDeviceChoice[]; disabled: boolean;
  onChange: (groups: ProjectionGroup[]) => void;
}) {
  const [name, setName] = useState("");
  return <section><h3>设备组 · {props.groups.length} / 32</h3>
    <div className="wall-toolbar">
      <label>新设备组名称<input className="studio-input" value={name} maxLength={128} disabled={props.disabled} onChange={(event) => setName(event.target.value)} /></label>
      <button type="button" className="ghost-button" disabled={props.disabled || props.groups.length >= 32 || !!projectionGroupNameError(name.trim())}
        onClick={() => { props.onChange([...props.groups, { groupId: "group-" + crypto.randomUUID(), name: name.trim(), members: [] }]); setName(""); }}>添加设备组</button>
    </div>
    {props.groups.map((group) => <details key={group.groupId}>
      <summary>{group.name} · {group.members.length} 台设备</summary>
      <label>组名称<input className="studio-input" disabled={props.disabled} maxLength={128} value={group.name} aria-invalid={!!projectionGroupNameError(group.name)}
        onChange={(event) => props.onChange(props.groups.map((item) => item.groupId === group.groupId ? { ...item, name: event.target.value } : item))} /></label>
      {projectionGroupNameError(group.name) && <p role="alert" className="wall-error">{projectionGroupNameError(group.name)}</p>}
      <ProjectionDeviceChecklist label="组成员" devices={props.devices} value={group.members} disabled={props.disabled}
        onChange={(members) => props.onChange(props.groups.map((item) => item.groupId === group.groupId ? { ...item, members } : item))} />
      <button type="button" className="ghost-button" disabled={props.disabled} onClick={() => props.onChange(props.groups.filter((item) => item.groupId !== group.groupId))}>移除设备组 {group.name}</button>
    </details>)}
  </section>;
}
