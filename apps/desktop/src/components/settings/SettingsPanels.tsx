// Presentational settings sections and cache conversion helpers.
import {
  DEFAULT_LOOM_SETTINGS,
  HookCacheSettings,
  LoomArtStoreSettings,
  LoomCacheSettings,
  LoomMcpSettings,
  type LoomPluginTrustPolicy,
  LoomProxySettings,
} from "../../services/loomApi";
import {
  HookCacheSnapshotInfo,
  LoomCacheSnapshotInfo,
  SettingsAppId,
  SettingsSectionIcon,
} from "./settingsModel";

export interface GeneralSettingsValue {
  language: string;
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
  const applicationName = appName === "loom" ? "Loom" : "Hook";
  return (
    <section className="settings-general-panel" aria-label={`${applicationName} 常规设置`}>
      <header className="settings-general-panel__header">
        <SettingsSectionIcon kind="general" />
        <strong>应用设置</strong>
      </header>
      <div className="settings-network-row">
        <div className="settings-network-row__label">
          <strong>语言</strong>
          <span>界面显示语言</span>
        </div>
        <select
          className="studio-input settings-network-row__control"
          aria-label={`${applicationName} 语言`}
          value={value.language}
          onChange={(event) => onChange({ language: event.target.value })}
        >
          <option value="zh-Hans">简体中文</option>
          <option value="en">English</option>
        </select>
      </div>
      <div className="settings-network-row">
        <div className="settings-network-row__label">
          <strong>主题</strong>
          <span>界面明暗模式</span>
        </div>
        <select
          className="studio-input settings-network-row__control"
          aria-label={`${applicationName} 主题`}
          value={value.theme}
          onChange={(event) => onChange({ theme: event.target.value })}
        >
          <option value="system">跟随系统</option>
          <option value="dark">深色</option>
          <option value="light">浅色</option>
        </select>
      </div>
      <div className="settings-network-row">
        <div className="settings-network-row__label">
          <strong>关闭到系统托盘</strong>
          <span>关闭窗口后继续在后台运行</span>
        </div>
        <label className="settings-general-toggle">
          <input
            type="checkbox"
            aria-label={`${applicationName} 关闭到系统托盘`}
            checked={value.closeToTray}
            onChange={(event) => onChange({ closeToTray: event.target.checked })}
          />
          <span>{value.closeToTray ? "已开启" : "已关闭"}</span>
        </label>
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
  return (
    <section className="settings-mcp-panel" aria-label="MCP 设置">
      <header className="settings-mcp-panel__header">
        <SettingsSectionIcon kind="mcp" />
        <strong>运行限制</strong>
      </header>
      <div className="settings-network-row">
        <div className="settings-network-row__label">
          <strong>请求超时</strong>
          <span>单次 MCP 初始化、工具列表或调用的最长等待时间</span>
        </div>
        <select
          className="studio-input settings-network-row__control"
          aria-label="MCP 请求超时"
          value={value.request_timeout_seconds}
          onChange={(event) => onChange({ request_timeout_seconds: Number(event.target.value) })}
        >
          {[15, 30, 60, 120, 300].map((seconds) => (
            <option key={seconds} value={seconds}>{seconds < 60 ? `${seconds} 秒` : `${seconds / 60} 分钟`}</option>
          ))}
        </select>
      </div>
      <div className="settings-network-row">
        <div className="settings-network-row__label">
          <strong>子进程内存上限</strong>
          <span>限制每个由 Loom 启动的 MCP 服务进程</span>
        </div>
        <select
          className="studio-input settings-network-row__control"
          aria-label="MCP 子进程内存上限"
          value={value.memory_limit_bytes}
          onChange={(event) => onChange({ memory_limit_bytes: Number(event.target.value) })}
        >
          {[
            { value: 256 * 1024 * 1024, label: "256 MB" },
            { value: 512 * 1024 * 1024, label: "512 MB" },
            { value: 1024 * 1024 * 1024, label: "1 GB" },
            { value: 2 * 1024 * 1024 * 1024, label: "2 GB" },
          ].map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
        </select>
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
  return (
    <section className="settings-art-store-panel" aria-label="Art 设置">
      <header className="settings-art-store-panel__header">
        <SettingsSectionIcon kind="art-store" />
        <strong>更新与安装</strong>
      </header>
      <div className="settings-network-row">
        <div className="settings-network-row__label">
          <strong>自动更新</strong>
          <span>启动 Art 页面时更新已启用自动更新的 Art</span>
        </div>
        <label className="settings-general-toggle">
          <input type="checkbox" aria-label="Art 自动更新" checked={value.auto_update} onChange={(event) => onChange({ auto_update: event.target.checked })} />
          <span>{value.auto_update ? "已开启" : "已关闭"}</span>
        </label>
      </div>
      <div className="settings-network-row">
        <div className="settings-network-row__label">
          <strong>默认只显示官方</strong>
          <span>进入商店时优先隐藏未经官方认证的 Art</span>
        </div>
        <label className="settings-general-toggle">
          <input type="checkbox" aria-label="Art 默认只显示官方" checked={value.official_only} onChange={(event) => onChange({ official_only: event.target.checked })} />
          <span>{value.official_only ? "已开启" : "已关闭"}</span>
        </label>
      </div>
      <div className="settings-network-row">
        <div className="settings-network-row__label">
          <strong>安装策略</strong>
          <span>决定 Loom 可以安装何种签名状态的 Art</span>
        </div>
        <select
          className="studio-input settings-network-row__control"
          aria-label="Art 安装策略"
          value={trustPolicy}
          disabled={trustPolicyBusy}
          onChange={(event) => onTrustPolicyChange(event.target.value as LoomPluginTrustPolicy)}
        >
          <option value="require_trusted">仅可信发布者</option>
          <option value="require_signed">需要有效签名</option>
          <option value="allow_unsigned">允许未签名</option>
        </select>
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
  return (
    <section className="settings-network-panel" aria-label={`${appName} 网络设置`}>
      <header className="settings-network-panel__header">
        <SettingsSectionIcon kind="network" />
        <strong>代理设置</strong>
      </header>
      <div className="settings-network-row">
        <div className="settings-network-row__label">
          <strong>代理模式</strong>
          <span>选择网络请求使用的代理方式</span>
        </div>
        <select
          className="studio-input settings-network-row__control"
          aria-label={`${appName} 代理模式`}
          value={value.mode}
          onChange={(event) => onChange({ mode: event.target.value as LoomProxySettings["mode"] })}
        >
          <option value="system">跟随系统</option>
          <option value="custom">自定义</option>
          <option value="disabled">不使用代理</option>
        </select>
      </div>
      {value.mode === "custom" ? (
        <div className="settings-network-row">
          <div className="settings-network-row__label">
            <strong>代理地址</strong>
            <span>填写 host:port，例如 127.0.0.1:7890</span>
          </div>
          <div className="settings-network-address">
            <select
              className="studio-input"
              aria-label={`${appName} 代理协议`}
              value={value.protocol}
              onChange={(event) => onChange({ protocol: event.target.value as LoomProxySettings["protocol"] })}
            >
              <option value="http">http://</option>
              <option value="https">https://</option>
              <option value="socks5">socks5://</option>
            </select>
            <input
              className="studio-input"
              aria-label={`${appName} 代理地址`}
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
  const retentionLabel = (days: number) => days === 0 ? "无限" : `${days} 天`;
  return (
    <div className="hook-cache-settings loom-cache-settings" aria-busy={loading || Boolean(busyKind)}>
      <section className="hook-cache-group" aria-labelledby="loom-cache-policy-title">
        <header className="hook-cache-group__header">
          <svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="13" r="7" /><path d="M12 9v4l2.5 1.5M9 3h6M12 3v3" /></svg>
          <strong id="loom-cache-policy-title">缓存规则</strong>
        </header>
        <div className="hook-cache-row">
          <span><strong>Art 运行缓存上限</strong><small>限制已安装 Art 在运行时生成的可重建缓存</small></span>
          <select className="studio-input hook-cache-row__control" aria-label="Art 运行缓存上限" value={settings.art_cache_max_bytes} onChange={(event) => onSettingsChange({ art_cache_max_bytes: Number(event.target.value) })}>
            {[
              { value: 256 * 1024 * 1024, label: "256 MB" },
              { value: 1024 * 1024 * 1024, label: "1 GB" },
              { value: 4 * 1024 * 1024 * 1024, label: "4 GB" },
              { value: 0, label: "无限制" },
            ].map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
          </select>
        </div>
        <div className="hook-cache-row">
          <span><strong>Art 运行缓存自动清理周期</strong><small>按最后修改时间移除可重新生成的缓存文件</small></span>
          <select className="studio-input hook-cache-row__control" aria-label="Art 运行缓存自动清理周期" value={settings.art_cache_retention_days} onChange={(event) => onSettingsChange({ art_cache_retention_days: Number(event.target.value) })}>
            {[3, 7, 30, 0].map((value) => <option key={value} value={value}>{retentionLabel(value)}</option>)}
          </select>
        </div>
        <div className="hook-cache-row">
          <span><strong>框架临时文件自动清理周期</strong><small>清理已经结束的框架执行残留，不影响已安装框架</small></span>
          <select className="studio-input hook-cache-row__control" aria-label="框架临时文件自动清理周期" value={settings.framework_temp_retention_days} onChange={(event) => onSettingsChange({ framework_temp_retention_days: Number(event.target.value) })}>
            {[1, 3, 7, 30, 0].map((value) => <option key={value} value={value}>{retentionLabel(value)}</option>)}
          </select>
        </div>
      </section>

      <section className="hook-cache-group" aria-labelledby="loom-cache-clean-title">
        <header className="hook-cache-group__header">
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="m4 18 8-8 4 4-8 8H4v-4ZM13.5 8.5l2-2 2 2-2 2M15.5 6.5l2-2 2 2-2 2" /></svg>
          <strong id="loom-cache-clean-title">手动清理</strong>
        </header>
        <div className="hook-cache-row hook-cache-row--action" title={snapshot?.artRuntime.path}>
          <span><strong>Art 运行缓存</strong><small>{snapshot ? `${snapshot.artRuntime.fileCount} 个文件` : "正在统计缓存"}</small></span>
          <b>{snapshot ? formatCacheBytes(snapshot.artRuntime.bytes) : "统计中..."}</b>
          <button className="ghost-button" type="button" disabled={Boolean(busyKind)} onClick={() => onClear("artRuntime")}>{busyKind === "artRuntime" ? "清理中..." : "清空"}</button>
        </div>
        <div className="hook-cache-row hook-cache-row--action" title={snapshot?.frameworkTemporary.path}>
          <span><strong>框架临时文件</strong><small>{snapshot ? `${snapshot.frameworkTemporary.fileCount} 个文件` : "正在统计缓存"}</small></span>
          <b>{snapshot ? formatCacheBytes(snapshot.frameworkTemporary.bytes) : "统计中..."}</b>
          <button className="ghost-button" type="button" disabled={Boolean(busyKind)} onClick={() => onClear("frameworkTemporary")}>{busyKind === "frameworkTemporary" ? "清理中..." : "清空"}</button>
        </div>
      </section>
    </div>
  );
}

export function hookCacheSettingsForUi(value?: Partial<HookCacheSettings>): HookCacheSettings {
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

export function hookCachePreferencesForRuntime(settings: HookCacheSettings) {
  return {
    recycleBinMaxEntries: settings.recycle_bin_max_entries,
    recycleBinRetentionDays: settings.recycle_bin_retention_days,
    tempCacheMaxBytes: settings.temp_cache_max_bytes,
    tempCacheRetentionDays: settings.temp_cache_retention_days,
  };
}

export function HookCacheSettingsPanel({
  settings,
  snapshot,
  loading,
  busyKind,
  onSettingsChange,
  onClear,
}: {
  settings: HookCacheSettings;
  snapshot: HookCacheSnapshotInfo | null;
  loading: boolean;
  busyKind: string | null;
  onSettingsChange: (patch: Partial<HookCacheSettings>) => void;
  onClear: (kind: "recycleBin" | "temporary" | "referenceLibrary") => void;
}) {
  const retentionLabel = (days: number) => days === 0 ? "无限" : `${days} 天`;
  return (
    <div className="hook-cache-settings" aria-busy={loading || Boolean(busyKind)}>
      <section className="hook-cache-group" aria-labelledby="hook-cache-policy-title">
        <header className="hook-cache-group__header">
          <svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="13" r="7" /><path d="M12 9v4l2.5 1.5M9 3h6M12 3v3" /></svg>
          <strong id="hook-cache-policy-title">缓存规则</strong>
        </header>
        <div className="hook-cache-row">
          <span><strong>回收站上限</strong><small>超过上限时优先移除最早删除的贴图</small></span>
          <select className="studio-input hook-cache-row__control" aria-label="回收站上限" value={settings.recycle_bin_max_entries} onChange={(event) => onSettingsChange({ recycle_bin_max_entries: Number(event.target.value) })}>
            {[15, 50, 0].map((value) => <option key={value} value={value}>{value === 0 ? "无限" : `${value} 项`}</option>)}
          </select>
        </div>
        <div className="hook-cache-row">
          <span><strong>回收站自动清理周期</strong><small>按删除时间清理过期贴图</small></span>
          <select className="studio-input hook-cache-row__control" aria-label="回收站自动清理周期" value={settings.recycle_bin_retention_days} onChange={(event) => onSettingsChange({ recycle_bin_retention_days: Number(event.target.value) })}>
            {[3, 7, 30, 0].map((value) => <option key={value} value={value}>{retentionLabel(value)}</option>)}
          </select>
        </div>
        <div className="hook-cache-row">
          <span><strong>临时缓存上限</strong><small>用于截图、拖放和图像处理中转文件</small></span>
          <select className="studio-input hook-cache-row__control" aria-label="临时缓存上限" value={settings.temp_cache_max_bytes} onChange={(event) => onSettingsChange({ temp_cache_max_bytes: Number(event.target.value) })}>
            {[
              { value: 128 * 1024 * 1024, label: "128 MB" },
              { value: 256 * 1024 * 1024, label: "256 MB" },
              { value: 1024 * 1024 * 1024, label: "1 GB" },
              { value: 0, label: "无限制" },
            ].map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
          </select>
        </div>
        <div className="hook-cache-row">
          <span><strong>临时缓存自动清理周期</strong><small>移除超过保留时间的临时文件</small></span>
          <select className="studio-input hook-cache-row__control" aria-label="临时缓存自动清理周期" value={settings.temp_cache_retention_days} onChange={(event) => onSettingsChange({ temp_cache_retention_days: Number(event.target.value) })}>
            {[3, 7, 30, 0].map((value) => <option key={value} value={value}>{retentionLabel(value)}</option>)}
          </select>
        </div>
      </section>

      <section className="hook-cache-group" aria-labelledby="hook-cache-clean-title">
        <header className="hook-cache-group__header">
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="m4 18 8-8 4 4-8 8H4v-4ZM13.5 8.5l2-2 2 2-2 2M15.5 6.5l2-2 2 2-2 2" /></svg>
          <strong id="hook-cache-clean-title">手动清理</strong>
        </header>
        <div className="hook-cache-row hook-cache-row--action">
          <span><strong>清空回收站</strong><small>永久移除回收站中的贴图记录</small></span>
          <b>{snapshot ? `${snapshot.recycleBinEntries} 项` : "统计中..."}</b>
          <button className="ghost-button" type="button" disabled={Boolean(busyKind)} onClick={() => onClear("recycleBin")}>{busyKind === "recycleBin" ? "清理中..." : "清空"}</button>
        </div>
        <div className="hook-cache-row hook-cache-row--action" title={snapshot?.temporary.path}>
          <span><strong>清空临时缓存</strong><small>{snapshot ? `${snapshot.temporary.fileCount} 个文件` : "正在统计缓存"}</small></span>
          <b>{snapshot ? formatCacheBytes(snapshot.temporary.bytes) : "统计中..."}</b>
          <button className="ghost-button" type="button" disabled={Boolean(busyKind)} onClick={() => onClear("temporary")}>{busyKind === "temporary" ? "清理中..." : "清空"}</button>
        </div>
        <div className="hook-cache-row hook-cache-row--action">
          <span><strong>清空参考图</strong><small>移除参考列表，不删除桌面上的贴图</small></span>
          <b>{snapshot ? `${snapshot.referenceEntries} 项` : "统计中..."}</b>
          <button className="ghost-button" type="button" disabled={Boolean(busyKind)} onClick={() => onClear("referenceLibrary")}>{busyKind === "referenceLibrary" ? "清理中..." : "清空"}</button>
        </div>
      </section>
    </div>
  );
}
