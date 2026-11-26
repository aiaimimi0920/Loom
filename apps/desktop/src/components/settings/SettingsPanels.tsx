// Presentational settings sections and cache conversion helpers.
import { Button, Input, Select, Switch } from "@neuro/ui";
import { DISPLAY_LANGUAGES, displayLanguageName, parsePersistedDisplayLanguage, useUiText } from "../../i18n/index.ts";
import {
  DEFAULT_LOOM_SETTINGS,
  OtterCacheSettings,
  LoomArtStoreSettings,
  LoomCacheSettings,
  LoomMcpSettings,
  type LoomPluginTrustPolicy,
  LoomProxySettings,
} from "../../services/loomApi";
import {
  OtterCacheSnapshotInfo,
  LoomCacheSnapshotInfo,
  SettingsAppId,
  SettingsSectionIcon,
} from "./settingsModel";

export interface GeneralSettingsValue {
  language?: string;
  theme: string;
  closeToTray: boolean;
}

export function GeneralSettingsPanel({
  appName,
  value,
  onChange,
}: {
  appName: SettingsAppId;
  value: GeneralSettingsValue;
  onChange: (patch: Partial<GeneralSettingsValue>) => void;
}) {
  const { t, tf } = useUiText();
  const applicationName = appName === "loom" ? "Loom" : "Otter";
  return (
    <section className="settings-general-panel" aria-label={tf("settings.general.panelAria", { appName: applicationName })}>
      <header className="settings-general-panel__header">
        <SettingsSectionIcon kind="general" />
        <strong>{t("settings.general.header")}</strong>
      </header>
      {appName === "loom" && <div className="settings-network-row">
        <div className="settings-network-row__label">
          <strong>{t("settings.general.languageTitle")}</strong>
          <span>{t("settings.general.languageDescription")}</span>
        </div>
        <Select
          className="settings-network-row__control"
          aria-label={tf("settings.general.languageAria", { appName: applicationName })}
          value={parsePersistedDisplayLanguage(value.language)}
          onChange={(event) => onChange({ language: parsePersistedDisplayLanguage(event.target.value) })}
        >
          {DISPLAY_LANGUAGES.map((lang) => (
            <option key={lang} value={lang}>{displayLanguageName(lang)}</option>
          ))}
        </Select>
      </div>}
      <div className="settings-network-row">
        <div className="settings-network-row__label">
          <strong>{t("settings.general.themeTitle")}</strong>
          <span>{t("settings.general.themeDescription")}</span>
        </div>
        <Select
          className="settings-network-row__control"
          aria-label={tf("settings.general.themeAria", { appName: applicationName })}
          value={value.theme}
          onChange={(event) => onChange({ theme: event.target.value })}
        >
          <option value="system">{t("settings.general.themeSystem")}</option>
          <option value="dark">{t("settings.general.themeDark")}</option>
          <option value="light">{t("settings.general.themeLight")}</option>
        </Select>
      </div>
      <div className="settings-network-row">
        <div className="settings-network-row__label">
          <strong>{t("settings.general.closeToTrayTitle")}</strong>
          <span>{t("settings.general.closeToTrayDescription")}</span>
        </div>
        <Switch className="settings-general-toggle" aria-label={tf("settings.general.closeToTrayAria", { appName: applicationName })}
          checked={value.closeToTray} onChange={(event) => onChange({ closeToTray: event.target.checked })} />
      </div>
    </section>
  );
}

