import { useEffect, useRef, useState } from "react";
import { postJson } from "../../services/loomApi/transport";
import { cloudError } from "../../services/naccount";

interface Publication { id: string; title: string; kind: string; size: number; sha256: string }
interface Page { items: Publication[]; nextAfter: string | null }
interface Download { publicationId: string; path: string; size: number; sha256: string }

export function CloudDownloadsPanel({ baseUrl }: { baseUrl: string }) {
  const [page, setPage] = useState<Page | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [saved, setSaved] = useState("");
  const generation = useRef(0);
  useEffect(() => () => { generation.current++; }, [baseUrl]);
  async function read(after = "") {
    const token = ++generation.current;
    setBusy(true); setError(""); setSaved("");
    try {
      const result = await postJson<Page>(baseUrl, "/v1/asset-library/library", { after });
      if (!Array.isArray(result.items) || result.items.some((r) => typeof r.id !== "string"
          || typeof r.title !== "string" || typeof r.kind !== "string"
          || !Number.isSafeInteger(r.size) || r.size <= 0 || r.size > 8 * 1024 * 1024
          || typeof r.sha256 !== "string" || !/^[0-9a-f]{64}$/.test(r.sha256))
          || (result.nextAfter !== null && typeof result.nextAfter !== "string"))
        throw new Error("asset_library_response_invalid");
      if (token === generation.current) setPage(result);
    } catch (e) { if (token === generation.current) setError(cloudError(e)); }
    finally { if (token === generation.current) setBusy(false); }
  }
  async function download(publicationId: string) {
    const token = ++generation.current;
    setBusy(true); setError(""); setSaved("");
    try {
      const result = await postJson<Download>(baseUrl, "/v1/asset-library/download", { publicationId });
      if (result.publicationId !== publicationId || typeof result.path !== "string")
        throw new Error("asset_library_response_invalid");
      if (token === generation.current) setSaved(result.path);
    } catch (e) { if (token === generation.current) setError(cloudError(e)); }
    finally { if (token === generation.current) setBusy(false); }
  }
  return <div aria-label="AssetLibrary 下载">
    <h4>下载资源库</h4>
    <button className="ghost-button" disabled={busy} onClick={() => void read()}>读取可下载内容</button>
    {page?.items.map((item) => <div key={item.id}>
      <p>{item.title} · {item.kind} · {item.size} 字节</p>
      <button className="ghost-button" disabled={busy} onClick={() => void download(item.id)}>下载 ZIP</button>
    </div>)}
    {page?.items.length === 0 && <p>暂无可下载内容</p>}
    {page?.nextAfter && <button className="ghost-button" disabled={busy} onClick={() => void read(page.nextAfter!)}>下一页</button>}
    {busy && <p role="status">正在处理…</p>}
    {saved && <p role="status">已校验并保存：{saved}</p>}
    {error && <p role="alert">{error}</p>}
  </div>;
}
