// Presents the official OCR capability without creating a second install path.
import type { CapabilityCatalogItem, CapabilityPluginRecord } from "../../../services/loomApi";
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
  const installedSameVersion = Boolean(catalogItem && installed?.versions.some(
    (version) => version.version === catalogItem.entry.version,
  ));
  const canInstall = Boolean(catalogItem?.compatible && !installedSameVersion);
  const status = installed
    ? capabilityStatusLabel[installed.status]
    : catalogItem ? (catalogItem.compatible ? "可下载安装" : "当前版本不兼容")
      : catalogConfigured ? "官方目录暂未提供" : "等待官方目录配置";
  const detail = installed
    ? "OCR、二维码和条码识别均由同一个按需能力包提供；停用后 Hook 不再加载模型或解码器。"
    : catalogItem
      ? `下载 ${formatCapabilityBytes(catalogItem.entry.package.bytes)}，安装后约占用 ${formatCapabilityBytes(catalogItem.entry.diskBytes)}。下载包、SBOM 与 Provenance 均在安装前验证。`
      : catalogDiagnostic || "配置经过签名验证的官方能力目录后，即可下载 OCR 模型与本地识别运行时。";
  const statusTone = installed
    ? capabilityStatusTone(installed.status)
    : catalogItem?.compatible ? "success" : "warning";

  return (
    <section className="capability-ocr-quickstart" aria-labelledby="official-ocr-title">
      <div className="capability-ocr-quickstart__copy">
        <span className="capability-eyebrow">Official Capability</span>
        <h3 id="official-ocr-title">OCR · 文本与二维码/条码识别</h3>
        <p>{detail}</p>
        <div className="capability-ocr-quickstart__signals" aria-label="OCR 能力特性">
          <span>本地推理</span><span>按需加载</span><span>签名包</span><span>最小权限</span>
        </div>
      </div>
      <div className="capability-ocr-quickstart__action">
        <span className={`capability-status capability-status--${statusTone}`} role="status" aria-live="polite">
          {loading ? "读取状态…" : status}
        </span>
        {installed ? (
          <button className="ghost-button" type="button" disabled={busy || loading} onClick={onManage}>
            管理 OCR
          </button>
        ) : (
          <button
            className="signal-button"
            type="button"
            disabled={busy || loading || !canInstall}
            aria-describedby="official-ocr-install-help"
            onClick={() => catalogItem && onInstall(catalogItem)}
          >
            {busy ? "下载验证中…" : canInstall ? "下载并安装 OCR" : "当前无法下载"}
          </button>
        )}
        <small id="official-ocr-install-help">
          {catalogItem && !catalogItem.compatible
            ? catalogItem.compatibilityDetail || "当前 Loom/Hook API 与该包不兼容。"
            : "大模型包只通过受信任目录下载，不经过本地 ZIP 的小包上传通道。"}
        </small>
      </div>
    </section>
  );
}