export function McpSettingsPanel({
  value,
  onChange,
}: {
  value: LoomMcpSettings;
  onChange: (patch: Partial<LoomMcpSettings>) => void;
}) {
  const { t, tf } = useUiText();
  return (
    <section className="settings-mcp-panel" aria-label={t("settings.mcp.panelAria")}>
      <header className="settings-mcp-panel__header">
        <SettingsSectionIcon kind="mcp" />
        <strong>{t("settings.mcp.header")}</strong>
      </header>
      <div className="settings-network-row">
        <div className="settings-network-row__label">
          <strong>{t("settings.mcp.timeoutTitle")}</strong>
          <span>{t("settings.mcp.timeoutDescription")}</span>
        </div>
        <Select
          className="settings-network-row__control"
          aria-label={t("settings.mcp.timeoutAria")}
          value={value.request_timeout_seconds}
          onChange={(event) => onChange({ request_timeout_seconds: Number(event.target.value) })}
        >
          {[15, 30, 60, 120, 300].map((seconds) => (
            <option key={seconds} value={seconds}>
              {seconds < 60 ? tf("settings.mcp.timeoutSeconds", { seconds }) : tf("settings.mcp.timeoutMinutes", { minutes: seconds / 60 })}
            </option>
          ))}
        </Select>
      </div>
      <div className="settings-network-row">
        <div className="settings-network-row__label">
          <strong>{t("settings.mcp.memoryLimitTitle")}</strong>
          <span>{t("settings.mcp.memoryLimitDescription")}</span>
        </div>
        <Select
          className="settings-network-row__control"
          aria-label={t("settings.mcp.memoryLimitAria")}
          value={value.memory_limit_bytes}
          onChange={(event) => onChange({ memory_limit_bytes: Number(event.target.value) })}
        >
          {[
            { value: 256 * 1024 * 1024, label: "256 MB" },
            { value: 512 * 1024 * 1024, label: "512 MB" },
            { value: 1024 * 1024 * 1024, label: "1 GB" },
            { value: 2 * 1024 * 1024 * 1024, label: "2 GB" },
          ].map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
        </Select>
      </div>
    </section>
  );
}

export function ArtStoreSettingsPanel({
  value,
  trustPolicy,
  trustPolicyBusy,
  onChange,
  onTrustPolicyChange,
}: {
  value: LoomArtStoreSettings;
  trustPolicy: LoomPluginTrustPolicy;
  trustPolicyBusy: boolean;
  onChange: (patch: Partial<LoomArtStoreSettings>) => void;
  onTrustPolicyChange: (policy: LoomPluginTrustPolicy) => void;
}) {
  const { t } = useUiText();
  return (
    <section className="settings-art-store-panel" aria-label={t("settings.artStore.panelAria")}>
      <header className="settings-art-store-panel__header">
        <SettingsSectionIcon kind="art-store" />
        <strong>{t("settings.artStore.header")}</strong>
      </header>
      <div className="settings-network-row">
        <div className="settings-network-row__label">
          <strong>{t("settings.artStore.autoUpdateTitle")}</strong>
          <span>{t("settings.artStore.autoUpdateDescription")}</span>
        </div>
        <Switch className="settings-general-toggle" aria-label={t("settings.artStore.autoUpdateAria")} checked={value.auto_update}
          onChange={(event) => onChange({ auto_update: event.target.checked })} />
      </div>
      <div className="settings-network-row">
        <div className="settings-network-row__label">
          <strong>{t("settings.artStore.officialOnlyTitle")}</strong>
          <span>{t("settings.artStore.officialOnlyDescription")}</span>
        </div>
        <Switch className="settings-general-toggle" aria-label={t("settings.artStore.officialOnlyAria")} checked={value.official_only}
          onChange={(event) => onChange({ official_only: event.target.checked })} />
      </div>
      <div className="settings-network-row">
        <div className="settings-network-row__label">
          <strong>{t("settings.artStore.trustPolicyTitle")}</strong>
          <span>{t("settings.artStore.trustPolicyDescription")}</span>
        </div>
        <Select
          className="settings-network-row__control"
          aria-label={t("settings.artStore.trustPolicyAria")}
          value={trustPolicy}
          disabled={trustPolicyBusy}
          onChange={(event) => onTrustPolicyChange(event.target.value as LoomPluginTrustPolicy)}
        >
          <option value="require_trusted">{t("settings.artStore.trustRequireTrusted")}</option>
          <option value="require_signed">{t("settings.artStore.trustRequireSigned")}</option>
          <option value="allow_unsigned">{t("settings.artStore.trustAllowUnsigned")}</option>
        </Select>
      </div>
    </section>
  );
}

export function NetworkSettingsPanel({
  appName,
  value,
  onChange,
}: {
  appName: string;
  value: LoomProxySettings;
  onChange: (patch: Partial<LoomProxySettings>) => void;
}) {
  const { t, tf } = useUiText();
  return (
    <section className="settings-network-panel" aria-label={tf("settings.network.panelAria", { appName })}>
      <header className="settings-network-panel__header">
        <SettingsSectionIcon kind="network" />
        <strong>{t("settings.network.header")}</strong>
      </header>
      <div className="settings-network-row">
        <div className="settings-network-row__label">
          <strong>{t("settings.network.proxyModeTitle")}</strong>
          <span>{t("settings.network.proxyModeDescription")}</span>
        </div>
        <Select
          className="settings-network-row__control"
          aria-label={tf("settings.network.proxyModeAria", { appName })}
          value={value.mode}
          onChange={(event) => onChange({ mode: event.target.value as LoomProxySettings["mode"] })}
        >
          <option value="system">{t("settings.network.proxySystem")}</option>
          <option value="custom">{t("settings.network.proxyCustom")}</option>
          <option value="disabled">{t("settings.network.proxyDisabled")}</option>
        </Select>
      </div>
      {value.mode === "custom" ? (
        <div className="settings-network-row">
          <div className="settings-network-row__label">
            <strong>{t("settings.network.proxyAddressTitle")}</strong>
            <span>{t("settings.network.proxyAddressDescription")}</span>
          </div>
          <div className="settings-network-address">
            <Select
              aria-label={tf("settings.network.proxyProtocolAria", { appName })}
              value={value.protocol}
              onChange={(event) => onChange({ protocol: event.target.value as LoomProxySettings["protocol"] })}
            >
              <option value="http">http://</option>
              <option value="https">https://</option>
              <option value="socks5">socks5://</option>
            </Select>
            <Input
              aria-label={tf("settings.network.proxyAddressAria", { appName })}
              value={value.address}
              placeholder="127.0.0.1:7890"
              spellCheck={false}
              onChange={(event) => onChange({ address: event.target.value })}
            />
          </div>
        </div>
      ) : null}
    </section>
  );
}

export function formatCacheBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  const unitIndex = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / (1024 ** unitIndex);
  return `${value >= 10 || unitIndex === 0 ? value.toFixed(0) : value.toFixed(1)} ${units[unitIndex]}`;
}

