// Owns application publication and FIFO confirmation requests; @neuro/ui owns rendering.
import { Checkbox, ConfirmDialog, ToastViewport, type ToastEntry } from "@neuro/ui";
import { uiText, useUiText } from "../../i18n/index.ts";
import {
  type LoomCredentialDetails,
  type LoomCredentialSummary,
  type LoomCredentialValueType,
  type LoomPluginTrustStore,
} from "../../services/loomApi";
import {
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";

export const credentialValueTypeLabels: Record<LoomCredentialValueType, string> = {
  get string() {
    return uiText("security.credentialType.string");
  },
  get number() {
    return uiText("security.credentialType.number");
  },
  get integer() {
    return uiText("security.credentialType.integer");
  },
  get boolean() {
    return uiText("security.credentialType.boolean");
  },
  get json() {
    return uiText("security.credentialType.json");
  },
};

export const defaultPluginTrustStore: LoomPluginTrustStore = {
  publishers: [],
  policy: "allow_unsigned",
  trustedPublishers: [],
};

export interface CredentialFieldDraft {
  name: string;
  value: string;
  valueType: LoomCredentialValueType;
  original?: LoomCredentialDetails;
}

export function credentialFieldId(credential: Pick<LoomCredentialSummary, "name" | "scope">): string {
  return `${credential.name}:${credential.scope.frameworkId || "*"}:${credential.scope.artId || "*"}`;
}

export type AppToastLevel = "error" | "warning" | "info";

export interface AppToastEntry {
  id: number;
  level: AppToastLevel;
  text: string;
}

export let nextAppToastId = 1;
export const appToastSubscribers = new Set<(entry: AppToastEntry) => void>();

export function pushAppToast(message: { level: AppToastLevel; text: string }) {
  const entry = { ...message, id: nextAppToastId++ };
  appToastSubscribers.forEach((subscriber) => subscriber(entry));
}

export function AppToastViewport() {
  const { t } = useUiText();
  const [entries, setEntries] = useState<AppToastEntry[]>([]);
  const remove = useCallback((id: number) => setEntries(current => current.filter(entry => entry.id !== id)), []);

  useEffect(() => {
    const receive = (entry: AppToastEntry) => {
      setEntries((current) => [...current, entry]);
    };
    appToastSubscribers.add(receive);
    return () => {
      appToastSubscribers.delete(receive);
    };
  }, []);
  const notifications: ToastEntry[] = entries.map(entry => ({
    id: entry.id, message: entry.text, tone: entry.level === "error" ? "danger" : entry.level,
  }));
  return <ToastViewport entries={notifications} onRemove={remove} closeLabel={t("shell.feedback.dismissToast")} />;
}

export type AppConfirmTone = "danger" | "warning";

export interface AppConfirmRequest {
  id: number;
  title: string;
  message: string;
  confirmLabel: string;
  tone: AppConfirmTone;
  optionLabel?: string;
  optionDefault: boolean;
  resolve: (result: AppConfirmResult) => void;
}

export interface AppConfirmResult {
  accepted: boolean;
  optionSelected: boolean;
}

export let nextAppConfirmId = 1;
export let appConfirmSubscriber: ((request: AppConfirmRequest) => void) | null = null;

export function requestAppConfirmation(options: {
  title: string;
  message: string;
  confirmLabel?: string;
  tone?: AppConfirmTone;
}): Promise<boolean> {
  return new Promise((resolve) => {
    if (!appConfirmSubscriber) {
      resolve(false);
      return;
    }
    appConfirmSubscriber({
      id: nextAppConfirmId++,
      title: options.title,
      message: options.message,
      confirmLabel: options.confirmLabel ?? uiText("common.confirm"),
      tone: options.tone ?? "danger",
      optionDefault: false,
      resolve: (result) => resolve(result.accepted),
    });
  });
}

export function requestAppConfirmationWithOption(options: {
  title: string;
  message: string;
  optionLabel: string;
  optionDefault?: boolean;
  confirmLabel?: string;
  tone?: AppConfirmTone;
}): Promise<AppConfirmResult> {
  return new Promise((resolve) => {
    if (!appConfirmSubscriber) {
      resolve({ accepted: false, optionSelected: options.optionDefault ?? false });
      return;
    }
    appConfirmSubscriber({
      id: nextAppConfirmId++,
      title: options.title,
      message: options.message,
      confirmLabel: options.confirmLabel ?? uiText("common.confirm"),
      tone: options.tone ?? "danger",
      optionLabel: options.optionLabel,
      optionDefault: options.optionDefault ?? false,
      resolve,
    });
  });
}

export function AppConfirmViewport() {
  const [queue, setQueue] = useState<AppConfirmRequest[]>([]);
  const queueRef = useRef<AppConfirmRequest[]>([]);
  const restoreFocusRef = useRef<HTMLElement | null>(null);
  const active = queue[0] ?? null;
  const [optionState, setOptionState] = useState({ requestId: 0, selected: false });
  const optionSelected = active?.id === optionState.requestId ? optionState.selected : active?.optionDefault ?? false;

  const settle = useCallback((accepted: boolean, selected: boolean) => {
    const [current, ...remaining] = queueRef.current;
    current?.resolve({ accepted, optionSelected: selected });
    queueRef.current = remaining;
    setQueue(remaining);
  }, []);

  useEffect(() => {
    appConfirmSubscriber = (request) => {
      if (queueRef.current.length === 0) {
        restoreFocusRef.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
      }
      queueRef.current = [...queueRef.current, request];
      setQueue(queueRef.current);
    };
    return () => {
      appConfirmSubscriber = null;
      queueRef.current.forEach((request) => request.resolve({
        accepted: false,
        optionSelected: request.optionDefault,
      }));
      queueRef.current = [];
    };
  }, []);

  if (!active) return null;
  return <ConfirmDialog open title={active.title} description={active.message}
    confirmLabel={active.confirmLabel} danger={active.tone === "danger"} dismissOnOutside
    focusResetKey={active.id} returnFocusRef={restoreFocusRef}
    onOpenChange={open => { if (!open) settle(false, optionSelected); }}
    onConfirm={() => settle(true, optionSelected)}>
    {active.optionLabel ? <Checkbox label={active.optionLabel} checked={optionSelected}
      onChange={event => setOptionState({ requestId: active.id, selected: event.target.checked })} /> : null}
  </ConfirmDialog>;
}
