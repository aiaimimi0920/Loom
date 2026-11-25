import { useEffect, useRef, useState } from "react";
import { Badge, Button, Notice } from "@neuro/ui";
import {
  loadProjectionSettings,
  saveProjectionSettings,
  projectionGroupNameError,
  type ProjectionSettings,
  type ProjectionSettingsView,
} from "../../services/loomApi/projectionSettings.ts";
import { errorMessage } from "../../services/loomApi/transport.ts";
import { requestAppConfirmation } from "../feedback/AppFeedback";
import { ProjectionGroupsEditor } from "./ProjectionGroupsEditor";
import { ProjectionRulesEditor } from "./ProjectionRulesEditor";
import "./ProjectionSettingsPanel.css";
import "./DeviceConnectionLayout.css";

export function ProjectionSettingsPanel({ baseUrl, online }: { baseUrl: string; online: boolean }) {
  const [view, setView] = useState<ProjectionSettingsView | null>(null);
  const [dirty, setDirty] = useState(false);
  const [busy, setBusy] = useState(false);
  const [operation, setOperation] = useState<"save" | "refresh" | null>(null);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const generation = useRef(0);
  const running = useRef(false);
  useEffect(() => {
    const current = ++generation.current;
    setView(null);
    setDirty(false);
    setError("");
    setNotice("");
    setBusy(online);
    running.current = online;
    setOperation(online ? "refresh" : null);
    if (online)
      void loadProjectionSettings(baseUrl)
        .then((next) => {
          if (generation.current === current) setView(next);
        })
        .catch((reason: unknown) => {
          if (generation.current === current) setError(errorMessage(reason));
        })
        .finally(() => {
          if (generation.current === current) {
            running.current = false;
            setBusy(false);
            setOperation(null);
          }
        });
    return () => {
      generation.current++;
    };
  }, [baseUrl, online]);

  async function perform(save: boolean) {
    if (
      !online ||
      running.current ||
      (save && (!view || view.settings.groups.some((group) => projectionGroupNameError(group.name))))
    )
      return;
    const current = generation.current;
    running.current = true;
    setBusy(true);
    setError("");
    setNotice("");
    setOperation(save ? "save" : "refresh");
    try {
      if (
        !save &&
        dirty &&
        !(await requestAppConfirmation({
          title: "刷新投射规则",
          message: "刷新会放弃当前未保存的修改。",
          confirmLabel: "放弃并刷新",
        }))
      )
        return;
      if (generation.current !== current) return;
      if (save && view) {
        const settings = await saveProjectionSettings(baseUrl, view.settings);
        if (generation.current === current) {
          setView({ ...view, settings });
          setDirty(false);
          setNotice("投射规则已保存并生效。");
        }
      } else {
        const next = await loadProjectionSettings(baseUrl);
        if (generation.current === current) {
          setView(next);
          setDirty(false);
        }
      }
    } catch (reason) {
      if (generation.current === current)
        setError(errorMessage(reason) + "。操作未获确认，草稿保留；请先刷新核对服务端状态，再重新修改。");
    } finally {
      if (generation.current === current) {
        running.current = false;
        setBusy(false);
        setOperation(null);
      }
    }
  }
  function change(settings: ProjectionSettings) {
    if (!view || busy || !online) return;
    setView({ ...view, settings });
    setDirty(true);
    setNotice("");
  }
  const disabled = busy || !online;
  return (
    <section
      className="loom-device-settings projection-settings"
      aria-label="投射规则管理"
      aria-busy={busy || undefined}
    >
      <header>
        <h2>投射规则与设备组</h2>
      </header>
      {!online && <Notice tone="warning">本机 Loom 离线。</Notice>}
      {error && <Notice tone="danger">{error}</Notice>}
      {notice && <Notice tone="success">{notice}</Notice>}
      <div className="loom-device-settings__actions">
        <Button
          variant="quiet"
          loading={operation === "refresh"}
          disabled={disabled}
          onClick={() => void perform(false)}
        >
          刷新配置与设备
        </Button>
        <Button
          variant="primary"
          loading={operation === "save"}
          disabled={
            disabled || !dirty || !view || view.settings.groups.some((group) => !!projectionGroupNameError(group.name))
          }
          onClick={() => void perform(true)}
        >
          保存投射配置
        </Button>
        {view && (
          <>
            <code>版本 {view.settings.revision}</code>
            <Badge tone={dirty ? "warning" : "neutral"}>{dirty ? "未保存" : "已保存"}</Badge>
          </>
        )}
      </div>
      {view && (
        <>
          {view.directoryStatus !== "complete" && (
            <Notice tone="warning">部分对等 Loom 目录暂不可用；已有成员和规则保留，请稍后刷新。</Notice>
          )}
          <ProjectionRulesEditor
            devices={view.devices}
            groups={view.settings.groups}
            rules={view.settings.rules}
            disabled={disabled}
            onChange={(rules) => change({ ...view.settings, rules })}
          />
          <ProjectionGroupsEditor
            devices={view.devices}
            groups={view.settings.groups}
            disabled={disabled}
            onChange={(groups) =>
              change({
                ...view.settings,
                groups,
                rules: view.settings.rules.map((rule) => ({
                  ...rule,
                  whitelist: {
                    ...rule.whitelist,
                    groups: rule.whitelist.groups.filter((id) => groups.some((group) => group.groupId === id)),
                  },
                })),
              })
            }
          />
        </>
      )}
    </section>
  );
}
