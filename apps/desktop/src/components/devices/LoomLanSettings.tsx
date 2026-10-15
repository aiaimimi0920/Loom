import { useEffect, useRef, useState } from "react";
import { getJson, putJson } from "../../services/loomApi/transport.ts";
import { errorMessage } from "../../services/loomApi/transport.ts";

interface LanView {
  enabled: boolean; listening: boolean; host: string; port: number;
  endpoint: string | null; error: string | null;
}

/** LAN listener is separate from local management and requires explicit user opt-in. */
export function LoomLanSettings({ baseUrl, online }: { baseUrl: string; online: boolean }) {
  const [view, setView] = useState<LanView | null>(null);
  const [enabled, setEnabled] = useState(false);
  const [host, setHost] = useState("");
  const [port, setPort] = useState("8766");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const epoch = useRef(0);
  useEffect(() => {
    epoch.current += 1;
    let active = true;
    setView(null); setError(""); setBusy(false);
    if (online) void getJson<LanView>(baseUrl, "/v1/otter/connections/lan").then((next) => {
      if (!active) return;
      setView(next); setEnabled(next.enabled); setHost(next.host === "127.0.0.1" ? "" : next.host); setPort(String(next.port));
    }).catch((reason) => { if (active) setError(errorMessage(reason)); });
    return () => { active = false; epoch.current += 1; };
  }, [baseUrl, online]);

  async function save() {
    if (busy || !online) return;
    const current = epoch.current;
    setBusy(true); setError("");
    try {
      if (!/^\d{4,5}$/.test(port) || Number(port) < 1024 || Number(port) > 65535) throw new Error("端口须为 1024–65535");
      if (enabled && !isPrivateIpv4(host.trim())) throw new Error("请填写本机局域网 IPv4 地址，不能使用 127.0.0.1 或代理虚拟地址");
      const next = await putJson<LanView>(baseUrl, "/v1/otter/connections/lan", { enabled, host: host.trim(), port: Number(port) });
      if (current === epoch.current) setView(next);
    } catch (reason) { if (current === epoch.current) setError(errorMessage(reason)); }
    finally { if (current === epoch.current) setBusy(false); }
  }
  return <form onSubmit={(event) => { event.preventDefault(); void save(); }} aria-label="Loom 局域网连接">
    <fieldset disabled={!online || busy || !view}>
      <label>允许局域网连接<input type="checkbox" checked={enabled} onChange={(event) => setEnabled(event.target.checked)} /></label>
      <label>本机局域网 IP<input value={host} maxLength={15} onChange={(event) => setHost(event.target.value)} required /></label>
      <label>端口<input value={port} inputMode="numeric" maxLength={5} onChange={(event) => setPort(event.target.value)} required /></label>
      <button type="submit" className="signal-button">{busy ? "处理中…" : "应用"}</button>
    </fieldset>
    <p role="status">{view?.listening ? `已监听 ${view.endpoint}` : "局域网连接未开启"}</p>
    {(error || view?.error) && <p className="wall-error" role="alert">{error || view?.error}</p>}
  </form>;
}

/** Do not offer loopback or TUN benchmarking addresses as a usable LAN endpoint. */
export function isPrivateIpv4(host: string): boolean {
  const parts = host.split(".");
  if (parts.length !== 4 || parts.some((part) => !/^\d{1,3}$/.test(part) || Number(part) > 255)) return false;
  const [a, b] = parts.map(Number);
  return a === 10 || (a === 172 && b >= 16 && b <= 31) || (a === 192 && b === 168);
}
