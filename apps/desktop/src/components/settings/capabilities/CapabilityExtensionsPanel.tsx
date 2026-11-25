// Renders the independent Capability Plugin catalog and lifecycle surface.
import { useMemo, useRef, useState } from "react";
import { Badge, Button, EmptyState, Notice, Panel, Tab, TabList, TabPanel, TabsRoot } from "@neuro/ui";
import type { CapabilityCatalogItem, CapabilityPluginRecord } from "../../../services/loomApi";
import { CapabilityPermissionDialog } from "./CapabilityPermissionDialog";
import { CapabilitySettingsEditor } from "./CapabilitySettingsEditor";
import {
  OFFICIAL_OCR_QUALIFIED_ID,
  OfficialOcrCapabilityCard,
} from "./OfficialOcrCapabilityCard";
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
    <Panel className="capability-card" aria-label={plugin.name}>
      <header className="capability-card__header">
        <div>
          <span className="capability-publisher">{plugin.publisherId}</span>
          <h3>{plugin.name}</h3>
          <p>{plugin.description || plugin.qualifiedId}</p>
        </div>
        <Badge tone={tone} role="status" aria-live="polite">
          {capabilityStatusLabel[plugin.status]}
        </Badge>
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
            <li key={permission}>
              <Badge tone={granted.includes(permission) ? "success" : "warning"}>
                {granted.includes(permission) ? "已授权" : "待授权"} · {permissionLabel(permission)}
              </Badge>
            </li>
          ))}</ul>
        ) : <span>无额外权限</span>}
      </div>
      {plugin.runtimeFailures.count > 0 ? (
        <Notice tone="warning">
          诊断：当前故障窗口记录 {plugin.runtimeFailures.count} 次失败
        </Notice>
      ) : null}
      <CapabilitySettingsEditor baseUrl={baseUrl} qualifiedId={plugin.qualifiedId} disabled={busy} />
      <footer className="capability-card__actions">
        {plugin.status === "active" ? (
          <Button compact disabled={busy} onClick={onDisable}>停用</Button>
        ) : (
          <Button compact variant="primary" disabled={busy || !shownVersion} onClick={onEnable}>权限审查并启用</Button>
        )}
        <Button compact disabled={busy || !plugin.previousDigest} onClick={onRollback}>回滚</Button>
        <Button compact variant="danger" disabled={busy} onClick={onUninstall}>卸载</Button>
      </footer>
    </Panel>
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
    <Panel className="capability-card capability-card--catalog" aria-label={item.entry.name}>
      <header className="capability-card__header">
        <div>
          <span className="capability-publisher">官方目录 · {item.entry.publisher.id}</span>
          <h3>{item.entry.name}</h3>
          <p>{item.entry.description}</p>
        </div>
        <Badge tone={item.compatible ? "success" : "warning"} role="status">
          {item.compatible ? "兼容" : "不兼容"}
        </Badge>
      </header>
      <dl className="capability-evidence-grid">
        <div><dt>版本</dt><dd>{item.entry.version}</dd></div>
        <div><dt>下载</dt><dd>{formatCapabilityBytes(item.entry.package.bytes)}</dd></div>
        <div><dt>安装后</dt><dd>{formatCapabilityBytes(item.entry.diskBytes)}</dd></div>
        <div><dt>签名密钥</dt><dd>{item.entry.package.signature.keyId}</dd></div>
      </dl>
      <dl className="capability-host-requirements">
        <div><dt>Loom API</dt><dd>≥ {item.entry.hostCompatibility.loomCapabilityApi.minimum}</dd></div>
        <div><dt>Otter API</dt><dd>≥ {item.entry.hostCompatibility.hookExtensionApi.minimum}</dd></div>
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
        <Badge>Ed25519 签名</Badge><Badge>SBOM {formatCapabilityBytes(item.entry.sbom.bytes)}</Badge>
        <Badge>Provenance {formatCapabilityBytes(item.entry.provenance.bytes)}</Badge>
      </div>
      <div className="capability-permission-summary">
        <strong>权限</strong>
        {item.entry.permissions.length
          ? <ul>{item.entry.permissions.map((permission) => <li key={permission}><Badge>{permissionLabel(permission)}</Badge></li>)}</ul>
          : <span>无额外权限</span>}
      </div>
      {!item.compatible ? <Notice tone="warning">{item.compatibilityDetail}</Notice> : null}
      <footer className="capability-card__actions">
        <Button compact variant="primary"
          disabled={busy || !item.compatible || installedSameVersion}
          onClick={onInstall}
        >
          {installedSameVersion ? "该版本已安装" : installed ? "验证并安装更新" : "验证并安装"}
        </Button>
      </footer>
    </Panel>
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
  const officialOcr = controller.catalog.packages.find(
    (item) => item.entry.qualifiedId === OFFICIAL_OCR_QUALIFIED_ID,
  );

  return (
    <TabsRoot asChild value={tab} onValueChange={value => setTab(value as CapabilityTab)}>
    <div className="capability-settings">
      <header className="capability-settings__header">
        <div>
          <h2>能力扩展</h2>
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
          <Button compact disabled={Boolean(controller.busyPluginId)} onClick={() => fileInputRef.current?.click()}>
            安装本地 ZIP
          </Button>
          <Button compact loading={controller.loading} onClick={() => void controller.refresh()}>刷新</Button>
        </div>
      </header>

      <OfficialOcrCapabilityCard
        installed={installedById.get(OFFICIAL_OCR_QUALIFIED_ID)}
        catalogItem={officialOcr}
        catalogConfigured={controller.catalog.configured}
        catalogDiagnostic={controller.catalog.diagnostic}
        loading={controller.loading}
        busy={Boolean(controller.busyPluginId)}
        onInstall={(item) => void controller.installFromCatalog(item)}
        onManage={() => setTab("installed")}
      />

      <TabList label="能力扩展来源">
        <Tab value="installed">
          已安装 <span>{controller.snapshot.plugins.length}</span>
        </Tab>
        <Tab value="catalog">
          官方目录 <span>{controller.catalog.packages.length}</span>
        </Tab>
      </TabList>

      {controller.loadError ? (
        <Notice tone="danger">
          <span>{controller.loadError}</span>
          <Button compact onClick={() => void controller.refresh()}>重试</Button>
        </Notice>
      ) : null}

        <TabPanel value="installed">
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
        </div> : <EmptyState title="尚未安装能力扩展">
          <Button compact variant="primary" onClick={() => setTab("catalog")}>打开官方目录</Button>
        </EmptyState>}
        </TabPanel>
        <TabPanel value="catalog">
        {controller.catalog.configured && controller.catalog.packages.length ? <div className="capability-card-grid">
          {controller.catalog.packages.map((item) => <CatalogCapabilityCard
            key={`${item.entry.qualifiedId}:${item.entry.version}`}
            item={item}
            installed={installedById.get(item.entry.qualifiedId)}
            busy={controller.busyPluginId === item.entry.qualifiedId}
            onInstall={() => void controller.installFromCatalog(item)}
          />)}
        </div> : <EmptyState title={controller.catalog.configured ? "官方目录暂无可安装能力" : "官方目录尚未配置"}>
          {controller.catalog.diagnostic ? <Notice tone="warning">{controller.catalog.diagnostic}</Notice> : null}
        </EmptyState>}
        </TabPanel>

      <CapabilityPermissionDialog
        review={controller.permissionReview}
        busy={Boolean(controller.busyPluginId)}
        onCancel={() => controller.setPermissionReview(null)}
        onConfirm={() => void controller.confirmPermissionReview()}
      />
    </div>
    </TabsRoot>
  );
}
