// Renders the independent Capability Plugin catalog and lifecycle surface.
import { useMemo, useRef, useState } from "react";
import type { CapabilityCatalogItem, CapabilityPluginRecord } from "../../../services/loomApi";
import { CapabilityPermissionDialog } from "./CapabilityPermissionDialog";
import { CapabilitySettingsEditor } from "./CapabilitySettingsEditor";
import {
  activeCapabilityVersion,
  capabilityStatusLabel,
  capabilityStatusTone,
  formatCapabilityBytes,
  permissionLabel,
  shortCapabilityDigest,
} from "./capabilityPresentation";
import { useCapabilityExtensionsController } from "./useCapabilityExtensionsController";

type CapabilityTab = "installed" | "catalog";

function InstalledCapabilityCard({
  plugin,
  diskBytes,
  granted,
  busy,
  onDisable,
  onEnable,
  onRollback,
  onUninstall,
  baseUrl,
}: {
  plugin: CapabilityPluginRecord;
  diskBytes: number;
  granted: string[];
  busy: boolean;
  onDisable: () => void;
  onEnable: () => void;
  onRollback: () => void;
  onUninstall: () => void;
  baseUrl: string;
}) {
  const activeVersion = activeCapabilityVersion(plugin);
  const shownVersion = activeVersion ?? plugin.versions[plugin.versions.length - 1];
  const tone = capabilityStatusTone(plugin.status);
  return (
    <article className="capability-card">
      <header className="capability-card__header">
        <div>
          <span className="capability-eyebrow">{plugin.publisherId}</span>
          <h3>{plugin.name}</h3>
          <p>{plugin.description || plugin.qualifiedId}</p>
        </div>
        <span className={`capability-status capability-status--${tone}`} role="status" aria-live="polite">
          {capabilityStatusLabel[plugin.status]}
        </span>
      </header>
      <dl className="capability-evidence-grid">
        <div><dt>版本</dt><dd>{shownVersion?.version ?? "未知"}</dd></div>
        <div><dt>信任</dt><dd>{shownVersion?.trustStatus ?? "未知"}</dd></div>
        <div><dt>磁盘</dt><dd>{formatCapabilityBytes(diskBytes)}</dd></div>
        <div><dt>摘要</dt><dd title={shownVersion?.digest}>{shortCapabilityDigest(shownVersion?.digest)}</dd></div>
      </dl>
      <div className="capability-permission-summary">
        <strong>权限</strong>
        {plugin.requestedPermissions.length ? (
          <ul>{plugin.requestedPermissions.map((permission) => (
            <li key={permission} className={granted.includes(permission) ? "is-granted" : "is-pending"}>
              {permissionLabel(permission)}
            </li>
          ))}</ul>
        ) : <span>无额外权限</span>}
      </div>
      {plugin.runtimeFailures.count > 0 ? (
        <p className="capability-diagnostic" role="status">
          诊断：当前故障窗口记录 {plugin.runtimeFailures.count} 次失败
        </p>
      ) : null}
      <CapabilitySettingsEditor baseUrl={baseUrl} qualifiedId={plugin.qualifiedId} disabled={busy} />
      <footer className="capability-card__actions">
        {plugin.status === "active" ? (
          <button className="ghost-button" type="button" disabled={busy} onClick={onDisable}>停用</button>
        ) : (
          <button className="signal-button" type="button" disabled={busy || !shownVersion} onClick={onEnable}>权限审查并启用</button>
        )}
        <button className="ghost-button" type="button" disabled={busy || !plugin.previousDigest} onClick={onRollback}>回滚</button>
        <button className="danger-button" type="button" disabled={busy} onClick={onUninstall}>卸载</button>
      </footer>
    </article>
  );
}

