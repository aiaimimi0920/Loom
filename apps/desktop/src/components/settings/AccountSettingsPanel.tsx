import { useEffect, useRef, useState } from "react";
import { Button, CopyButton, Input, Notice, Stack } from "@neuro/ui";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { useUiText } from "../../i18n/index.ts";
import { accountError, accountRequest, type AccountView } from "../../services/accountLogin";

export function AccountSettingsPanel({ baseUrl }: { baseUrl: string }) {
  const { t, tf } = useUiText();
  const [view, setView] = useState<AccountView>({ status: "signed_out" });
  const [origin, setOrigin] = useState("");
  const [deviceName, setDeviceName] = useState(() => t("settings.cloud.account.defaultDeviceName"));
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState("");
  const generation = useRef(0);

  useEffect(() => {
    const token = ++generation.current;
    setBusy(true);
    void accountRequest(baseUrl, "status").then(async (result) => {
      if (generation.current !== token) return;
      setView(result);
      if (result.status === "signed_in") {
        const refreshed = await accountRequest(baseUrl, "refresh");
        if (generation.current === token) setView(refreshed);
      }
    }).catch((failure: unknown) => {
      if (generation.current === token) setError(accountError(failure));
    }).finally(() => { if (generation.current === token) setBusy(false); });
    return () => { generation.current++; };
  }, [baseUrl]);

  async function perform(action: "start" | "poll" | "refresh" | "logout", body: unknown = {}) {
    const token = ++generation.current;
    setBusy(true);
    setError("");
    try {
      const result = await accountRequest(baseUrl, action, body);
      if (generation.current === token) setView(result);
    } catch (failure) {
      if (generation.current === token) {
        setError(accountError(failure));
        if (failure instanceof Error && failure.message.includes("account_signed_out")) setView({ status: "signed_out" });
      }
    } finally {
      if (generation.current === token) setBusy(false);
    }
  }

  useEffect(() => {
    if (busy || view.status !== "pending") return;
    const timer = window.setTimeout(() => { void perform("poll", { requestId: view.requestId }); }, 5_000);
    return () => window.clearTimeout(timer);
  }, [busy, view, baseUrl]);

  async function openAuthorization() {
    if (view.status !== "pending") return;
    try {
      if (isTauri()) await invoke("open_loom_account_login", { baseUrl, requestId: view.requestId });
      else window.open(view.authorizationUrl, "_blank", "noopener,noreferrer");
    } catch { setError(t("settings.cloud.account.openBrowserFailed")); }
  }

  return (
    <section className="settings-general-panel settings-account-panel" aria-label={t("settings.cloud.account.panelAria")}>
      <header className="settings-general-panel__header"><strong>{t("settings.cloud.account.header")}</strong></header>
      {view.status === "signed_out" && <>
        <label className="settings-network-row"><span>{t("settings.cloud.account.originLabel")}</span><Input aria-label={t("settings.cloud.account.originLabel")} value={origin} onChange={(event) => setOrigin(event.target.value)} placeholder={t("settings.cloud.account.originPlaceholder")} disabled={busy} /></label>
        <label className="settings-network-row"><span>{t("settings.cloud.account.deviceNameLabel")}</span><Input aria-label={t("settings.cloud.account.deviceNameLabel")} value={deviceName} maxLength={80} onChange={(event) => setDeviceName(event.target.value)} disabled={busy} /></label>
        <Stack horizontal><Button compact variant="primary" disabled={busy || !origin.trim() || !deviceName.trim()} onClick={() => void perform("start", { origin, deviceName })}>{t("settings.cloud.account.signIn")}</Button></Stack>
        {view.remoteRevoked === false && <Notice tone="warning">{t("settings.cloud.account.remoteRevokePending")}</Notice>}
      </>}
      {view.status === "pending" && <>
        <p>{t("settings.cloud.account.pendingPrompt")}<code>{view.fingerprint}</code></p>
        <p>{tf("settings.cloud.account.serviceOrigin", { origin: view.origin })}</p>
        <Stack horizontal>
          <Button compact variant="primary" onClick={() => void openAuthorization()}>{t("settings.cloud.account.continueInBrowser")}</Button>
          <CopyButton compact label={t("settings.cloud.account.copyLoginUrl")} errorLabel={t("settings.cloud.account.copyLoginUrlFailed")} onCopy={() => navigator.clipboard.writeText(view.authorizationUrl)}>{t("settings.cloud.account.copyLoginUrl")}</CopyButton>
          <Button compact disabled={busy} onClick={() => void perform("logout")}>{t("settings.cloud.account.cancelSignIn")}</Button>
        </Stack>
      </>}
      {view.status === "signed_in" && <>
        <p>{tf("settings.cloud.account.signedInAs", { user: view.session.username || view.session.accountId })}</p>
        <p>{tf("settings.cloud.account.deviceSummary", { device: view.session.deviceName })}</p>
        <p>{tf("settings.cloud.account.serviceOrigin", { origin: view.origin })}</p>
        <Stack horizontal>
          <Button compact disabled={busy} onClick={() => void perform("refresh")}>{t("settings.cloud.account.checkStatus")}</Button>
          <Button compact disabled={busy} onClick={() => void perform("logout")}>{t("settings.cloud.account.signOut")}</Button>
        </Stack>
      </>}
      {busy && <Notice>{t("settings.cloud.account.processing")}</Notice>}
      {error && <Notice tone="danger">{error}</Notice>}
    </section>
  );
}
