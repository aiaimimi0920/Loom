import { useEffect, useRef, useState } from "react";
import { Button, Notice, Stack } from "@neuro/ui";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { useUiText } from "../../i18n/index.ts";
import { cloudError, naccountRequest, type NAccountView } from "../../services/naccount";
import { CloudResourcesPanel } from "./CloudResourcesPanel";
import { CloudDownloadsPanel } from "./CloudDownloadsPanel";

export function NAccountSettingsPanel({ baseUrl }: { baseUrl: string }) {
  const { t } = useUiText();
  const [view, setView] = useState<NAccountView | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const generation = useRef(0);
  useEffect(() => {
    const token = ++generation.current;
    setBusy(true); setError(""); setView(null);
    void naccountRequest(baseUrl, "status").then(async (next) => {
      if (next.status === "signed_in") next = await naccountRequest(baseUrl, "refresh");
      if (token === generation.current) setView(next);
    }).catch((e: unknown) => { if (token === generation.current) setError(cloudError(e)); })
      .finally(() => { if (token === generation.current) setBusy(false); });
    return () => { generation.current++; };
  }, [baseUrl]);
  async function action(name: "start" | "status" | "refresh" | "logout") {
    const token = ++generation.current;
    setBusy(true); setError("");
    try {
      const next = await naccountRequest(baseUrl, name);
      if (token === generation.current) setView(next);
    } catch (e) { if (token === generation.current) setError(cloudError(e)); }
    finally { if (token === generation.current) setBusy(false); }
  }
  useEffect(() => {
    if (view?.status !== "pending" || busy || error) return;
    const timer = window.setTimeout(() => { void action("status"); }, 2000);
    return () => window.clearTimeout(timer);
  }, [view, busy, error, baseUrl]);
  async function openLogin() {
    if (view?.status !== "pending") return;
    try {
      if (isTauri()) await invoke("open_loom_naccount_login", { baseUrl, requestId: view.requestId });
      else window.open(view.authorizationUrl, "_blank", "noopener,noreferrer");
    } catch { setError(t("settings.cloud.naccount.openLoginFailed")); }
  }
  return <section className="settings-general-panel settings-account-panel" aria-label="NAccount">
    <header className="settings-general-panel__header"><strong>NAccount</strong></header>
    {!view && !busy && <p>{t("settings.cloud.naccount.statusUnread")}</p>}
    {view?.status === "signed_out" && <>
      <p>{view.configured ? t("settings.cloud.naccount.signedOut") : t("settings.cloud.naccount.notConfigured")}</p>
      <Stack horizontal><Button compact variant="primary" disabled={busy || !view.configured} onClick={() => void action("start")}>{t("settings.cloud.naccount.signIn")}</Button></Stack>
      {view.remoteRevoked === false && <Notice tone="warning">{t("settings.cloud.naccount.remoteRevokeIncomplete")}</Notice>}
    </>}
    {view?.status === "pending" && <>
      <Stack horizontal>
        <Button compact variant="primary" disabled={busy} onClick={() => void openLogin()}>{t("settings.cloud.naccount.signInInBrowser")}</Button>
        <Button compact disabled={busy} onClick={() => void action("logout")}>{t("settings.cloud.naccount.cancelSignIn")}</Button>
      </Stack>
    </>}
    {view?.status === "signed_in" && <>
      <p>{view.name || view.principal}</p>
      <Stack horizontal>
        <Button compact disabled={busy} onClick={() => void action("refresh")}>{t("settings.cloud.naccount.checkStatus")}</Button>
        <Button compact disabled={busy} onClick={() => void action("logout")}>{t("settings.cloud.naccount.signOut")}</Button>
      </Stack>
      <CloudResourcesPanel key={`${baseUrl}:${view.principal}`} baseUrl={baseUrl} />
      <CloudDownloadsPanel key={`downloads:${baseUrl}:${view.principal}`} baseUrl={baseUrl} />
    </>}
    {busy && <Notice>{t("settings.cloud.naccount.connecting")}</Notice>}
    {error && <>
      <Notice tone="danger">{error}</Notice>
      <Stack horizontal><Button compact disabled={busy} onClick={() => void action("status")}>{t("settings.cloud.naccount.retryConnection")}</Button></Stack>
    </>}
  </section>;
}
