// Renders the independent Capability Plugin catalog and lifecycle surface.
import { useMemo, useRef, useState } from "react";
import { Badge, Button, EmptyState, Notice, Panel, Tab, TabList, TabPanel, TabsRoot } from "@neuro/ui";
import { useUiText } from "../../../i18n/index.ts";
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
  const { t, tf } = useUiText();
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
        <div><dt>{t("settings.capabilities.versionLabel")}</dt><dd>{shownVersion?.version ?? t("settings.capabilities.unknown")}</dd></div>
        <div><dt>{t("settings.capabilities.trustLabel")}</dt><dd>{shownVersion?.trustStatus ?? t("settings.capabilities.unknown")}</dd></div>
        <div><dt>{t("settings.capabilities.diskLabel")}</dt><dd>{formatCapabilityBytes(diskBytes)}</dd></div>
        <div><dt>{t("settings.capabilities.digestLabel")}</dt><dd title={shownVersion?.digest}>{shortCapabilityDigest(shownVersion?.digest)}</dd></div>
      </dl>
      <div className="capability-permission-summary">
        <strong>{t("settings.capabilities.permissionsHeading")}</strong>
        {plugin.requestedPermissions.length ? (
          <ul>{plugin.requestedPermissions.map((permission) => (
            <li key={permission}>
              <Badge tone={granted.includes(permission) ? "success" : "warning"}>
                {granted.includes(permission) ? t("settings.capabilities.permissionGranted") : t("settings.capabilities.permissionPending")} · {permissionLabel(permission)}
              </Badge>
            </li>
          ))}</ul>
        ) : <span>{t("settings.capabilities.noExtraPermissions")}</span>}
      </div>
      {plugin.runtimeFailures.count > 0 ? (
        <Notice tone="warning">
          {tf("settings.capabilities.runtimeFailuresNotice", { count: plugin.runtimeFailures.count })}
        </Notice>
      ) : null}
      <CapabilitySettingsEditor baseUrl={baseUrl} qualifiedId={plugin.qualifiedId} disabled={busy} />
      <footer className="capability-card__actions">
        {plugin.status === "active" ? (
          <Button compact disabled={busy} onClick={onDisable}>{t("settings.capabilities.disableAction")}</Button>
        ) : (
          <Button compact variant="primary" disabled={busy || !shownVersion} onClick={onEnable}>{t("settings.capabilities.reviewAndEnableAction")}</Button>
        )}
        <Button compact disabled={busy || !plugin.previousDigest} onClick={onRollback}>{t("settings.capabilities.rollbackAction")}</Button>
        <Button compact variant="danger" disabled={busy} onClick={onUninstall}>{t("settings.capabilities.uninstallAction")}</Button>
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
  const { t, tf } = useUiText();
  const installedSameVersion = installed?.versions.some((version) => version.version === item.entry.version);
  return (
    <Panel className="capability-card capability-card--catalog" aria-label={item.entry.name}>
      <header className="capability-card__header">
        <div>
          <span className="capability-publisher">{tf("settings.capabilities.catalogPublisher", { publisher: item.entry.publisher.id })}</span>
          <h3>{item.entry.name}</h3>
          <p>{item.entry.description}</p>
        </div>
        <Badge tone={item.compatible ? "success" : "warning"} role="status">
          {item.compatible ? t("settings.capabilities.compatible") : t("settings.capabilities.incompatible")}
        </Badge>
      </header>
      <dl className="capability-evidence-grid">
        <div><dt>{t("settings.capabilities.versionLabel")}</dt><dd>{item.entry.version}</dd></div>
        <div><dt>{t("settings.capabilities.downloadSizeLabel")}</dt><dd>{formatCapabilityBytes(item.entry.package.bytes)}</dd></div>
        <div><dt>{t("settings.capabilities.installedSizeLabel")}</dt><dd>{formatCapabilityBytes(item.entry.diskBytes)}</dd></div>
        <div><dt>{t("settings.capabilities.signingKeyLabel")}</dt><dd>{item.entry.package.signature.keyId}</dd></div>
      </dl>
      <dl className="capability-host-requirements">
        <div><dt>Loom API</dt><dd>≥ {item.entry.hostCompatibility.loomCapabilityApi.minimum}</dd></div>
        <div><dt>Otter API</dt><dd>≥ {item.entry.hostCompatibility.hookExtensionApi.minimum}</dd></div>
        <div>
          <dt>{t("settings.capabilities.requiredFeaturesLabel")}</dt>
          <dd>{[
            ...item.entry.hostCompatibility.loomCapabilityApi.requiredFeatures,
            ...item.entry.hostCompatibility.hookExtensionApi.requiredFeatures,
            ...(item.entry.hostCompatibility.surfaceApi?.requiredFeatures ?? []),
          ].join(t("settings.capabilities.listSeparator")) || t("settings.capabilities.none")}</dd>
        </div>
      </dl>
      <div className="capability-supply-chain" aria-label={t("settings.capabilities.supplyChainAria")}>
        <Badge>{t("settings.capabilities.ed25519Signed")}</Badge>
        <Badge>{tf("settings.capabilities.sbomBadge", { size: formatCapabilityBytes(item.entry.sbom.bytes) })}</Badge>
        <Badge>{tf("settings.capabilities.provenanceBadge", { size: formatCapabilityBytes(item.entry.provenance.bytes) })}</Badge>
      </div>
      <div className="capability-permission-summary">
        <strong>{t("settings.capabilities.permissionsHeading")}</strong>
        {item.entry.permissions.length
          ? <ul>{item.entry.permissions.map((permission) => <li key={permission}><Badge>{permissionLabel(permission)}</Badge></li>)}</ul>
          : <span>{t("settings.capabilities.noExtraPermissions")}</span>}
      </div>
      {!item.compatible ? <Notice tone="warning">{item.compatibilityDetail}</Notice> : null}
      <footer className="capability-card__actions">
        <Button compact variant="primary"
          disabled={busy || !item.compatible || installedSameVersion}
          onClick={onInstall}
        >
          {installedSameVersion ? t("settings.capabilities.installedSameVersion") : installed ? t("settings.capabilities.verifyAndUpdate") : t("settings.capabilities.verifyAndInstall")}
        </Button>
      </footer>
    </Panel>
  );
}

export function CapabilityExtensionsPanel({ baseUrl }: { baseUrl: string }) {
  const { t } = useUiText();
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
          <h2>{t("settings.capabilities.title")}</h2>
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
            {t("settings.capabilities.installLocalZip")}
          </Button>
          <Button compact loading={controller.loading} onClick={() => void controller.refresh()}>{t("settings.capabilities.refresh")}</Button>
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

      <TabList label={t("settings.capabilities.sourceTabsAria")}>
        <Tab value="installed">
          {t("settings.capabilities.tabInstalled")} <span>{controller.snapshot.plugins.length}</span>
        </Tab>
        <Tab value="catalog">
          {t("settings.capabilities.tabCatalog")} <span>{controller.catalog.packages.length}</span>
        </Tab>
      </TabList>

      {controller.loadError ? (
        <Notice tone="danger">
          <span>{controller.loadError}</span>
          <Button compact onClick={() => void controller.refresh()}>{t("settings.capabilities.retry")}</Button>
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
        </div> : <EmptyState title={t("settings.capabilities.emptyInstalled")}>
          <Button compact variant="primary" onClick={() => setTab("catalog")}>{t("settings.capabilities.openOfficialCatalog")}</Button>
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
        </div> : <EmptyState title={controller.catalog.configured ? t("settings.capabilities.emptyCatalogConfigured") : t("settings.capabilities.emptyCatalogUnconfigured")}>
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
