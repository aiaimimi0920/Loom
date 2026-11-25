import { useEffect, useRef, useState } from "react";
import { Button, EmptyState, Notice, Stack } from "@neuro/ui";
import { postJson } from "../../services/loomApi/transport";
import { cloudError } from "../../services/naccount";

interface Resource { id: string; title: string; kind: "art" | "capability" | "application" | "mcp" }
interface Page { items: Resource[]; nextCursor?: string | null }
export function CloudResourcesPanel({ baseUrl }: { baseUrl: string }) {
  const [page, setPage] = useState<Page | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const generation = useRef(0);
  useEffect(() => () => { generation.current++; }, [baseUrl]);
  async function read(after = "") {
    const token = ++generation.current;
    setBusy(true); setError("");
    try {
      const result = await postJson<Page>(baseUrl, "/v1/asset-library/resources", { after });
      if (!Array.isArray(result.items) || result.items.some((r) => typeof r.id !== "string"
          || typeof r.title !== "string" || !["art", "capability", "application", "mcp"].includes(r.kind)))
        throw new Error("asset_library_response_invalid");
      if (generation.current === token) setPage(result);
    } catch (e) { if (generation.current === token) setError(cloudError(e)); }
    finally { if (generation.current === token) setBusy(false); }
  }
  return <div className="settings-cloud-panel" aria-label="AssetLibrary 云端资源">
    <h4>AssetLibrary</h4>
    <Stack horizontal><Button compact disabled={busy} onClick={() => void read()}>读取云端资源</Button></Stack>
    {page?.items.map((resource) => <p key={resource.id}>{resource.title} · {resource.kind}</p>)}
    {page?.items.length === 0 && <EmptyState title="暂无资源" />}
    {page?.nextCursor && <Stack horizontal><Button compact disabled={busy} onClick={() => void read(page.nextCursor!)}>下一页</Button></Stack>}
    {busy && <Notice>正在读取…</Notice>}
    {error && <Notice tone="danger">{error}</Notice>}
  </div>;
}
