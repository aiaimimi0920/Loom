import { useEffect, useRef, useState } from "react";
import { listProjectionPeers, probeProjectionPeer, removeProjectionPeer, saveProjectionPeer,
  type ProjectionPeer, type ProjectionPeerState } from "../../services/loomApi/projectionPeers.ts";
import { errorMessage } from "../../services/loomApi/transport.ts";
import { requestAppConfirmation } from "../feedback/AppFeedback";
import "./ProjectionPeersPanel.css";

const emptyPeer = (): ProjectionPeer => ({ peerId: "", publicKey: "", name: "", origin: "", enabled: true });
const trustWarning = "修改任一对等连接会终止此 Loom 现有的跨 Loom 投射关联，需要重新投送。请先核对对方公开身份。";

export function ProjectionPeersPanel({ baseUrl, online }: { baseUrl: string; online: boolean }) {
  const [state, setState] = useState<ProjectionPeerState | null>(null);
  const [draft, setDraft] = useState<ProjectionPeer>(emptyPeer);
  const [editing, setEditing] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const active = useRef(0);
  const inFlight = useRef(false);
  useEffect(() => {
    const generation = ++active.current;
    setState(null); setError(""); setNotice(""); setDraft(emptyPeer()); setEditing(false);
    inFlight.current = online; setBusy(online);
    if (online) void listProjectionPeers(baseUrl).then((next) => {
      if (active.current === generation) setState(next);
    }).catch((reason: unknown) => {
      if (active.current === generation) setError(errorMessage(reason));
    }).finally(() => {
      if (active.current === generation) { inFlight.current = false; setBusy(false); }
    });
    return () => { active.current++; };
  }, [baseUrl, online]);

  async function run(action: () => Promise<void>) {
    if (!online || inFlight.current) return;
    const generation = active.current;
    inFlight.current = true; setBusy(true); setError(""); setNotice("");
    try { await action(); } catch (reason) {
      if (active.current === generation) setError(errorMessage(reason) + "。请刷新配置后重试；不会自动覆盖并发修改。");
    } finally {
      if (active.current === generation) { inFlight.current = false; setBusy(false); }
    }
  }
  function validGeneration(generation: number) { return active.current === generation; }
  async function refresh() {
    const generation = active.current;
    const next = await listProjectionPeers(baseUrl);
    if (validGeneration(generation)) setState(next);
  }
  async function mutate(remove: boolean) {
    if (!state) return;
    const generation = active.current;
    const accepted = await requestAppConfirmation({ title: remove ? "移除对等 Loom" : "保存对等 Loom",
      message: trustWarning, confirmLabel: remove ? "移除" : "保存" });
    if (!accepted || !validGeneration(generation)) return;
    const next = remove ? await removeProjectionPeer(baseUrl, state.revision, draft.peerId)
      : await saveProjectionPeer(baseUrl, state.revision, { ...draft, name: draft.name.trim(), origin: draft.origin.trim(),
        peerId: draft.peerId.trim(), publicKey: draft.publicKey.trim() });
    if (!validGeneration(generation)) return;
    setState(next); setDraft(emptyPeer()); setEditing(false);
    setNotice(remove ? "对等连接已移除。" : "连接配置已保存。请在对方 Loom 配置本机公开身份，再验证连接。");
  }
  async function probe(peer: ProjectionPeer) {
    if (!state) return;
    const generation = active.current;
    await probeProjectionPeer(baseUrl, state.revision, peer.peerId);
    if (validGeneration(generation)) setNotice(peer.name + "：已验证双方信任及签名连接。本次检查不表示贴图已经送达。");
  }
  const disabled = busy || !online;
  return <section className="wall-management projection-peers" aria-label="投射连接管理">
    <header><h2>跨 Loom 投射连接</h2><p className="wall-muted">在双方 Loom 中登记对方公开身份和地址。Hook 会自动列出已授权且在线的接收设备。</p></header>
    {!online && <p role="status">本机 Loom 离线。</p>}
    {error && <p role="alert" className="wall-error">{error}</p>}
    {notice && <p role="status">{notice}</p>}
    <button type="button" className="ghost-button" disabled={disabled} onClick={() => void run(refresh)}>刷新配置</button>
    {state && <>
      <details><summary>本机公开身份</summary>
        <label>身份指纹<input className="studio-input" readOnly value={state.identity.peerId} /></label>
        <label>公钥<input className="studio-input" readOnly value={state.identity.publicKey} /></label>
        <p className="wall-muted">仅交换这两个公开字段，不交换管理员令牌或私钥文件。</p>
      </details>
      <p className="wall-muted">{state.peers.length} / 16 个连接 · 配置版本 {state.revision}</p>
      {state.peers.length === 0 && <p>尚未添加其他 Loom。</p>}
      {state.peers.map((peer) => <div className="wall-toolbar" key={peer.peerId}>
        <span>{peer.name} · {peer.enabled ? "已启用" : "已停用"}</span>
        <span className="wall-muted">{peer.origin}</span>
        <button type="button" className="ghost-button" disabled={disabled} onClick={() => { setDraft({ ...peer }); setEditing(true); setNotice(""); }}>编辑 {peer.name}</button>
        <button type="button" className="ghost-button" disabled={disabled || !peer.enabled} onClick={() => void run(() => probe(peer))}>验证 {peer.name}</button>
      </div>)}
      <form onSubmit={(event) => { event.preventDefault(); void run(() => mutate(false)); }}>
        <h3>{editing ? "编辑连接" : "添加连接"}</h3>
        <fieldset disabled={disabled}>
          <label>名称<input className="studio-input" required maxLength={128} value={draft.name} onChange={(event) => setDraft({ ...draft, name: event.target.value })} /></label>
          <label>Loom 地址<input className="studio-input" required maxLength={256} placeholder="https://loom.example.test" value={draft.origin} onChange={(event) => setDraft({ ...draft, origin: event.target.value })} /></label>
          <label>对方身份指纹<input className="studio-input" required readOnly={editing} maxLength={69} value={draft.peerId} onChange={(event) => setDraft({ ...draft, peerId: event.target.value })} /></label>
          <label>对方公钥<input className="studio-input" required readOnly={editing} maxLength={44} value={draft.publicKey} onChange={(event) => setDraft({ ...draft, publicKey: event.target.value })} /></label>
          <label><input type="checkbox" checked={draft.enabled} onChange={(event) => setDraft({ ...draft, enabled: event.target.checked })} />启用连接</label>
        </fieldset>
        <p className="wall-muted">非回环地址必须使用可信 HTTPS；换密钥需要先移除原连接。{trustWarning}</p>
        <div className="wall-toolbar">
          <button type="submit" className="signal-button" disabled={disabled || (!editing && state.peers.length >= 16)}>保存连接</button>
          {editing && <button type="button" className="ghost-button" disabled={disabled} onClick={() => void run(() => mutate(true))}>移除连接</button>}
          <button type="button" className="ghost-button" disabled={disabled} onClick={() => { setDraft(emptyPeer()); setEditing(false); }}>清空编辑</button>
        </div>
      </form>
    </>}
  </section>;
}
