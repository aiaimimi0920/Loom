import { useEffect, useRef, useState } from "react";
import { loadProjectionSettings, saveProjectionSettings, projectionGroupNameError, type ProjectionSettings, type ProjectionSettingsView } from "../../services/loomApi/projectionSettings.ts";
import { errorMessage } from "../../services/loomApi/transport.ts";
import { requestAppConfirmation } from "../feedback/AppFeedback";
import { ProjectionGroupsEditor } from "./ProjectionGroupsEditor";
import { ProjectionRulesEditor } from "./ProjectionRulesEditor";
import "./ProjectionSettingsPanel.css";

export function ProjectionSettingsPanel({ baseUrl, online }: { baseUrl: string; online: boolean }) {
  const [view, setView] = useState<ProjectionSettingsView | null>(null);
  const [dirty, setDirty] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const generation = useRef(0);
  const running = useRef(false);
  useEffect(() => {
    const current = ++generation.current;
    setView(null); setDirty(false); setError(""); setNotice(""); setBusy(online); running.current = online;
    if (online) void loadProjectionSettings(baseUrl).then((next) => {
      if (generation.current === current) setView(next);
    }).catch((reason: unknown) => {
      if (generation.current === current) setError(errorMessage(reason));
    }).finally(() => {
      if (generation.current === current) { running.current = false; setBusy(false); }
    });
    return () => { generation.current++; };
  }, [baseUrl, online]);

  async function perform(save: boolean) {
    if (!online || running.current || (save && (!view || view.settings.groups.some((group) => projectionGroupNameError(group.name))))) return;
    const current = generation.current;
    running.current = true; setBusy(true); setError(""); setNotice("");
    try {
      if (!save && dirty && !await requestAppConfirmation({ title: "刷新投射规则", message: "刷新会放弃当前未保存的修改。", confirmLabel: "放弃并刷新" })) return;
      if (generation.current !== current) return;
      if (save && view) {
        const settings = await saveProjectionSettings(baseUrl, view.settings);
        if (generation.current === current) { setView({ ...view, settings }); setDirty(false); setNotice("投射规则已保存并生效。"); }
      } else {
        const next = await loadProjectionSettings(baseUrl);
        if (generation.current === current) { setView(next); setDirty(false); }
      }
    } catch (reason) {
      if (generation.current === current) setError(errorMessage(reason) + "。操作未获确认，草稿保留；请先刷新核对服务端状态，再重新修改。");
    } finally {
      if (generation.current === current) { running.current = false; setBusy(false); }
    }
  }
  function change(settings: ProjectionSettings) {
    if (!view || busy || !online) return;
    setView({ ...view, settings }); setDirty(true); setNotice("");
  }
  const disabled = busy || !online;
  return <section className="wall-management projection-settings" aria-label="投射规则管理">
    <header><h2>投射规则与设备组</h2><p className="wall-muted">在 Loom 管理连接与接收规则，Hook 中选择目标后即可投射。</p></header>
    {!online && <p role="status">本机 Loom 离线。</p>}
    {busy && <p role="status">正在处理投射配置...</p>}
    {error && <p role="alert" className="wall-error">{error}</p>}
    {notice && <p role="status">{notice}</p>}
    <div className="wall-toolbar">
      <button type="button" className="ghost-button" disabled={disabled} onClick={() => void perform(false)}>刷新配置与设备</button>
      <button type="button" className="signal-button" disabled={disabled || !dirty || !view || view.settings.groups.some((group) => !!projectionGroupNameError(group.name))} onClick={() => void perform(true)}>保存投射配置</button>
      {view && <span className="wall-muted">版本 {view.settings.revision}{dirty ? " · 未保存" : ""}</span>}
    </div>
    {view && <>
      {view.directoryStatus !== "complete" && <p role="status" className="wall-warning">部分对等 Loom 目录暂不可用；已有成员和规则保留，请稍后刷新。</p>}
      <ProjectionRulesEditor devices={view.devices} groups={view.settings.groups} rules={view.settings.rules} disabled={disabled}
        onChange={(rules) => change({ ...view.settings, rules })} />
      <ProjectionGroupsEditor devices={view.devices} groups={view.settings.groups} disabled={disabled}
        onChange={(groups) => change({ ...view.settings, groups, rules: view.settings.rules.map((rule) => ({ ...rule,
          whitelist: { ...rule.whitelist, groups: rule.whitelist.groups.filter((id) => groups.some((group) => group.groupId === id)) } })) })} />
      <p className="wall-muted">名单决定新投射是否接受；修改规则不会删除已接受的贴图。接收端 Hook 必须在线且开启投射接收。</p>
    </>}
  </section>;
}
