import { uiText } from "../../../i18n/index.ts";
import type {
  CapabilityLifecycleStatus,
  CapabilityPluginRecord,
} from "../../../services/loomApi";

export const capabilityStatusLabel: Record<CapabilityLifecycleStatus, string> = {
  get installed_disabled() { return uiText("settings.capabilities.status.installedDisabled"); },
  get approval_required() { return uiText("settings.capabilities.status.approvalRequired"); },
  get activating() { return uiText("settings.capabilities.status.activating"); },
  get active() { return uiText("settings.capabilities.status.active"); },
  get faulted() { return uiText("settings.capabilities.status.faulted"); },
  get disabling() { return uiText("settings.capabilities.status.disabling"); },
  get upgrading() { return uiText("settings.capabilities.status.upgrading"); },
  get rolling_back() { return uiText("settings.capabilities.status.rollingBack"); },
  get uninstalling() { return uiText("settings.capabilities.status.uninstalling"); },
};

export const capabilityStatusTone = (status: CapabilityLifecycleStatus) => {
  if (status === "active") return "success";
  if (status === "faulted" || status === "approval_required") return "warning";
  return "neutral";
};

export const formatCapabilityBytes = (bytes: number) => {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KiB", "MiB", "GiB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value >= 10 || unit === 0 ? value.toFixed(0) : value.toFixed(1)} ${units[unit]}`;
};

export const shortCapabilityDigest = (digest?: string) => digest
  ? `${digest.slice(0, 12)}…${digest.slice(-8)}`
  : uiText("settings.capabilities.none");

export const activeCapabilityVersion = (plugin: CapabilityPluginRecord) =>
  plugin.versions.find((version) => version.digest === plugin.activeDigest) ?? null;

const PERMISSION_MESSAGE_IDS: Readonly<Record<string, string>> = {
  "hook.unit.image.read": "settings.capabilities.permission.unitImageRead",
  "hook.notice.show": "settings.capabilities.permission.noticeShow",
  "hook.unit.attachments.read": "settings.capabilities.permission.unitAttachmentsRead",
  "hook.unit.attachments.write": "settings.capabilities.permission.unitAttachmentsWrite",
  "hook.overlay.render": "settings.capabilities.permission.overlayRender",
  "hook.clipboard.write": "settings.capabilities.permission.clipboardWrite",
  "hook.command.invoke": "settings.capabilities.permission.commandInvoke",
};

export const permissionLabel = (permission: string) => {
  const id = PERMISSION_MESSAGE_IDS[permission];
  return id ? uiText(id) : permission;
};
