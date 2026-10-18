import { useEffect, useRef, useState } from "react";
import { controlCloudDownload, type CloudTransferStatus } from "../../services/cloudDownloadControl";
import { cloudError } from "../../services/naccount";

export function CloudTransferControls({ baseUrl, taskId }: { baseUrl: string; taskId: string }) {
  const [status, setStatus] = useState<CloudTransferStatus | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const alive = useRef(true);
  const revision = useRef(0);
  useEffect(() => {
    alive.current = true;
    setStatus(null);
    setBusy(false);
    setError("");
    let stopped = false;
    let timer: ReturnType<typeof setTimeout>;
    async function poll() {
      const expected = revision.current;
      try {
        const next = await controlCloudDownload(baseUrl, taskId, "status");
        if (!stopped && expected === revision.current) setStatus(next);
      } catch {
        // 任务尚未注册或已进入安装阶段时，不继续展示可操作的旧状态。
        if (!stopped && expected === revision.current) setStatus(null);
      }
      if (!stopped) timer = setTimeout(poll, 500);
    }
    void poll();
    return () => { stopped = true; alive.current = false; revision.current++; clearTimeout(timer); };
  }, [baseUrl, taskId]);
  async function command(action: "pause" | "resume" | "cancel") {
    const expected = ++revision.current;
    setBusy(true); setError("");
    try {
      const next = await controlCloudDownload(baseUrl, taskId, action);
      if (alive.current && expected === revision.current) setStatus(next);
    } catch (e) { if (alive.current && expected === revision.current) setError(cloudError(e)); }
    finally { if (alive.current && expected === revision.current) { revision.current++; setBusy(false); } }
  }
  return <div aria-label="下载控制">
    {status && <>
      <p role="status">{status.completed} / {status.total || "—"} 字节 · {status.speed} 字节/秒
        {status.control === 2 ? " · 已请求暂停" : status.control === 1 ? " · 正在取消" : ""}</p>
      <button className="ghost-button" disabled={busy || status.control === 1}
        onClick={() => void command(status.control === 2 ? "resume" : "pause")}>
        {status.control === 2 ? "继续下载" : "暂停下载"}</button>
      <button className="ghost-button" disabled={busy || status.control === 1}
        onClick={() => void command("cancel")}>取消下载</button>
    </>}
    {error && <p role="alert">{error}</p>}
  </div>;
}