function CatalogCapabilityCard({
  item,
  installed,
  busy,
  onInstall,
}: {
  item: CapabilityCatalogItem;
  installed?: CapabilityPluginRecord;
  busy: boolean;
  onInstall: () => void;
}) {
  const installedSameVersion = installed?.versions.some((version) => version.version === item.entry.version);
  return (
    <article className="capability-card capability-card--catalog">
      <header className="capability-card__header">
        <div>
          <span className="capability-eyebrow">官方目录 · {item.entry.publisher.id}</span>
          <h3>{item.entry.name}</h3>
          <p>{item.entry.description}</p>
        </div>
        <span className={`capability-status capability-status--${item.compatible ? "success" : "warning"}`} role="status">
          {item.compatible ? "兼容" : "不兼容"}
        </span>
      </header>
      <dl className="capability-evidence-grid">
        <div><dt>版本</dt><dd>{item.entry.version}</dd></div>
        <div><dt>下载</dt><dd>{formatCapabilityBytes(item.entry.package.bytes)}</dd></div>
        <div><dt>安装后</dt><dd>{formatCapabilityBytes(item.entry.diskBytes)}</dd></div>
        <div><dt>签名密钥</dt><dd>{item.entry.package.signature.keyId}</dd></div>
      </dl>
      <dl className="capability-host-requirements">
        <div><dt>Loom API</dt><dd>≥ {item.entry.hostCompatibility.loomCapabilityApi.minimum}</dd></div>
        <div><dt>Hook API</dt><dd>≥ {item.entry.hostCompatibility.hookExtensionApi.minimum}</dd></div>
        <div>
          <dt>必需特性</dt>
          <dd>{[
            ...item.entry.hostCompatibility.loomCapabilityApi.requiredFeatures,
            ...item.entry.hostCompatibility.hookExtensionApi.requiredFeatures,
            ...(item.entry.hostCompatibility.surfaceApi?.requiredFeatures ?? []),
          ].join("、") || "无"}</dd>
        </div>
      </dl>
      <div className="capability-supply-chain" aria-label="供应链证据">
        <span>Ed25519 签名</span><span>SBOM {formatCapabilityBytes(item.entry.sbom.bytes)}</span>
        <span>Provenance {formatCapabilityBytes(item.entry.provenance.bytes)}</span>
      </div>
      <div className="capability-permission-summary">
        <strong>权限</strong>
        {item.entry.permissions.length
          ? <ul>{item.entry.permissions.map((permission) => <li key={permission}>{permissionLabel(permission)}</li>)}</ul>
          : <span>无额外权限</span>}
      </div>
      {!item.compatible ? <p className="capability-diagnostic">{item.compatibilityDetail}</p> : null}
      <footer className="capability-card__actions">
        <button
          className="signal-button"
          type="button"
          disabled={busy || !item.compatible || installedSameVersion}
          onClick={onInstall}
        >
          {installedSameVersion ? "该版本已安装" : installed ? "验证并安装更新" : "验证并安装"}
        </button>
      </footer>
    </article>
  );
}

