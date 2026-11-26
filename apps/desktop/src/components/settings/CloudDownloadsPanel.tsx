import { useEffect, useRef, useState } from "react";
import { Button, EmptyState, Notice, Stack } from "@neuro/ui";
import { uiFormat, uiText, useUiText } from "../../i18n/index.ts";
import { postJson } from "../../services/loomApi/transport";
import { cloudError } from "../../services/naccount";
import { canInstallCloudPackage, installCloudPackage, parseCloudDownload } from "../../services/cloudPackageInstall";
import { CloudTransferControls } from "./CloudTransferControls";
import { parseCloudPublicationPage } from "../../services/cloudCatalog";

interface Publication { id: string; title: string; label: string; kind: string; size: number; sha256: string; policy?: string }
interface Page { items: Publication[]; nextAfter: string | null }
interface Download { publicationId: string; path: string; size: number; sha256: string }

export function CloudDownloadsPanel({ baseUrl }: { baseUrl: string }) {
  const { t, tf } = useUiText();
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
    const id = randomId();
    setTaskId(id);
    setBusy(true); setError(""); setSaved(""); setInstalled("");
    try {
      const result = install ? await installCloudPackage(baseUrl, item, id)
        : parseCloudDownload(await postJson<Download>(baseUrl, "/v1/asset-library/download", { publicationId: item.id, taskId: id }), item);
      if (token === generation.current) {
        setSaved(result.path);
        if (install) {
          setInstalled(
            item.kind === "application"
              ? uiFormat("settings.cloud.downloads.installedApplication", { entry: result.installedEntry ?? "" })
              : item.kind === "capability"
                ? uiText("settings.cloud.downloads.installedCapability")
                : item.kind === "mcp"
                  ? uiText("settings.cloud.downloads.installedMcp")
                  : uiText("settings.cloud.downloads.installedArt"),
          );
        }
      }
    } catch (e) { if (token === generation.current) setError(cloudError(e)); }
    finally { if (token === generation.current) { setBusy(false); setTaskId(null); } }
  }
  return <div className="settings-cloud-panel" aria-label={t("settings.cloud.downloads.panelAria")}>
    <h4>{t("settings.cloud.downloads.header")}</h4>
    <Stack horizontal><Button compact disabled={busy} onClick={() => void read()}>{t("settings.cloud.downloads.load")}</Button></Stack>
    {page?.items.map((item) => <div className="settings-cloud-item" key={item.id}>
      <p>{tf("settings.cloud.downloads.itemSummary", { title: item.title, label: item.label, kind: item.kind, size: item.size })}</p>
      <Stack horizontal>
        <Button compact disabled={busy} onClick={() => void download(item)}>{t("settings.cloud.downloads.downloadZip")}</Button>
        {canInstallCloudPackage(item) && <Button compact disabled={busy}
          onClick={() => void download(item, true)}>{t("settings.cloud.downloads.downloadAndInstall")}</Button>}
      </Stack>
    </div>)}
    {page?.items.length === 0 && <EmptyState title={t("settings.cloud.downloads.empty")} />}
    {page?.nextAfter && <Stack horizontal><Button compact disabled={busy} onClick={() => void read(page.nextAfter!)}>{t("settings.cloud.downloads.nextPage")}</Button></Stack>}
    {busy && <Notice>{t("settings.cloud.downloads.processing")}</Notice>}
    {taskId && <CloudTransferControls key={taskId} baseUrl={baseUrl} taskId={taskId} />}
    {saved && <Notice tone="success">{tf("settings.cloud.downloads.saved", { path: saved })}</Notice>}
    {installed && <Notice tone="success">{installed}</Notice>}
    {error && <Notice tone="danger">{error}</Notice>}
  </div>;
}
import { randomId } from "../../services/randomId.ts";
