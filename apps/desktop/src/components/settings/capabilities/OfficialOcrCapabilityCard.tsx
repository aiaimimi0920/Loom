// Presents the official OCR capability without creating a second install path.
import type { CapabilityCatalogItem, CapabilityPluginRecord } from "../../../services/loomApi";
import { Badge, Button, Panel } from "@neuro/ui";
import { useUiText } from "../../../i18n/index.ts";
import {
  capabilityStatusLabel,
  capabilityStatusTone,
  formatCapabilityBytes,
} from "./capabilityPresentation";

export const OFFICIAL_OCR_QUALIFIED_ID = "neuro.official/ocr";

interface OfficialOcrCapabilityCardProps {
  installed?: CapabilityPluginRecord;
  catalogItem?: CapabilityCatalogItem;
  catalogConfigured: boolean;
  catalogDiagnostic?: string;
  loading: boolean;
  busy: boolean;
  onInstall: (item: CapabilityCatalogItem) => void;
  onManage: () => void;
}

export function OfficialOcrCapabilityCard({
  installed,
  catalogItem,
  catalogConfigured,
  catalogDiagnostic,
  loading,
  busy,
  onInstall,
  onManage,
}: OfficialOcrCapabilityCardProps) {
  const { t, tf } = useUiText();
  const installedSameVersion = Boolean(catalogItem && installed?.versions.some(
    (version) => version.version === catalogItem.entry.version,
  ));
  const canInstall = Boolean(catalogItem?.compatible && !installedSameVersion);
  const status = installed
    ? capabilityStatusLabel[installed.status]
    : catalogItem ? (catalogItem.compatible ? t("settings.capabilities.ocr.statusInstallable") : t("settings.capabilities.ocr.statusIncompatible"))
      : catalogConfigured ? t("settings.capabilities.ocr.statusNotInCatalog") : t("settings.capabilities.ocr.statusAwaitingCatalog");
  const detail = installed
    ? t("settings.capabilities.ocr.detailInstalled")
    : catalogItem
      ? tf("settings.capabilities.ocr.detailCatalog", {
          downloadSize: formatCapabilityBytes(catalogItem.entry.package.bytes),
          diskSize: formatCapabilityBytes(catalogItem.entry.diskBytes),
        })
      : catalogDiagnostic || t("settings.capabilities.ocr.detailUnconfigured");
  const statusTone = installed
    ? capabilityStatusTone(installed.status)
    : catalogItem?.compatible ? "success" : "warning";

  return (
    <Panel className="capability-ocr-quickstart" aria-labelledby="official-ocr-title">
      <div className="capability-ocr-quickstart__copy">
        <h3 id="official-ocr-title">{t("settings.capabilities.ocr.title")}</h3>
        <p>{detail}</p>
        <div className="capability-ocr-quickstart__signals" aria-label={t("settings.capabilities.ocr.signalsAria")}>
          <Badge>{t("settings.capabilities.ocr.signalLocalInference")}</Badge>
          <Badge>{t("settings.capabilities.ocr.signalOnDemand")}</Badge>
          <Badge>{t("settings.capabilities.ocr.signalSignedPackage")}</Badge>
          <Badge>{t("settings.capabilities.ocr.signalLeastPrivilege")}</Badge>
        </div>
      </div>
      <div className="capability-ocr-quickstart__action">
        <Badge tone={statusTone} role="status" aria-live="polite">
          {loading ? t("settings.capabilities.ocr.loadingStatus") : status}
        </Badge>
        {installed ? (
          <Button compact disabled={busy || loading} onClick={onManage}>
            {t("settings.capabilities.ocr.manageAction")}
          </Button>
        ) : (
          <Button compact variant="primary"
            disabled={busy || loading || !canInstall}
            aria-describedby="official-ocr-install-help"
            onClick={() => catalogItem && onInstall(catalogItem)}
          >
            {busy ? t("settings.capabilities.ocr.verifyingAction") : canInstall ? t("settings.capabilities.ocr.installAction") : t("settings.capabilities.ocr.unavailableAction")}
          </Button>
        )}
        <small id="official-ocr-install-help">
          {catalogItem && !catalogItem.compatible
            ? catalogItem.compatibilityDetail || t("settings.capabilities.ocr.incompatibleFallback")
            : t("settings.capabilities.ocr.catalogOnlyNote")}
        </small>
      </div>
    </Panel>
  );
}
