import { useEffect, useRef, useState } from "react";
import { Button, Notice, Stack } from "@neuro/ui";
import { useUiText } from "../../i18n/index.ts";
import { controlCloudDownload, type CloudTransferStatus } from "../../services/cloudDownloadControl";
import { cloudError } from "../../services/naccount";

export function CloudTransferControls({ baseUrl, taskId }: { baseUrl: string; taskId: string }) {
  const { t, tf } = useUiText();
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
        // Clear stale actionable controls when the task is not registered yet or has entered installation.
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
  return <div className="settings-cloud-panel" aria-label={t("settings.cloud.transfer.panelAria")}>
    {status && <>
      <Notice>{tf("settings.cloud.transfer.progress", { completed: status.completed, total: status.total || "—", speed: status.speed })}
        {status.control === 2 ? t("settings.cloud.transfer.pauseRequested") : status.control === 1 ? t("settings.cloud.transfer.cancelling") : ""}</Notice>
      <Stack horizontal>
        <Button compact disabled={busy || status.control === 1}
          onClick={() => void command(status.control === 2 ? "resume" : "pause")}>
          {status.control === 2 ? t("settings.cloud.transfer.resume") : t("settings.cloud.transfer.pause")}</Button>
        <Button compact disabled={busy || status.control === 1}
          onClick={() => void command("cancel")}>{t("settings.cloud.transfer.cancel")}</Button>
      </Stack>
    </>}
    {error && <Notice tone="danger">{error}</Notice>}
  </div>;
}
