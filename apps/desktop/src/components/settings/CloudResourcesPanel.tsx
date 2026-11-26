import { useEffect, useRef, useState } from "react";
import { Button, EmptyState, Notice, Stack } from "@neuro/ui";
import { useUiText } from "../../i18n/index.ts";
import { postJson } from "../../services/loomApi/transport";
import { cloudError } from "../../services/naccount";

interface Resource { id: string; title: string; kind: "art" | "capability" | "application" | "mcp" }
interface Page { items: Resource[]; nextCursor?: string | null }
export function CloudResourcesPanel({ baseUrl }: { baseUrl: string }) {
  const { t } = useUiText();
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
  return <div className="settings-cloud-panel" aria-label={t("settings.cloud.resources.panelAria")}>
    <h4>AssetLibrary</h4>
    <Stack horizontal><Button compact disabled={busy} onClick={() => void read()}>{t("settings.cloud.resources.load")}</Button></Stack>
    {page?.items.map((resource) => <p key={resource.id}>{resource.title} · {resource.kind}</p>)}
    {page?.items.length === 0 && <EmptyState title={t("settings.cloud.resources.empty")} />}
    {page?.nextCursor && <Stack horizontal><Button compact disabled={busy} onClick={() => void read(page.nextCursor!)}>{t("settings.cloud.resources.nextPage")}</Button></Stack>}
    {busy && <Notice>{t("settings.cloud.resources.loading")}</Notice>}
    {error && <Notice tone="danger">{error}</Notice>}
  </div>;
}