export function CapabilityExtensionsPanel({ baseUrl }: { baseUrl: string }) {
  const controller = useCapabilityExtensionsController(baseUrl);
  const [tab, setTab] = useState<CapabilityTab>("installed");
  const fileInputRef = useRef<HTMLInputElement | null>(null);
  const installedById = useMemo(
    () => new Map(controller.snapshot.plugins.map((plugin) => [plugin.qualifiedId, plugin])),
    [controller.snapshot.plugins],
  );
  const grantsByScope = useMemo(() => new Map(controller.grants.map((grant) => [
    `${grant.qualifiedId}:${grant.packageDigest}`,
    grant.permissions,
  ])), [controller.grants]);

  return (
    <div className="capability-settings">
      <header className="capability-settings__header">
        <div>
          <span className="capability-eyebrow">Capability Plugins</span>
          <h2>能力扩展</h2>
          <p>按需安装 OCR 等大体积能力。能力扩展独立于 Art 模块，并通过最小权限接入 Hook。</p>
        </div>
        <div className="capability-settings__header-actions">
          <input
            ref={fileInputRef}
            type="file"
            accept=".zip,application/zip"
            hidden
            onChange={(event) => {
              const file = event.currentTarget.files?.[0];
              event.currentTarget.value = "";
              if (file) void controller.installFromFile(file);
            }}
          />
          <button className="ghost-button" type="button" disabled={Boolean(controller.busyPluginId)} onClick={() => fileInputRef.current?.click()}>
            安装本地 ZIP
          </button>
          <button className="ghost-button" type="button" disabled={controller.loading} onClick={() => void controller.refresh()}>
            {controller.loading ? "刷新中…" : "刷新"}
          </button>
        </div>
      </header>

      <nav className="capability-tabs" role="tablist" aria-label="能力扩展来源">
        <button id="capability-tab-installed" type="button" role="tab" aria-controls="capability-panel-installed" aria-selected={tab === "installed"} className={tab === "installed" ? "is-active" : ""} onClick={() => setTab("installed")}>
          已安装 <span>{controller.snapshot.plugins.length}</span>
        </button>
        <button id="capability-tab-catalog" type="button" role="tab" aria-controls="capability-panel-catalog" aria-selected={tab === "catalog"} className={tab === "catalog" ? "is-active" : ""} onClick={() => setTab("catalog")}>
          官方目录 <span>{controller.catalog.packages.length}</span>
        </button>
      </nav>

      {controller.loadError ? (
        <div className="capability-load-error" role="alert">
          <span>{controller.loadError}</span>
          <button className="ghost-button" type="button" onClick={() => void controller.refresh()}>重试</button>
        </div>
      ) : null}

      {tab === "installed" ? (
        <section id="capability-panel-installed" role="tabpanel" aria-labelledby="capability-tab-installed">
        {controller.snapshot.plugins.length ? <div className="capability-card-grid">
          {controller.snapshot.plugins.map((plugin) => {
            const activeDigest = plugin.activeDigest ?? plugin.versions[plugin.versions.length - 1]?.digest ?? "";
            return <InstalledCapabilityCard
              key={plugin.qualifiedId}
              plugin={plugin}
              diskBytes={controller.snapshot.diskBytesByPlugin[plugin.qualifiedId] ?? 0}
              granted={grantsByScope.get(`${plugin.qualifiedId}:${activeDigest}`) ?? []}
              busy={controller.busyPluginId === plugin.qualifiedId}
              onDisable={() => void controller.disable(plugin)}
              onEnable={() => controller.requestEnable(plugin)}
              onRollback={() => controller.requestRollback(plugin)}
              onUninstall={() => void controller.uninstall(plugin)}
              baseUrl={baseUrl}
            />;
          })}
        </div> : <div className="capability-empty-state">
          <strong>尚未安装能力扩展</strong>
          <p>从官方目录选择经过签名验证的能力，或安装本地开发 ZIP。</p>
          <button className="signal-button" type="button" onClick={() => setTab("catalog")}>打开官方目录</button>
        </div>}
        </section>
      ) : (
        <section id="capability-panel-catalog" role="tabpanel" aria-labelledby="capability-tab-catalog">
        {controller.catalog.configured ? <div className="capability-card-grid">
          {controller.catalog.packages.map((item) => <CatalogCapabilityCard
            key={`${item.entry.qualifiedId}:${item.entry.version}`}
            item={item}
            installed={installedById.get(item.entry.qualifiedId)}
            busy={controller.busyPluginId === item.entry.qualifiedId}
            onInstall={() => void controller.installFromCatalog(item)}
          />)}
        </div> : <div className="capability-empty-state">
          <strong>官方目录尚未配置</strong>
          <p>{controller.catalog.diagnostic || "设置目录地址和官方签名信任后即可浏览。"}</p>
        </div>}
        </section>
      )}

      <CapabilityPermissionDialog
        review={controller.permissionReview}
        busy={Boolean(controller.busyPluginId)}
        onCancel={() => controller.setPermissionReview(null)}
        onConfirm={() => void controller.confirmPermissionReview()}
      />
    </div>
  );
}
