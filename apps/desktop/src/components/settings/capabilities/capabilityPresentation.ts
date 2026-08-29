import type {
  CapabilityLifecycleStatus,
  CapabilityPluginRecord,
} from "../../../services/loomApi";

export const capabilityStatusLabel: Record<CapabilityLifecycleStatus, string> = {
  installed_disabled: "已停用",
  approval_required: "等待权限确认",
  activating: "正在启用",
  active: "运行中",
  faulted: "运行故障",
  disabling: "正在停用",
  upgrading: "正在更新",
  rolling_back: "正在回滚",
  uninstalling: "正在卸载",
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
  : "无";

export const activeCapabilityVersion = (plugin: CapabilityPluginRecord) =>
  plugin.versions.find((version) => version.digest === plugin.activeDigest) ?? null;

export const installedCapabilityVersion = (plugin: CapabilityPluginRecord) =>
  activeCapabilityVersion(plugin) ?? plugin.versions[plugin.versions.length - 1] ?? null;

export const permissionLabel = (permission: string) => ({
  "hook.notice.show": "在贴图内显示通知",
  "hook.unit.attachments.read": "读取贴图能力数据",
  "hook.unit.attachments.write": "写入贴图能力数据",
  "hook.unit.overlays.render": "在贴图内绘制扩展界面",
  "hook.command.invoke": "响应 Hook 扩展命令",
}[permission] ?? permission);

