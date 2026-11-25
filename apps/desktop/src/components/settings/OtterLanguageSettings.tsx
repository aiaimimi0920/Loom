import { Button, Select } from "@neuro/ui";
import { localeDisplayName, localeRequestMessage } from "./otterLanguageState";
import { useOtterLanguageSettings } from "./useOtterLanguageSettings";
import { SettingsSectionIcon } from "./settingsModel";

export function OtterLanguageSettings({ baseUrl, online, active }: {
  baseUrl: string;
  online: boolean;
  active: boolean;
}) {
  const state = useOtterLanguageSettings(baseUrl, online, active);
  const locale = state.selected?.locale;
  const message = state.tracked ? localeRequestMessage(state.tracked) : state.unavailable;
  return (
    <section className="settings-general-panel" aria-label="Otter 显示语言" aria-busy={state.loading || state.pending}>
      <header className="settings-general-panel__header">
        <SettingsSectionIcon kind="general" />
        <strong>Otter 显示语言</strong>
      </header>
      <div className="settings-network-row">
        <div className="settings-network-row__label">
          <strong>目标设备</strong>
          <span>仅修改所选 Otter，不影响 Loom 或其他设备。</span>
        </div>
        <Select className="settings-network-row__control" aria-label="Otter 语言目标设备"
          value={state.selectedId} disabled={!online || state.loading || state.pending}
          onChange={event => state.chooseDevice(event.target.value)}>
          <option value="">{state.loading ? "正在读取设备…" : "请选择已批准的 Otter"}</option>
          {state.devices.filter(device => device.approval === "approved").map(device =>
            <option key={device.id} value={device.id}>{device.name}{device.locale?.active ? "" : "（未连接语言接收端）"}</option>)}
        </Select>
      </div>
      <div className="settings-network-row">
        <div className="settings-network-row__label">
          <strong>语言</strong>
          <span>{locale ? `当前已上报：${localeDisplayName(locale.language)}` : "读取 Otter 实际语言和支持列表"}
            {locale && !locale.active ? "（离线快照）" : ""}</span>
        </div>
        <Select className="settings-network-row__control" aria-label="Otter 显示语言"
          aria-describedby="otter-language-status" value={locale?.language ?? ""}
          disabled={!active || Boolean(state.unavailable) || state.loading || state.pending || Boolean(state.loadError)}
          onChange={event => void state.chooseLanguage(event.target.value)}>
          {!locale && <option value="">尚未读取</option>}
          {locale?.supportedLanguages.map(language =>
            <option key={language} value={language}>{localeDisplayName(language)}</option>)}
        </Select>
      </div>
      <div className="settings-network-row">
        <div className="settings-network-row__label" id="otter-language-status" role="status" aria-live="polite">
          <span>{message ?? "选择后由 Otter 保存并立即应用；托盘修改使用同一设置。"}</span>
          {state.tracked?.status === "pending" && <span>目标语言：{localeDisplayName(state.tracked.language)}</span>}
          {state.tracked?.code && <span>{state.tracked.code}</span>}
          {(state.loadError || state.submitError) && <span role="alert">{state.loadError ?? state.submitError}</span>}
        </div>
        <Button compact disabled={!online || state.loading} onClick={() => void state.refresh()}>刷新状态</Button>
      </div>
    </section>
  );
}
