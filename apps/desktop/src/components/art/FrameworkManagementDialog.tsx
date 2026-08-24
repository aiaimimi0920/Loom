// Owns framework package installation, removal confirmation, and upgrades.
import { frameworkFilterLabel, frameworkIdentity } from "../../services/artHubUi";
import type { LoomFramework } from "../../services/loomApi";
import type { StudioMessage } from "../app/appShell";
import { useEffect, useRef, useState } from "react";

export type FrameworkBusyAction = "toggle" | "upgrade" | null;

export function readFrameworkPackageBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(reader.error ?? new Error("无法读取框架更新包。"));
    reader.onload = () => {
      const result = typeof reader.result === "string" ? reader.result : "";
      const separator = result.indexOf(",");
      if (separator < 0 || !result.slice(separator + 1)) {
        reject(new Error("框架更新包内容为空。"));
        return;
      }
      resolve(result.slice(separator + 1));
    };
    reader.readAsDataURL(file);
  });
}

export function FrameworkManagementDialog({
  open,
  frameworks,
  busyId,
  busyAction,
  error,
  message,
  onClose,
  onToggle,
  onUpgrade,
}: {
  open: boolean;
  frameworks: LoomFramework[];
  busyId: string | null;
  busyAction: FrameworkBusyAction;
  error: string | null;
  message: StudioMessage | null;
  onClose: () => void;
  onToggle: (framework: LoomFramework) => Promise<void>;
  onUpgrade: (framework: LoomFramework, file: File) => Promise<void>;
}) {
  const dialogRef = useRef<HTMLDivElement | null>(null);
  const closeButtonRef = useRef<HTMLButtonElement | null>(null);
  const fileInputRefs = useRef<Record<string, HTMLInputElement | null>>({});
  const [pendingUninstallId, setPendingUninstallId] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    closeButtonRef.current?.focus();
    const handleKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
        return;
      }
      if (event.key !== "Tab" || !dialogRef.current) return;
      const focusable = [...dialogRef.current.querySelectorAll<HTMLElement>(
        'button:not([disabled]), input:not([disabled]):not([hidden]), [tabindex]:not([tabindex="-1"])',
      )];
      if (!focusable.length) return;
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
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [onClose, open]);

  useEffect(() => {
    if (!open) setPendingUninstallId(null);
  }, [open]);

  if (!open) return null;

  return (
    <div className="framework-dialog-backdrop" role="presentation" onMouseDown={(event) => {
      if (event.target === event.currentTarget) onClose();
    }}>
      <div className="framework-dialog" ref={dialogRef} role="dialog" aria-modal="true" aria-labelledby="framework-dialog-title">
        <header className="framework-dialog__header">
          <h2 id="framework-dialog-title">管理框架</h2>
          <button className="ghost-button" type="button" ref={closeButtonRef} onClick={onClose}>关闭</button>
        </header>
        {error ? <p className="error-text">{error}</p> : null}
        {message ? <p className={message.kind === "error" ? "error-text" : "success-text"}>{message.text}</p> : null}
        <div className="framework-dialog__table-wrap">
          <table className="framework-dialog__table">
            <thead><tr><th scope="col">框架</th><th scope="col">版本</th><th scope="col">安装</th><th scope="col">更新</th></tr></thead>
            <tbody>
              {frameworks.map((framework) => {
                const identity = frameworkIdentity(framework);
                const rowBusy = busyId === identity;
                const toggleBusy = rowBusy && busyAction === "toggle";
                const upgradeBusy = rowBusy && busyAction === "upgrade";
                const confirmingUninstall = framework.installed && pendingUninstallId === identity;
                return (
                  <tr key={identity}>
                    <th scope="row">{frameworkFilterLabel(framework)}</th>
                    <td>{framework.version || "—"}</td>
                    <td>
                      <button
                        className={framework.installed ? "ghost-button" : "signal-button"}
                        type="button"
                        disabled={busyId !== null}
                        onClick={() => {
                          if (!framework.installed) {
                            void onToggle(framework);
                            return;
                          }
                          if (!confirmingUninstall) {
                            setPendingUninstallId(identity);
                            return;
                          }
                          setPendingUninstallId(null);
                          void onToggle(framework);
                        }}
                      >
                        {toggleBusy ? "处理中" : confirmingUninstall ? "确认卸载" : framework.installed ? "卸载" : "安装"}
                      </button>
                    </td>
                    <td>
                      <input
                        hidden
                        ref={(element) => { fileInputRefs.current[identity] = element; }}
                        type="file"
                        accept=".zip,application/zip"
                        onChange={(event) => {
                          const file = event.currentTarget.files?.[0];
                          event.currentTarget.value = "";
                          if (file) void onUpgrade(framework, file);
                        }}
                      />
                      <button className="ghost-button" type="button" disabled={!framework.installed || busyId !== null} onClick={() => fileInputRefs.current[identity]?.click()}>
                        {upgradeBusy ? "更新中" : "更新"}
                      </button>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
          {frameworks.length === 0 && !error ? <p className="muted-line">加载中…</p> : null}
        </div>
      </div>
    </div>
  );
}
