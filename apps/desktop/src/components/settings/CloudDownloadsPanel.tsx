import { useEffect, useRef, useState } from "react";
import { postJson } from "../../services/loomApi/transport";
import { cloudError } from "../../services/naccount";
import { canInstallCloudPackage, installCloudPackage, parseCloudDownload } from "../../services/cloudPackageInstall";
import { CloudTransferControls } from "./CloudTransferControls";
import { parseCloudPublicationPage } from "../../services/cloudCatalog";

interface Publication { id: string; title: string; label: string; kind: string; size: number; sha256: string; policy?: string }
interface Page { items: Publication[]; nextAfter: string | null }
interface Download { publicationId: string; path: string; size: number; sha256: string }

export function CloudDownloadsPanel({ baseUrl }: { baseUrl: string }) {
  const [page, setPage] = useState<Page | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [saved, setSaved] = useState("");
  const [installed, setInstalled] = useState("");
  const [taskId, setTaskId] = useState<string | null>(null);
  const generation = useRef(0);
  useEffect(() => () => { generation.current++; }, [baseUrl]);
  async function read(after = "") {
    const token = ++generation.current;
    setBusy(true); setError(""); setSaved(""); setInstalled("");
    try {
      const result = parseCloudPublicationPage(await postJson<Page>(baseUrl, "/v1/asset-library/library", { after }));
      if (token === generation.current) setPage(result);
    } catch (e) { if (token === generation.current) setError(cloudError(e)); }
    finally { if (token === generation.current) setBusy(false); }
  }
  async function download(item: Publication, install = false) {
    const token = ++generation.current;
    const id = crypto.randomUUID();
    setTaskId(id);
    setBusy(true); setError(""); setSaved(""); setInstalled("");
    try {
      const result = install ? await installCloudPackage(baseUrl, item, id)
        : parseCloudDownload(await postJson<Download>(baseUrl, "/v1/asset-library/download", { publicationId: item.id, taskId: id }), item);
      if (token === generation.current) {
        setSaved(result.path);
        if (install) setInstalled(item.kind === "application" ? `应用已安装，未自动运行：${result.installedEntry}` : item.kind === "capability" ? "已安装；请在能力插件中批准权限并启用。" : item.kind === "mcp" ? "MCP 已安装。" : "Art Node 已安装。");
      }
    } catch (e) { if (token === generation.current) setError(cloudError(e)); }
    finally { if (token === generation.current) { setBusy(false); setTaskId(null); } }
  }
  return <div aria-label="AssetLibrary 下载">
    <h4>下载资源库</h4>
    <button className="ghost-button" disabled={busy} onClick={() => void read()}>读取可下载内容</button>
    {page?.items.map((item) => <div key={item.id}>
      <p>{item.title} · {item.label} · {item.kind} · {item.size} 字节</p>
      <button className="ghost-button" disabled={busy} onClick={() => void download(item)}>下载 ZIP</button>
      {canInstallCloudPackage(item) && <button className="ghost-button" disabled={busy}
        onClick={() => void download(item, true)}>下载并安装</button>}
    </div>)}
    {page?.items.length === 0 && <p>暂无可下载内容</p>}
    {page?.nextAfter && <button className="ghost-button" disabled={busy} onClick={() => void read(page.nextAfter!)}>下一页</button>}
    {busy && <p role="status">正在处理…</p>}
    {taskId && <CloudTransferControls key={taskId} baseUrl={baseUrl} taskId={taskId} />}
    {saved && <p role="status">已校验并保存：{saved}</p>}
    {installed && <p role="status">{installed}</p>}
    {error && <p role="alert">{error}</p>}
  </div>;
}
