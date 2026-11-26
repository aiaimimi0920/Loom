import { displayLanguageName, parseDisplayLanguage, uiText } from "../../i18n/index.ts";
import type { LoomManagedDevice } from "../../services/loomApi/otterTypes.ts";
import type { OtterLocaleRequestInput, OtterLocaleRequestStatus, OtterLocaleState } from "../../services/loomApi/otterLocaleTypes.ts";

export interface TrackedLocaleRequest extends OtterLocaleRequestInput {
  deviceId: string;
  deadline: number;
  status: OtterLocaleRequestStatus;
  code?: string;
}

export function localeUnavailableReason(device: LoomManagedDevice | undefined, online: boolean): string | null {
  if (!online) return uiText("settings.otterLanguage.unavailableOffline");
  if (!device) return uiText("settings.otterLanguage.unavailableNoDevice");
  if (device.approval !== "approved" || device.enabled === false) {
    return uiText("settings.otterLanguage.unavailableNotApproved");
  }
  if (!device.locale) return uiText("settings.otterLanguage.unavailableUnsupported");
  if (!device.locale.active) return uiText("settings.otterLanguage.unavailableReceiverOffline");
  if (device.locale.request?.status === "pending") return uiText("settings.otterLanguage.unavailablePending");
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
    case "pending": return uiText("settings.otterLanguage.requestPending");
    case "applied": return uiText("settings.otterLanguage.requestApplied");
    case "conflict": return uiText("settings.otterLanguage.requestConflict");
    case "rejected": return uiText("settings.otterLanguage.requestRejected");
    case "unknown": return uiText("settings.otterLanguage.requestUnknown");
  }
}

export const localeDisplayName = (language: string): string => {
  const parsed = parseDisplayLanguage(language);
  return parsed ? displayLanguageName(parsed) : language;
};
