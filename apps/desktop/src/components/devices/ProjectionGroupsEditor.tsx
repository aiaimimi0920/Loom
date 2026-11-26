import { useState } from "react";
import { Badge, Button, Disclosure, Field, Input, Notice } from "@neuro/ui";
import { useUiText } from "../../i18n/index.ts";
import type { ProjectionDeviceChoice, ProjectionGroup } from "../../services/loomApi/projectionSettings.ts";
import { projectionGroupNameError } from "../../services/loomApi/projectionSettings.ts";
import { ProjectionDeviceChecklist } from "./ProjectionDeviceChecklist";

export function ProjectionGroupsEditor(props: {
  groups: ProjectionGroup[];
  devices: ProjectionDeviceChoice[];
  disabled: boolean;
  onChange: (groups: ProjectionGroup[]) => void;
}) {
  const { t, tf } = useUiText();
  const [name, setName] = useState("");
  return (
    <section className="loom-device-settings__section">
      <div className="loom-device-settings__metadata">
        <h3>{t("devices.projectionGroups.heading")}</h3>
        <Badge>{props.groups.length} / 32</Badge>
      </div>
      <div className="projection-group-create">
        <Field label={t("devices.projectionGroups.newNameLabel")}>
          <Input
            value={name}
            maxLength={128}
            disabled={props.disabled}
            onChange={(event) => setName(event.target.value)}
          />
        </Field>
        <Button
          disabled={props.disabled || props.groups.length >= 32 || !!projectionGroupNameError(name.trim())}
          onClick={() => {
            props.onChange([
              ...props.groups,
              { groupId: "group-" + crypto.randomUUID(), name: name.trim(), members: [] },
            ]);
            setName("");
          }}
        >
          {t("devices.projectionGroups.addAction")}
        </Button>
      </div>
      {props.groups.map((group) => (
        <Disclosure
          key={group.groupId}
          className="projection-group"
          title={
            <>
              <span className="projection-group__name" title={group.name}>
                {group.name}
              </span>
              <Badge className="projection-group__count">
                {tf("devices.projectionGroups.memberCount", { count: group.members.length })}
              </Badge>
            </>
          }
        >
          <div className="loom-device-settings__section">
            <Field label={t("devices.projectionGroups.nameLabel")}>
              <Input
                disabled={props.disabled}
                maxLength={128}
                value={group.name}
                aria-invalid={!!projectionGroupNameError(group.name)}
                onChange={(event) =>
                  props.onChange(
                    props.groups.map((item) =>
                      item.groupId === group.groupId ? { ...item, name: event.target.value } : item,
                    ),
                  )
                }
              />
            </Field>
            {projectionGroupNameError(group.name) && (
              <Notice tone="danger">{projectionGroupNameError(group.name)}</Notice>
            )}
            <ProjectionDeviceChecklist
              label={t("devices.projectionGroups.membersLabel")}
              devices={props.devices}
              value={group.members}
              disabled={props.disabled}
              onChange={(members) =>
                props.onChange(
                  props.groups.map((item) => (item.groupId === group.groupId ? { ...item, members } : item)),
                )
              }
            />
            <div className="loom-device-settings__actions">
              <Button
                variant="danger"
                aria-label={tf("devices.projectionGroups.removeAriaLabel", { name: group.name })}
                disabled={props.disabled}
                onClick={() => props.onChange(props.groups.filter((item) => item.groupId !== group.groupId))}
              >
                {t("devices.projectionGroups.removeAction")}
              </Button>
            </div>
          </div>
        </Disclosure>
      ))}
    </section>
  );
}
