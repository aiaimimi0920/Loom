import { useEffect, useRef, useState } from "react";
import { decideOtterConnection, loadOtterConnections, saveOtterConnectionPolicy,
  type OtterConnectionPolicyDraft, type OtterConnectionsView, type OtterApprovalPolicy } from "../../services/loomApi/otterConnections.ts";
import { errorMessage } from "../../services/loomApi/transport.ts";
import "./OtterConnectionSettings.css";
import { LoomLanSettings } from "./LoomLanSettings.tsx";

export function OtterConnectionSettings({ baseUrl, online }: { baseUrl: string; online: boolean }) {
  const [view, setView] = useState<OtterConnectionsView | null>(null);
  const [draft, setDraft] = useState<OtterConnectionPolicyDraft | null>(null);
  const [password, setPassword] = useState("");
  const [passwordEnabled, setPasswordEnabled] = useState(false);
  const initialized = useRef(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const epoch = useRef(0);
  const readVersion = useRef(0);
  const running = useRef(false);

  useEffect(() => {
    const current = ++epoch.current;
    let refreshing = false;
    setView(null); setDraft(null); setPassword(""); setError(""); setNotice(""); setPasswordEnabled(false); setBusy(false); initialized.current = false;
    const refresh = async () => {
      if (!online || refreshing || running.current) return;
      refreshing = true;
      const read = ++readVersion.current;
      try {
        const next = await loadOtterConnections(baseUrl);
        if (current !== epoch.current || read !== readVersion.current) return;
        setView(next);
        if (!initialized.current) { setPasswordEnabled(next.loom.passwordRequired); initialized.current = true; }
        setDraft((old) => old ?? { name: next.loom.name, endpoint: null,
          discoverable: false, approval: next.loom.approval });
      } catch (reason) { if (current === epoch.current && read === readVersion.current) setError(errorMessage(reason)); }
      finally { refreshing = false; }
    };
    void refresh();
    const timer = setInterval(() => void refresh(), 2000);
    return () => { epoch.current++; clearInterval(timer); };
  }, [baseUrl, online]);

  async function perform(action: () => Promise<unknown>, saved: boolean) {
    if (running.current || !online) return;
    const current = epoch.current;
    readVersion.current++;
    running.current = true; setBusy(true); setError(""); setNotice("");
    try {
      await action();
      if (current !== epoch.current) return;
      if (saved) { setPassword(""); setNotice("已保存"); }
      const next = await loadOtterConnections(baseUrl);
      if (current === epoch.current) setView(next);
    } catch (reason) { if (current === epoch.current) setError(errorMessage(reason)); }
    finally { running.current = false; if (current === epoch.current) setBusy(false); }
  }
  const change = (patch: Partial<OtterConnectionPolicyDraft>) => { if (draft) setDraft({ ...draft, ...patch }); setNotice(""); };
  return <section className="otter-connection-settings" aria-label="Otter 连接设置">
    <header><h2>Otter 连接</h2><code title={view?.loom.loomId}>{view?.loom.loomId ?? "-"}</code></header>
    {!online && <p role="status">Loom 离线</p>}
    {error && <p className="wall-error" role="alert">{error}</p>}
    {notice && <p role="status">{notice}</p>}
    <LoomLanSettings baseUrl={baseUrl} online={online} />
    {draft && <form onSubmit={(event) => {
      event.preventDefault();
      void perform(() => saveOtterConnectionPolicy(baseUrl, { ...draft,
        ...(!passwordEnabled ? { password: "" } : password ? { password } : {}) }), true);
    }}>
      <fieldset disabled={!online || busy}>
        <label>名称<input maxLength={80} value={draft.name} onChange={(event) => change({ name: event.target.value })} required /></label>
        <label>审批<select value={draft.approval} onChange={(event) => change({ approval: event.target.value as OtterApprovalPolicy })}>
          <option value="manual">手动批准</option><option value="lan">局域网内自动批准</option><option value="authenticated">连接过的设备自动批准</option>
        </select></label>
        <label>启用连接码<input type="checkbox" checked={passwordEnabled} onChange={(event) => { setPasswordEnabled(event.target.checked); setNotice(""); }} /></label>
        {passwordEnabled && <label>连接码<input type="password" autoComplete="new-password" minLength={8} maxLength={256} value={password}
          required={!view?.loom.passwordRequired} placeholder={view?.loom.passwordRequired ? "已设置 · 留空保留" : "至少 8 位"} onChange={(event) => setPassword(event.target.value)} /></label>}
        <button type="submit" className="signal-button">{busy ? "处理中…" : "保存"}</button>
      </fieldset>
    </form>}
    <div className="otter-connection-requests" aria-label="待批准连接">
      <h3>待批准连接</h3>
      {view?.requests.length === 0 && <span>暂无申请</span>}
      {view?.requests.map((request) => <div className="otter-connection-request" key={request.id}>
        <span title={request.deviceId}>{request.name}</span>
        <button className="ghost-button" type="button" disabled={busy || !online} onClick={() => void perform(() => decideOtterConnection(baseUrl, request.id, false), false)}>拒绝</button>
        <button className="ghost-button" type="button" disabled={busy || !online} onClick={() => void perform(() => decideOtterConnection(baseUrl, request.id, true), false)}>批准</button>
      </div>)}
    </div>
  </section>;
}
