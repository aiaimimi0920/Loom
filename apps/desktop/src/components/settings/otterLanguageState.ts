import type { LoomManagedDevice } from "../../services/loomApi/otterTypes.ts";
import type { OtterLocaleRequestInput, OtterLocaleRequestStatus, OtterLocaleState } from "../../services/loomApi/otterLocaleTypes.ts";

export interface TrackedLocaleRequest extends OtterLocaleRequestInput {
  deviceId: string;
  deadline: number;
  status: OtterLocaleRequestStatus;
  code?: string;
}

export function localeUnavailableReason(device: LoomManagedDevice | undefined, online: boolean): string | null {
  if (!online) return "Loom 未连接，无法修改 Otter 语言。";
  if (!device) return "请先连接并批准一台 Otter 设备。";
  if (device.approval !== "approved" || device.enabled === false) return "此设备未获批准或已停用。";
  if (!device.locale) return "此设备尚未上报语言设置；请连接支持显示语言设置的新版 Otter。";
  if (!device.locale.active) return "Otter 语言接收端离线，语言请求不会离线排队。";
  if (device.locale.request?.status === "pending") return "此设备正在处理语言请求，请等待回执。";
  return null;
}

// A changed value alone is not proof that this request was applied.
export function observeLocaleRequest(
  tracked: TrackedLocaleRequest,
  locale: OtterLocaleState | null | undefined,
  now: number,
): TrackedLocaleRequest {
  if (tracked.status !== "pending" && tracked.status !== "unknown") return tracked;
  if (locale && locale.receiverGeneration !== tracked.receiverGeneration) {
    return { ...tracked, status: "unknown", code: "locale_receiver_changed" };
  }
  const receipt = locale?.request;
  if (receipt?.requestId === tracked.requestId
    && receipt.receiverGeneration === tracked.receiverGeneration
    && receipt.expectedRevision === tracked.expectedRevision
    && receipt.language === tracked.language) {
    if (receipt.status !== "pending") return { ...tracked, status: receipt.status, code: receipt.code };
    const deadline = Math.min(tracked.deadline, now + receipt.validForMs);
    if (tracked.status === "pending" && now < deadline) return { ...tracked, deadline };
    return tracked.status === "unknown" ? tracked : { ...tracked, deadline, status: "unknown" };
  }
  return now >= tracked.deadline && tracked.status !== "unknown" ? { ...tracked, status: "unknown" } : tracked;
}

export function localeRequestMessage(request: TrackedLocaleRequest): string {
  switch (request.status) {
    case "pending": return "请求已发送，正在等待 Otter 保存并应用。";
    case "applied": return "此请求已由 Otter 确认应用；当前语言以上报值为准。";
    case "conflict": return "设置已被其他入口修改；请核对当前语言后重新选择，未自动覆盖。";
    case "rejected": return "Otter 拒绝了语言请求，设置未应用。";
    case "unknown": return "结果未确认；请求不会自动重试，请核对 Otter 当前语言。";
  }
}

export const localeDisplayName = (language: string) =>
  ({ en: "English", "zh-Hans": "简体中文" }[language] ?? language);
