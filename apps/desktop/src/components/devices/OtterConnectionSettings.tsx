import { useEffect, useRef, useState } from "react";
import { Button, Field, Input, Notice, Select, Switch } from "@neuro/ui";
import { useUiText } from "../../i18n/index.ts";
import {
  loadOtterConnections,
  saveOtterConnectionPolicy,
  type OtterConnectionPolicyDraft,
  type OtterConnectionsView,
  type OtterApprovalPolicy,
} from "../../services/loomApi/otterConnections.ts";
import { errorMessage } from "../../services/loomApi/transport.ts";
import "./OtterConnectionSettings.css";
import "./DeviceConnectionLayout.css";
import { LoomLanSettings } from "./LoomLanSettings.tsx";

export function OtterConnectionSettings({ baseUrl, online }: { baseUrl: string; online: boolean }) {
  const { t } = useUiText();
  const [view, setView] = useState<OtterConnectionsView | null>(null);
  const [draft, setDraft] = useState<OtterConnectionPolicyDraft | null>(null);
  const [password, setPassword] = useState("");
  const [passwordEnabled, setPasswordEnabled] = useState(false);
  const initialized = useRef(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const epoch = useRef(0);
  const readVersion = useRef(0);
  const running = useRef(false);

  useEffect(() => {
    const current = ++epoch.current;
    let refreshing = false;
    setView(null);
    setDraft(null);
    setPassword("");
    setError("");
    setNotice("");
    setPasswordEnabled(false);
    setBusy(false);
    initialized.current = false;
    const refresh = async () => {
      if (!online || refreshing || running.current) return;
      refreshing = true;
      const read = ++readVersion.current;
      try {
        const next = await loadOtterConnections(baseUrl);
        if (current !== epoch.current || read !== readVersion.current) return;
        setView(next);
        if (!initialized.current) {
          setPasswordEnabled(next.loom.passwordRequired);
          initialized.current = true;
        }
        setDraft(
          (old) => old ?? { name: next.loom.name, endpoint: null, discoverable: false, approval: next.loom.approval },
        );
      } catch (reason) {
        if (current === epoch.current && read === readVersion.current) setError(errorMessage(reason));
      } finally {
        refreshing = false;
      }
    };
    void refresh();
    const timer = setInterval(() => void refresh(), 2000);
    return () => {
      epoch.current++;
      clearInterval(timer);
    };
  }, [baseUrl, online]);

  async function perform(action: () => Promise<unknown>, saved: boolean) {
    if (running.current || !online) return;
    const current = epoch.current;
    readVersion.current++;
    running.current = true;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      await action();
      if (current !== epoch.current) return;
      if (saved) {
        setPassword("");
        setNotice(t("devices.otterConnection.savedNotice"));
      }
      const next = await loadOtterConnections(baseUrl);
      if (current === epoch.current) setView(next);
    } catch (reason) {
      if (current === epoch.current) setError(errorMessage(reason));
    } finally {
      running.current = false;
      if (current === epoch.current) setBusy(false);
    }
  }
  const change = (patch: Partial<OtterConnectionPolicyDraft>) => {
    if (draft) setDraft({ ...draft, ...patch });
    setNotice("");
  };
  return (
    <section
      className="loom-device-settings otter-connection-settings"
      aria-label={t("devices.otterConnection.ariaLabel")}
      aria-busy={busy || undefined}
    >
      <header>
        <h2>{t("devices.otterConnection.heading")}</h2>
        <code title={view?.loom.loomId}>{view?.loom.loomId ?? "-"}</code>
      </header>
      {!online && <Notice tone="warning">{t("devices.otterConnection.offlineNotice")}</Notice>}
      {error && <Notice tone="danger">{error}</Notice>}
      {notice && <Notice tone="success">{notice}</Notice>}
      <LoomLanSettings baseUrl={baseUrl} online={online} />
      {draft && (
        <form
          onSubmit={(event) => {
            event.preventDefault();
            void perform(
              () =>
                saveOtterConnectionPolicy(baseUrl, {
                  ...draft,
                  ...(!passwordEnabled ? { password: "" } : password ? { password } : {}),
                }),
              true,
            );
          }}
        >
          <fieldset className="loom-device-settings__fields" disabled={!online || busy}>
            <div className="loom-device-settings__wide">
              <Field label={t("devices.common.name")}>
                <Input
                  maxLength={80}
                  value={draft.name}
                  onChange={(event) => change({ name: event.target.value })}
                  required
                />
              </Field>
            </div>
            <div className="loom-device-settings__wide">
              <Field label={t("devices.otterConnection.approvalLabel")}>
                <Select
                  value={draft.approval}
                  onChange={(event) => change({ approval: event.target.value as OtterApprovalPolicy })}
                >
                  <option value="manual">{t("devices.otterConnection.approval.manual")}</option>
                  <option value="lan">{t("devices.otterConnection.approval.lan")}</option>
                  <option value="authenticated">{t("devices.otterConnection.approval.authenticated")}</option>
                </Select>
              </Field>
            </div>
            <div className="otter-connection-password loom-device-settings__wide">
              <Switch
                label={t("devices.otterConnection.codeSwitch")}
                checked={passwordEnabled}
                onChange={(event) => {
                  setPasswordEnabled(event.target.checked);
                  setNotice("");
                }}
              />
              <div className="otter-connection-secret-slot">
                <div hidden={!passwordEnabled} inert={!passwordEnabled}>
                  <Input
                    aria-label={t("devices.otterConnection.codeAriaLabel")}
                    type="password"
                    autoComplete="new-password"
                    minLength={8}
                    maxLength={256}
                    value={password}
                    disabled={!passwordEnabled}
                    required={passwordEnabled && !view?.loom.passwordRequired}
                    placeholder={
                      view?.loom.passwordRequired
                        ? t("devices.otterConnection.codePlaceholderKeep")
                        : t("devices.otterConnection.codePlaceholderMin")
                    }
                    onChange={(event) => setPassword(event.target.value)}
                  />
                </div>
              </div>
            </div>
            <div className="loom-device-settings__actions loom-device-settings__wide">
              <Button type="submit" variant="primary" loading={busy}>
                {t("devices.common.save")}
              </Button>
            </div>
          </fieldset>
        </form>
      )}
    </section>
  );
}