export function loomCacheSettingsForUi(value?: Partial<LoomCacheSettings>): LoomCacheSettings {
  const merged = { ...DEFAULT_LOOM_SETTINGS.loom_cache, ...value };
  return {
    art_cache_max_bytes: [0, 256 * 1024 * 1024, 1024 * 1024 * 1024, 4 * 1024 * 1024 * 1024]
      .includes(merged.art_cache_max_bytes)
      ? merged.art_cache_max_bytes
      : 1024 * 1024 * 1024,
    art_cache_retention_days: [0, 3, 7, 30].includes(merged.art_cache_retention_days)
      ? merged.art_cache_retention_days
      : 30,
    framework_temp_retention_days: [0, 1, 3, 7, 30]
      .includes(merged.framework_temp_retention_days)
      ? merged.framework_temp_retention_days
      : 3,
  };
}

export function loomCachePreferencesForRuntime(settings: LoomCacheSettings) {
  return {
    artCacheMaxBytes: settings.art_cache_max_bytes,
    artCacheRetentionDays: settings.art_cache_retention_days,
    frameworkTempRetentionDays: settings.framework_temp_retention_days,
  };
}

export function LoomCacheSettingsPanel({
  settings,
  snapshot,
  loading,
  busyKind,
  onSettingsChange,
  onClear,
}: {
  settings: LoomCacheSettings;
  snapshot: LoomCacheSnapshotInfo | null;
  loading: boolean;
  busyKind: string | null;
  onSettingsChange: (patch: Partial<LoomCacheSettings>) => void;
  onClear: (kind: "artRuntime" | "frameworkTemporary") => void;
}) {
  const { t, tf } = useUiText();
  const retentionLabel = (days: number) => days === 0 ? t("settings.cache.retentionUnlimited") : tf("settings.cache.retentionDays", { days });
  return (
    <div className="otter-cache-settings loom-cache-settings" aria-busy={loading || Boolean(busyKind)}>
      <section className="otter-cache-group" aria-labelledby="loom-cache-policy-title">
        <header className="otter-cache-group__header">
          <svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="13" r="7" /><path d="M12 9v4l2.5 1.5M9 3h6M12 3v3" /></svg>
          <strong id="loom-cache-policy-title">{t("settings.cache.policyHeader")}</strong>
        </header>
        <div className="otter-cache-row">
          <span><strong>{t("settings.cache.artMaxBytesTitle")}</strong><small>{t("settings.cache.artMaxBytesDescription")}</small></span>
          <Select className="otter-cache-row__control" aria-label={t("settings.cache.artMaxBytesTitle")} value={settings.art_cache_max_bytes} onChange={(event) => onSettingsChange({ art_cache_max_bytes: Number(event.target.value) })}>
            {[
              { value: 256 * 1024 * 1024, label: "256 MB" },
              { value: 1024 * 1024 * 1024, label: "1 GB" },
              { value: 4 * 1024 * 1024 * 1024, label: "4 GB" },
              { value: 0, label: t("settings.cache.unlimited") },
            ].map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
          </Select>
        </div>
        <div className="otter-cache-row">
          <span><strong>{t("settings.cache.artRetentionTitle")}</strong><small>{t("settings.cache.artRetentionDescription")}</small></span>
          <Select className="otter-cache-row__control" aria-label={t("settings.cache.artRetentionTitle")} value={settings.art_cache_retention_days} onChange={(event) => onSettingsChange({ art_cache_retention_days: Number(event.target.value) })}>
            {[3, 7, 30, 0].map((value) => <option key={value} value={value}>{retentionLabel(value)}</option>)}
          </Select>
        </div>
        <div className="otter-cache-row">
          <span><strong>{t("settings.cache.frameworkTempRetentionTitle")}</strong><small>{t("settings.cache.frameworkTempRetentionDescription")}</small></span>
          <Select className="otter-cache-row__control" aria-label={t("settings.cache.frameworkTempRetentionTitle")} value={settings.framework_temp_retention_days} onChange={(event) => onSettingsChange({ framework_temp_retention_days: Number(event.target.value) })}>
            {[1, 3, 7, 30, 0].map((value) => <option key={value} value={value}>{retentionLabel(value)}</option>)}
          </Select>
        </div>
      </section>

      <section className="otter-cache-group" aria-labelledby="loom-cache-clean-title">
        <header className="otter-cache-group__header">
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="m4 18 8-8 4 4-8 8H4v-4ZM13.5 8.5l2-2 2 2-2 2M15.5 6.5l2-2 2 2-2 2" /></svg>
          <strong id="loom-cache-clean-title">{t("settings.cache.manualCleanHeader")}</strong>
        </header>
        <div className="otter-cache-row otter-cache-row--action" title={snapshot?.artRuntime.path}>
          <span><strong>{t("settings.cache.artRuntimeLabel")}</strong><small>{snapshot ? tf("settings.cache.fileCount", { count: snapshot.artRuntime.fileCount }) : t("settings.cache.countingDetails")}</small></span>
          <b>{snapshot ? formatCacheBytes(snapshot.artRuntime.bytes) : t("settings.cache.countingShort")}</b>
          <Button compact loading={busyKind === "artRuntime"} disabled={Boolean(busyKind)} onClick={() => onClear("artRuntime")}>{t("settings.cache.clearAction")}</Button>
        </div>
        <div className="otter-cache-row otter-cache-row--action" title={snapshot?.frameworkTemporary.path}>
          <span><strong>{t("settings.cache.frameworkTempLabel")}</strong><small>{snapshot ? tf("settings.cache.fileCount", { count: snapshot.frameworkTemporary.fileCount }) : t("settings.cache.countingDetails")}</small></span>
          <b>{snapshot ? formatCacheBytes(snapshot.frameworkTemporary.bytes) : t("settings.cache.countingShort")}</b>
          <Button compact loading={busyKind === "frameworkTemporary"} disabled={Boolean(busyKind)} onClick={() => onClear("frameworkTemporary")}>{t("settings.cache.clearAction")}</Button>
        </div>
      </section>
    </div>
  );
}

export function otterCacheSettingsForUi(value?: Partial<OtterCacheSettings>): OtterCacheSettings {
  const merged = { ...DEFAULT_LOOM_SETTINGS.hook_cache, ...value };
  return {
    recycle_bin_max_entries: [0, 15, 50].includes(merged.recycle_bin_max_entries)
      ? merged.recycle_bin_max_entries
      : 15,
    recycle_bin_retention_days: [0, 3, 7, 30].includes(merged.recycle_bin_retention_days)
      ? merged.recycle_bin_retention_days
      : 7,
    temp_cache_max_bytes: [0, 128 * 1024 * 1024, 256 * 1024 * 1024, 1024 * 1024 * 1024]
      .includes(merged.temp_cache_max_bytes)
      ? merged.temp_cache_max_bytes
      : 256 * 1024 * 1024,
    temp_cache_retention_days: [0, 3, 7, 30].includes(merged.temp_cache_retention_days)
      ? merged.temp_cache_retention_days
      : 7,
  };
}

export function otterCachePreferencesForRuntime(settings: OtterCacheSettings) {
  return {
    recycleBinMaxEntries: settings.recycle_bin_max_entries,
    recycleBinRetentionDays: settings.recycle_bin_retention_days,
    tempCacheMaxBytes: settings.temp_cache_max_bytes,
    tempCacheRetentionDays: settings.temp_cache_retention_days,
  };
}

export function OtterCacheSettingsPanel({
  settings,
  snapshot,
  loading,
  busyKind,
  onSettingsChange,
  onClear,
}: {
  settings: OtterCacheSettings;
  snapshot: OtterCacheSnapshotInfo | null;
  loading: boolean;
  busyKind: string | null;
  onSettingsChange: (patch: Partial<OtterCacheSettings>) => void;
  onClear: (kind: "recycleBin" | "temporary" | "referenceLibrary") => void;
}) {
  const { t, tf } = useUiText();
  const retentionLabel = (days: number) => days === 0 ? t("settings.cache.retentionUnlimited") : tf("settings.cache.retentionDays", { days });
  return (
    <div className="otter-cache-settings" aria-busy={loading || Boolean(busyKind)}>
      <section className="otter-cache-group" aria-labelledby="otter-cache-policy-title">
        <header className="otter-cache-group__header">
          <svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="13" r="7" /><path d="M12 9v4l2.5 1.5M9 3h6M12 3v3" /></svg>
          <strong id="otter-cache-policy-title">{t("settings.cache.policyHeader")}</strong>
        </header>
        <div className="otter-cache-row">
          <span><strong>{t("settings.cache.recycleBinMaxTitle")}</strong><small>{t("settings.cache.recycleBinMaxDescription")}</small></span>
          <Select className="otter-cache-row__control" aria-label={t("settings.cache.recycleBinMaxTitle")} value={settings.recycle_bin_max_entries} onChange={(event) => onSettingsChange({ recycle_bin_max_entries: Number(event.target.value) })}>
            {[15, 50, 0].map((value) => <option key={value} value={value}>{value === 0 ? t("settings.cache.retentionUnlimited") : tf("settings.cache.entryCount", { count: value })}</option>)}
          </Select>
        </div>
        <div className="otter-cache-row">
          <span><strong>{t("settings.cache.recycleBinRetentionTitle")}</strong><small>{t("settings.cache.recycleBinRetentionDescription")}</small></span>
          <Select className="otter-cache-row__control" aria-label={t("settings.cache.recycleBinRetentionTitle")} value={settings.recycle_bin_retention_days} onChange={(event) => onSettingsChange({ recycle_bin_retention_days: Number(event.target.value) })}>
            {[3, 7, 30, 0].map((value) => <option key={value} value={value}>{retentionLabel(value)}</option>)}
          </Select>
        </div>
        <div className="otter-cache-row">
          <span><strong>{t("settings.cache.tempMaxTitle")}</strong><small>{t("settings.cache.tempMaxDescription")}</small></span>
          <Select className="otter-cache-row__control" aria-label={t("settings.cache.tempMaxTitle")} value={settings.temp_cache_max_bytes} onChange={(event) => onSettingsChange({ temp_cache_max_bytes: Number(event.target.value) })}>
            {[
              { value: 128 * 1024 * 1024, label: "128 MB" },
              { value: 256 * 1024 * 1024, label: "256 MB" },
              { value: 1024 * 1024 * 1024, label: "1 GB" },
              { value: 0, label: t("settings.cache.unlimited") },
            ].map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
          </Select>
        </div>
        <div className="otter-cache-row">
          <span><strong>{t("settings.cache.tempRetentionTitle")}</strong><small>{t("settings.cache.tempRetentionDescription")}</small></span>
          <Select className="otter-cache-row__control" aria-label={t("settings.cache.tempRetentionTitle")} value={settings.temp_cache_retention_days} onChange={(event) => onSettingsChange({ temp_cache_retention_days: Number(event.target.value) })}>
            {[3, 7, 30, 0].map((value) => <option key={value} value={value}>{retentionLabel(value)}</option>)}
          </Select>
        </div>
      </section>

      <section className="otter-cache-group" aria-labelledby="otter-cache-clean-title">
        <header className="otter-cache-group__header">
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="m4 18 8-8 4 4-8 8H4v-4ZM13.5 8.5l2-2 2 2-2 2M15.5 6.5l2-2 2 2-2 2" /></svg>
          <strong id="otter-cache-clean-title">{t("settings.cache.manualCleanHeader")}</strong>
        </header>
        <div className="otter-cache-row otter-cache-row--action">
          <span><strong>{t("settings.cache.clearRecycleBinTitle")}</strong><small>{t("settings.cache.clearRecycleBinDescription")}</small></span>
          <b>{snapshot ? tf("settings.cache.entryCount", { count: snapshot.recycleBinEntries }) : t("settings.cache.countingShort")}</b>
          <Button compact loading={busyKind === "recycleBin"} disabled={Boolean(busyKind)} onClick={() => onClear("recycleBin")}>{t("settings.cache.clearAction")}</Button>
        </div>
        <div className="otter-cache-row otter-cache-row--action" title={snapshot?.temporary.path}>
          <span><strong>{t("settings.cache.clearTempTitle")}</strong><small>{snapshot ? tf("settings.cache.fileCount", { count: snapshot.temporary.fileCount }) : t("settings.cache.countingDetails")}</small></span>
          <b>{snapshot ? formatCacheBytes(snapshot.temporary.bytes) : t("settings.cache.countingShort")}</b>
          <Button compact loading={busyKind === "temporary"} disabled={Boolean(busyKind)} onClick={() => onClear("temporary")}>{t("settings.cache.clearAction")}</Button>
        </div>
        <div className="otter-cache-row otter-cache-row--action">
          <span><strong>{t("settings.cache.clearReferenceTitle")}</strong><small>{t("settings.cache.clearReferenceDescription")}</small></span>
          <b>{snapshot ? tf("settings.cache.entryCount", { count: snapshot.referenceEntries }) : t("settings.cache.countingShort")}</b>
          <Button compact loading={busyKind === "referenceLibrary"} disabled={Boolean(busyKind)} onClick={() => onClear("referenceLibrary")}>{t("settings.cache.clearAction")}</Button>
        </div>
      </section>
    </div>
  );
}
