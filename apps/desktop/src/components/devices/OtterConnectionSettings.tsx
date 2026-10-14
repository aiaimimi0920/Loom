import { useEffect, useRef, useState } from "react";
import { decideOtterConnection, loadOtterConnections, saveOtterConnectionPolicy,
  type OtterConnectionPolicyDraft, type OtterConnectionsView, type OtterApprovalPolicy } from "../../services/loomApi/otterConnections.ts";
import { errorMessage } from "../../services/loomApi/transport.ts";
import "./OtterConnectionSettings.css";

export function OtterConnectionSettings({ baseUrl, online }: { baseUrl: string; online: boolean }) {
  const [view, setView] = useState<OtterConnectionsView | null>(null);
  const [draft, setDraft] = useState<OtterConnectionPolicyDraft | null>(null);
  const [password, setPassword] = useState("");
  const [clearPassword, setClearPassword] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const epoch = useRef(0);
  const readVersion = useRef(0);
  const running = useRef(false);

  useEffect(() => {
    const current = ++epoch.current;
    let refreshing = false;
    setView(null); setDraft(null); setPassword(""); setError(""); setNotice(""); setClearPassword(false); setBusy(false);
    const refresh = async () => {
      if (!online || refreshing || running.current) return;
      refreshing = true;
      const read = ++readVersion.current;
      try {
        const next = await loadOtterConnections(baseUrl);
        if (current !== epoch.current || read !== readVersion.current) return;
        setView(next);
        setDraft((old) => old ?? { name: next.loom.name, endpoint: next.loom.endpoint,
          discoverable: next.loom.discoverable, approval: next.loom.approval });
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
      if (saved) { setPassword(""); setClearPassword(false); setNotice("已保存"); }
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
    {draft && <form onSubmit={(event) => {
      event.preventDefault();
      void perform(() => saveOtterConnectionPolicy(baseUrl, { ...draft,
        ...(clearPassword ? { password: "" } : password ? { password } : {}) }), true);
    }}>
      <fieldset disabled={!online || busy}>
        <label>名称<input maxLength={80} value={draft.name} onChange={(event) => change({ name: event.target.value })} required /></label>
        <label>公开发现<input type="checkbox" checked={draft.discoverable} onChange={(event) => change({ discoverable: event.target.checked })} /></label>
        <label>HTTPS 地址<input type="url" value={draft.endpoint ?? ""} onChange={(event) => change({ endpoint: event.target.value || null })} required={draft.discoverable} /></label>
        <label>审批<select value={draft.approval} onChange={(event) => change({ approval: event.target.value as OtterApprovalPolicy })}>
          <option value="manual">每次手动批准</option><option value="password">密码正确自动连接</option><option value="authenticated">已认证设备自动连接</option>
        </select></label>
        <label>连接密码<input type="password" autoComplete="new-password" maxLength={256} value={password} disabled={clearPassword}
          placeholder={view?.loom.passwordRequired ? "已设置 · 留空保留" : "未设置"} onChange={(event) => setPassword(event.target.value)} /></label>
        {view?.loom.passwordRequired && <label>清除密码<input type="checkbox" checked={clearPassword} onChange={(event) => setClearPassword(event.target.checked)} /></label>}
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
