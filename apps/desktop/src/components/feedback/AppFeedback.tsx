// Centralizes queued toast and accessible confirmation portals.
import {
  type LoomCredentialDetails,
  type LoomCredentialSummary,
  type LoomCredentialValueType,
  type LoomPluginTrustStore,
} from "../../services/loomApi";
import {
  type CSSProperties,
  type KeyboardEvent,
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";

export const credentialValueTypeLabels: Record<LoomCredentialValueType, string> = {
  string: "文本",
  number: "数字",
  integer: "整数",
  boolean: "布尔",
  json: "JSON",
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
  leaving?: boolean;
}

export let nextAppToastId = 1;
export const appToastSubscribers = new Set<(entry: AppToastEntry) => void>();

export function pushAppToast(message: { level: AppToastLevel; text: string }) {
  const entry = { ...message, id: nextAppToastId++ };
  appToastSubscribers.forEach((subscriber) => subscriber(entry));
}

export function AppToastViewport() {
  const [entries, setEntries] = useState<AppToastEntry[]>([]);
  const timers = useRef(new Map<number, number>());

  const dismiss = useCallback((id: number) => {
    const existingTimer = timers.current.get(id);
    if (existingTimer) window.clearTimeout(existingTimer);
    setEntries((current) => current.map((entry) => entry.id === id ? { ...entry, leaving: true } : entry));
    const removalTimer = window.setTimeout(() => {
      setEntries((current) => current.filter((entry) => entry.id !== id));
      timers.current.delete(id);
    }, 220);
    timers.current.set(id, removalTimer);
  }, []);

  useEffect(() => {
    const receive = (entry: AppToastEntry) => {
      setEntries((current) => [...current, entry]);
      const lifetime = entry.level === "error" ? 5200 : entry.level === "warning" ? 4200 : 3200;
      timers.current.set(entry.id, window.setTimeout(() => dismiss(entry.id), lifetime));
    };
    appToastSubscribers.add(receive);
    return () => {
      appToastSubscribers.delete(receive);
      timers.current.forEach((timer) => window.clearTimeout(timer));
      timers.current.clear();
    };
  }, [dismiss]);

  if (entries.length === 0) return null;
  return createPortal(
    <div className="app-toast-stack" aria-label="通知">
      {entries.map((entry, index) => (
        <button
          className={`app-toast app-toast--${entry.level}${entry.leaving ? " app-toast--leaving" : ""}`}
          type="button"
          key={entry.id}
          role={entry.level === "error" ? "alert" : "status"}
          aria-live={entry.level === "error" ? "assertive" : "polite"}
          aria-label={`${entry.text}，点击关闭`}
          style={{ "--toast-offset": `${index * 72}px` } as CSSProperties}
          onClick={() => dismiss(entry.id)}
        >
          {entry.text}
        </button>
      ))}
    </div>,
    document.body,
  );
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
      confirmLabel: options.confirmLabel ?? "确认",
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
      confirmLabel: options.confirmLabel ?? "确认",
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
  const dialogRef = useRef<HTMLElement | null>(null);
  const cancelRef = useRef<HTMLButtonElement | null>(null);
  const restoreFocusRef = useRef<HTMLElement | null>(null);
  const bodyOverflowRef = useRef<string | null>(null);
  const active = queue[0] ?? null;
  const [optionSelected, setOptionSelected] = useState(false);
  const optionSelectedRef = useRef(false);

  const settle = useCallback((accepted: boolean) => {
    const [current, ...remaining] = queueRef.current;
    current?.resolve({ accepted, optionSelected: optionSelectedRef.current });
    queueRef.current = remaining;
    setQueue(remaining);
  }, []);

  useEffect(() => {
    appConfirmSubscriber = (request) => {
      setQueue((current) => {
        const next = [...current, request];
        queueRef.current = next;
        return next;
      });
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

  useEffect(() => {
    const selected = active?.optionDefault ?? false;
    optionSelectedRef.current = selected;
    setOptionSelected(selected);
  }, [active?.id, active?.optionDefault]);

  useEffect(() => {
    if (active) {
      if (bodyOverflowRef.current === null) {
        bodyOverflowRef.current = document.body.style.overflow;
        restoreFocusRef.current = document.activeElement instanceof HTMLElement
          ? document.activeElement
          : null;
        document.body.style.overflow = "hidden";
      }
      return;
    }
    if (bodyOverflowRef.current !== null) {
      document.body.style.overflow = bodyOverflowRef.current;
      bodyOverflowRef.current = null;
      restoreFocusRef.current?.focus();
      restoreFocusRef.current = null;
    }
  }, [active]);

  useEffect(() => () => {
    if (bodyOverflowRef.current !== null) {
      document.body.style.overflow = bodyOverflowRef.current;
    }
    restoreFocusRef.current?.focus();
  }, []);

  useEffect(() => {
    if (!active) return;
    const focusTimer = window.setTimeout(() => cancelRef.current?.focus(), 0);
    const handleKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        settle(false);
        return;
      }
      if (event.key !== "Tab") return;
      const focusable = Array.from(dialogRef.current?.querySelectorAll<HTMLElement>(
        'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
      ) ?? []);
      if (focusable.length === 0) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.clearTimeout(focusTimer);
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [active, settle]);

  if (!active) return null;
  return createPortal(
    <div className="app-confirm-backdrop" role="presentation" onMouseDown={(event) => {
      if (event.target === event.currentTarget) settle(false);
    }}>
      <section ref={dialogRef} className={`app-confirm-dialog app-confirm-dialog--${active.tone}`} role="alertdialog" aria-modal="true" aria-labelledby="app-confirm-title" aria-describedby="app-confirm-message">
        <header><h2 id="app-confirm-title">{active.title}</h2></header>
        <p id="app-confirm-message">{active.message}</p>
        {active.optionLabel ? (
          <label className="app-confirm-dialog__option">
            <input type="checkbox" checked={optionSelected} onChange={(event) => {
              optionSelectedRef.current = event.target.checked;
              setOptionSelected(event.target.checked);
            }} />
            <span>{active.optionLabel}</span>
          </label>
        ) : null}
        <footer>
          <button ref={cancelRef} className="ghost-button" type="button" onClick={() => settle(false)}>取消</button>
          <button className={active.tone === "danger" ? "danger-button" : "signal-button"} type="button" onClick={() => settle(true)}>{active.confirmLabel}</button>
        </footer>
      </section>
    </div>,
    document.body,
  );
}
