import { useEffect, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { cloudError, naccountRequest, type NAccountView } from "../../services/naccount";
import { CloudResourcesPanel } from "./CloudResourcesPanel";
import { CloudDownloadsPanel } from "./CloudDownloadsPanel";

export function NAccountSettingsPanel({ baseUrl }: { baseUrl: string }) {
  const [view, setView] = useState<NAccountView | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const generation = useRef(0);
  useEffect(() => {
    const token = ++generation.current;
    setBusy(true); setError(""); setView(null);
    void naccountRequest(baseUrl, "status").then(async (next) => {
      if (next.status === "signed_in") next = await naccountRequest(baseUrl, "refresh");
      if (token === generation.current) setView(next);
    }).catch((e: unknown) => { if (token === generation.current) setError(cloudError(e)); })
      .finally(() => { if (token === generation.current) setBusy(false); });
    return () => { generation.current++; };
  }, [baseUrl]);
  async function action(name: "start" | "status" | "refresh" | "logout") {
    const token = ++generation.current;
    setBusy(true); setError("");
    try {
      const next = await naccountRequest(baseUrl, name);
      if (token === generation.current) setView(next);
    } catch (e) { if (token === generation.current) setError(cloudError(e)); }
    finally { if (token === generation.current) setBusy(false); }
  }
  useEffect(() => {
    if (view?.status !== "pending" || busy || error) return;
    const timer = window.setTimeout(() => { void action("status"); }, 2000);
    return () => window.clearTimeout(timer);
  }, [view, busy, error, baseUrl]);
  async function openLogin() {
    if (view?.status !== "pending") return;
    try {
      if (isTauri()) await invoke("open_loom_naccount_login", { baseUrl, requestId: view.requestId });
      else window.open(view.authorizationUrl, "_blank", "noopener,noreferrer");
    } catch { setError("无法打开登录页面，请重试。"); }
  }
  return <section className="settings-general-panel" aria-label="NAccount">
    <header className="settings-general-panel__header"><strong>NAccount</strong></header>
    {!view && !busy && <p>尚未读取登录状态</p>}
    {view?.status === "signed_out" && <>
      <p>{view.configured ? "未登录" : "客户端尚未配置"}</p>
      <button className="primary-button" disabled={busy || !view.configured} onClick={() => void action("start")}>登录 NAccount</button>
      {view.remoteRevoked === false && <p role="status">已退出本机；远端会话撤销未完成。</p>}
    </>}
    {view?.status === "pending" && <>
      <button className="primary-button" disabled={busy} onClick={() => void openLogin()}>在浏览器中登录</button>
      <button className="ghost-button" disabled={busy} onClick={() => void action("logout")}>取消登录</button>
    </>}
    {view?.status === "signed_in" && <>
      <p>{view.name || view.principal}</p>
      <button className="ghost-button" disabled={busy} onClick={() => void action("refresh")}>检查登录状态</button>
      <button className="ghost-button" disabled={busy} onClick={() => void action("logout")}>退出 NAccount</button>
      <CloudResourcesPanel key={`${baseUrl}:${view.principal}`} baseUrl={baseUrl} />
      <CloudDownloadsPanel key={`downloads:${baseUrl}:${view.principal}`} baseUrl={baseUrl} />
    </>}
    {busy && <p role="status">正在连接…</p>}
    {error && <>
      <p role="alert">{error}</p>
      <button className="ghost-button" disabled={busy} onClick={() => void action("status")}>重试连接</button>
    </>}
  </section>;
}
