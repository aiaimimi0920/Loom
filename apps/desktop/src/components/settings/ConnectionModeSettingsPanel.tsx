// Desktop/Web server connection mode selector (`local` bundled core vs `remote` LAN Loom server).
import { Button } from "@neuro/ui";
import { useEffect, useState } from "react";
import { useUiText } from "../../i18n/index.ts";
import { pushAppToast } from "../feedback/AppFeedback";
import {
  getLoomConnectionProfile,
  isDesktopTauriShell,
  type LoomConnectionMode,
  type LoomConnectionStatus,
  saveLoomConnectionProfile,
} from "../../services/loomRuntimeTarget";

export function ConnectionModeSettingsPanel({ baseUrl }: { baseUrl: string }) {
  const { t, tf } = useUiText();
  const desktopShell = isDesktopTauriShell();
  const [status, setStatus] = useState<LoomConnectionStatus | null>(null);
  const [mode, setMode] = useState<LoomConnectionMode>("local");
  const [remoteUrl, setRemoteUrl] = useState("");
  const [serverLabel, setServerLabel] = useState("");
  const [remoteToken, setRemoteToken] = useState("");
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    let cancelled = false;
    void getLoomConnectionProfile()
      .then((loaded) => {
        if (cancelled) return;
        setStatus(loaded);
        setMode(loaded.mode);
        setRemoteUrl(loaded.remoteUrl ?? "");
        setServerLabel(loaded.serverLabel ?? "");
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [baseUrl]);

  if (!desktopShell) {
    return (
      <section className="settings-card" aria-label={t("settings.connection.ariaLabel")}>
        <div className="settings-row">
          <div>
            <strong>{t("settings.connection.webTitle")}</strong>
            <p className="settings-muted">{tf("settings.connection.webDescription", { baseUrl })}</p>
          </div>
        </div>
      </section>
    );
  }

  const handleSave = async () => {
    setSaving(true);
    try {
      const updated = await saveLoomConnectionProfile({
        mode,
        remoteUrl: mode === "remote" ? remoteUrl.trim() : (remoteUrl.trim() || null),
        serverLabel: serverLabel.trim() || null,
        remoteToken: remoteToken.trim() || null,
        clearRemoteToken: false,
      });
      setStatus(updated);
      setRemoteToken("");
      pushAppToast({
        level: "info",
        text: updated.mode === "remote"
          ? tf("settings.connection.switchedRemote", { url: updated.activeUrl })
          : t("settings.connection.switchedLocal"),
      });
    } catch (error) {
      pushAppToast({
        level: "error",
        text: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setSaving(false);
    }
  };

  return (
    <section className="settings-card" aria-label={t("settings.connection.ariaLabel")}>
      <div className="settings-row">
        <div>
          <strong>{t("settings.connection.modeTitle")}</strong>
          <p className="settings-muted">
            {t("settings.connection.modeDescriptionPrefix")}
            <code>loom-daemon</code>
            {t("settings.connection.modeDescriptionSuffix")}
          </p>
        </div>
        <select
          aria-label={t("settings.connection.modeSelectAria")}
          value={mode}
          onChange={(event) => setMode(event.target.value as LoomConnectionMode)}
        >
          <option value="local">{t("settings.connection.modeLocal")}</option>
          <option value="remote">{t("settings.connection.modeRemote")}</option>
        </select>
      </div>
      {mode === "remote" ? (
        <div className="settings-stack" style={{ display: "grid", gap: "0.65rem", marginTop: "0.75rem" }}>
          <input
            type="url"
            placeholder={t("settings.connection.remoteUrlPlaceholder")}
            value={remoteUrl}
            onChange={(event) => setRemoteUrl(event.target.value)}
          />
          <input
            type="text"
            placeholder={t("settings.connection.serverLabelPlaceholder")}
            value={serverLabel}
            onChange={(event) => setServerLabel(event.target.value)}
          />
          <input
            type="password"
            placeholder={
              status?.hasRemoteToken
                ? t("settings.connection.remoteTokenSavedPlaceholder")
                : t("settings.connection.remoteTokenPlaceholder")
            }
            value={remoteToken}
            onChange={(event) => setRemoteToken(event.target.value)}
          />
        </div>
      ) : null}
      <div className="settings-row" style={{ marginTop: "0.75rem", justifyContent: "flex-end" }}>
        <Button variant="primary" disabled={saving} onClick={() => void handleSave()}>
          {saving ? t("settings.connection.saving") : t("settings.connection.save")}
        </Button>
      </div>
    </section>
  );
}
