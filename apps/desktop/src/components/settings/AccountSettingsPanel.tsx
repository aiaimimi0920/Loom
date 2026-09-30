import { useEffect, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { accountError, accountRequest, type AccountView } from "../../services/accountLogin";

export function AccountSettingsPanel({ baseUrl }: { baseUrl: string }) {
  const [view, setView] = useState<AccountView>({ status: "signed_out" });
  const [origin, setOrigin] = useState("");
  const [deviceName, setDeviceName] = useState("我的电脑");
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState("");
  const generation = useRef(0);

  useEffect(() => {
    const token = ++generation.current;
    setBusy(true);
    void accountRequest(baseUrl, "status").then(async (result) => {
      if (generation.current !== token) return;
      setView(result);
      if (result.status === "signed_in") {
        const refreshed = await accountRequest(baseUrl, "refresh");
        if (generation.current === token) setView(refreshed);
      }
    }).catch((failure: unknown) => {
      if (generation.current === token) setError(accountError(failure));
    }).finally(() => { if (generation.current === token) setBusy(false); });
    return () => { generation.current++; };
  }, [baseUrl]);

  async function perform(action: "start" | "poll" | "refresh" | "logout", body: unknown = {}) {
    const token = ++generation.current;
    setBusy(true);
    setError("");
    try {
      const result = await accountRequest(baseUrl, action, body);
      if (generation.current === token) setView(result);
    } catch (failure) {
      if (generation.current === token) {
        setError(accountError(failure));
        if (failure instanceof Error && failure.message.includes("account_signed_out")) setView({ status: "signed_out" });
      }
    } finally {
      if (generation.current === token) setBusy(false);
    }
  }

  useEffect(() => {
    if (busy || view.status !== "pending") return;
    const timer = window.setTimeout(() => { void perform("poll", { requestId: view.requestId }); }, 5_000);
    return () => window.clearTimeout(timer);
  }, [busy, view, baseUrl]);

  async function openAuthorization() {
    if (view.status !== "pending") return;
    try {
      if (isTauri()) await invoke("open_loom_account_login", { baseUrl, requestId: view.requestId });
      else window.open(view.authorizationUrl, "_blank", "noopener,noreferrer");
    } catch { setError("无法打开浏览器，请复制登录地址后打开。"); }
  }

  return (
    <section className="settings-general-panel" aria-label="Loom 账号">
      <header className="settings-general-panel__header"><strong>账号与设备</strong></header>
      {view.status === "signed_out" && <>
        <p>在 Loom 登录后，为本机建立统一的账号与设备身份。</p>
        <label className="settings-network-row"><span>账号服务地址</span><input className="studio-input" aria-label="账号服务地址" value={origin} onChange={(event) => setOrigin(event.target.value)} placeholder="https://你的平台地址" disabled={busy} /></label>
        <label className="settings-network-row"><span>设备名称</span><input className="studio-input" aria-label="设备名称" value={deviceName} maxLength={80} onChange={(event) => setDeviceName(event.target.value)} disabled={busy} /></label>
        <button className="primary-button" disabled={busy || !origin.trim() || !deviceName.trim()} onClick={() => void perform("start", { origin, deviceName })}>登录账号</button>
        {view.remoteRevoked === false && <p role="status">已退出并清除本机凭据；账号服务暂不可达，中心设备记录尚未删除。</p>}
      </>}
      {view.status === "pending" && <>
        <p>请在浏览器完成登录，并核对校验码：<code>{view.fingerprint}</code></p>
        <p>账号服务：{view.origin}</p>
        <button className="primary-button" onClick={() => void openAuthorization()}>在浏览器中继续</button>
        <button className="ghost-button" onClick={() => void navigator.clipboard.writeText(view.authorizationUrl).catch(() => setError("无法复制登录地址。"))}>复制登录地址</button>
        <button className="ghost-button" disabled={busy} onClick={() => void perform("logout")}>取消登录</button>
      </>}
      {view.status === "signed_in" && <>
        <p>已登录：{view.session.username || view.session.accountId}</p>
        <p>设备：{view.session.deviceName}</p>
        <p>账号服务：{view.origin}</p>
        <button className="ghost-button" disabled={busy} onClick={() => void perform("refresh")}>检查登录状态</button>
        <button className="ghost-button" disabled={busy} onClick={() => void perform("logout")}>退出登录</button>
      </>}
      {busy && <p role="status">正在处理账号请求...</p>}
      {error && <p role="alert">{error}</p>}
    </section>
  );
}
