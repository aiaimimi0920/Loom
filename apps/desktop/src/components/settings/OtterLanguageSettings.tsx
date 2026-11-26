import { Button, Select } from "@neuro/ui";
import { useUiText } from "../../i18n/index.ts";
import { localeDisplayName, localeRequestMessage } from "./otterLanguageState";
import { useOtterLanguageSettings } from "./useOtterLanguageSettings";
import { SettingsSectionIcon } from "./settingsModel";

export function OtterLanguageSettings({ baseUrl, online, active }: {
  baseUrl: string;
  online: boolean;
  active: boolean;
}) {
  const { t, tf } = useUiText();
  const state = useOtterLanguageSettings(baseUrl, online, active);
  const locale = state.selected?.locale;
  const message = state.tracked ? localeRequestMessage(state.tracked) : state.unavailable;
  return (
    <section className="settings-general-panel" aria-label={t("settings.otterLanguage.title")} aria-busy={state.loading || state.pending}>
      <header className="settings-general-panel__header">
        <SettingsSectionIcon kind="general" />
        <strong>{t("settings.otterLanguage.title")}</strong>
      </header>
      <div className="settings-network-row">
        <div className="settings-network-row__label">
          <strong>{t("settings.otterLanguage.targetDeviceTitle")}</strong>
          <span>{t("settings.otterLanguage.targetDeviceDescription")}</span>
        </div>
        <Select className="settings-network-row__control" aria-label={t("settings.otterLanguage.targetDeviceAria")}
          value={state.selectedId} disabled={!online || state.loading || state.pending}
          onChange={event => state.chooseDevice(event.target.value)}>
          <option value="">{state.loading ? t("settings.otterLanguage.loadingDevices") : t("settings.otterLanguage.selectApprovedDevice")}</option>
          {state.devices.filter(device => device.approval === "approved").map(device =>
            <option key={device.id} value={device.id}>
              {device.name}{device.locale?.active ? "" : t("settings.otterLanguage.receiverDisconnectedSuffix")}
            </option>)}
        </Select>
      </div>
      <div className="settings-network-row">
        <div className="settings-network-row__label">
          <strong>{t("settings.otterLanguage.languageTitle")}</strong>
          <span>{locale ? tf("settings.otterLanguage.reportedLanguage", { language: localeDisplayName(locale.language) }) : t("settings.otterLanguage.readSupportedLanguages")}
            {locale && !locale.active ? t("settings.otterLanguage.offlineSnapshotSuffix") : ""}</span>
        </div>
        <Select className="settings-network-row__control" aria-label={t("settings.otterLanguage.title")}
          aria-describedby="otter-language-status" value={locale?.language ?? ""}
          disabled={!active || Boolean(state.unavailable) || state.loading || state.pending || Boolean(state.loadError)}
          onChange={event => void state.chooseLanguage(event.target.value)}>
          {!locale && <option value="">{t("settings.otterLanguage.notLoadedYet")}</option>}
          {locale?.supportedLanguages.map(language =>
            <option key={language} value={language}>{localeDisplayName(language)}</option>)}
        </Select>
      </div>
      <div className="settings-network-row">
        <div className="settings-network-row__label" id="otter-language-status" role="status" aria-live="polite">
          <span>{message ?? t("settings.otterLanguage.defaultHint")}</span>
          {state.tracked?.status === "pending" && <span>{tf("settings.otterLanguage.pendingTarget", { language: localeDisplayName(state.tracked.language) })}</span>}
          {state.tracked?.code && <span>{state.tracked.code}</span>}
          {(state.loadError || state.submitError) && <span role="alert">{state.loadError ?? state.submitError}</span>}
        </div>
        <Button compact disabled={!online || state.loading} onClick={() => void state.refresh()}>{t("settings.otterLanguage.refreshStatus")}</Button>
      </div>
    </section>
  );
}
