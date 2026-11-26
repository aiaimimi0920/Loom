// Desktop/Web server connection mode selector (`local` bundled core vs `remote` LAN Loom server).
import { Button } from "@neuro/ui";
import { useEffect, useState } from "react";
import { pushAppToast } from "../feedback/AppFeedback";
import {
  getLoomConnectionProfile,
  isDesktopTauriShell,
  type LoomConnectionMode,
  type LoomConnectionStatus,
  saveLoomConnectionProfile,
} from "../../services/loomRuntimeTarget";

export function ConnectionModeSettingsPanel({ baseUrl }: { baseUrl: string }) {
  const desktopShell = isDesktopTauriShell();
  const [status, setStatus] = useState<LoomConnectionStatus | null>(null);
  const [mode, setMode] = useState<LoomConnectionMode>("local");
  const [remoteUrl, setRemoteUrl] = useState("");
  const [serverLabel, setServerLabel] = useState("");
  const [remoteToken, setRemoteToken] = useState("");
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    let cancelled = false;
    void getLoomConnectionProfile()
      .then((loaded) => {
        if (cancelled) return;
        setStatus(loaded);
        setMode(loaded.mode);
        setRemoteUrl(loaded.remoteUrl ?? "");
        setServerLabel(loaded.serverLabel ?? "");
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [baseUrl]);

  if (!desktopShell) {
    return (
      <section className="settings-card" aria-label="Loom 服务端连接模式">
        <div className="settings-row">
          <div>
            <strong>当前运行形态：Web 管理控制台</strong>
            <p className="settings-muted">已通过浏览器直连当前 Loom 服务端（{baseUrl}）。</p>
          </div>
        </div>
      </section>
    );
  }

  const handleSave = async () => {
    setSaving(true);
    try {
      const updated = await saveLoomConnectionProfile({
        mode,
        remoteUrl: mode === "remote" ? remoteUrl.trim() : (remoteUrl.trim() || null),
        serverLabel: serverLabel.trim() || null,
        remoteToken: remoteToken.trim() || null,
        clearRemoteToken: false,
      });
      setStatus(updated);
      setRemoteToken("");
      pushAppToast({
        level: "info",
        text: updated.mode === "remote"
          ? `已切换至远程 Loom 服务端：${updated.activeUrl}`
          : "已切换回本机内置 Loom 核心服务",
      });
    } catch (error) {
      pushAppToast({
        level: "error",
        text: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setSaving(false);
    }
  };

  return (
    <section className="settings-card" aria-label="Loom 服务端连接模式">
      <div className="settings-row">
        <div>
          <strong>Loom 核心连接模式</strong>
          <p className="settings-muted">
            可管理本机内置 <code>loom-daemon</code>，或连接家庭局域网 NAS / Linux 主机上的远程 Loom 服务端。
          </p>
        </div>
        <select
          aria-label="核心连接模式"
          value={mode}
          onChange={(event) => setMode(event.target.value as LoomConnectionMode)}
        >
          <option value="local">本机内置核心（默认）</option>
          <option value="remote">连接局域网 / 远程 Loom 服务端</option>
        </select>
      </div>
      {mode === "remote" ? (
        <div className="settings-stack" style={{ display: "grid", gap: "0.65rem", marginTop: "0.75rem" }}>
          <input
            type="url"
            placeholder="例如 http://192.168.1.50:8765 或 https://loom.example.com"
            value={remoteUrl}
            onChange={(event) => setRemoteUrl(event.target.value)}
          />
          <input
            type="text"
            placeholder="服务端备注名称（可选，如 客厅 NAS）"
            value={serverLabel}
            onChange={(event) => setServerLabel(event.target.value)}
          />
          <input
            type="password"
            placeholder={
              status?.hasRemoteToken
                ? "已保存远程令牌（留空保持不变，输入新值覆盖）"
                : "输入远程 Loom daemon-token"
            }
            value={remoteToken}
            onChange={(event) => setRemoteToken(event.target.value)}
          />
        </div>
      ) : null}
      <div className="settings-row" style={{ marginTop: "0.75rem", justifyContent: "flex-end" }}>
        <Button variant="primary" disabled={saving} onClick={() => void handleSave()}>
          {saving ? "正在切换..." : "保存连接模式"}
        </Button>
      </div>
    </section>
  );
}
