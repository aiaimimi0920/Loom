// Selects the source mode for a new Art.
import { StudioMessage } from "../app/appShell";
import {
  type KeyboardEvent,
  type ReactNode,
  useEffect,
  useRef,
} from "react";

export function ArtCreationDialog({
  open,
  busy,
  message,
  onClose,
  children,
}: {
  open: boolean;
  busy: boolean;
  message: StudioMessage | null;
  onClose: () => void;
  children: ReactNode;
}) {
  const dialogRef = useRef<HTMLDivElement | null>(null);
  const closeButtonRef = useRef<HTMLButtonElement | null>(null);

  useEffect(() => {
    if (!open) return;
    closeButtonRef.current?.focus();
    const handleKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        if (!busy) onClose();
        return;
      }
      if (event.key !== "Tab" || !dialogRef.current) return;
      const focusable = [...dialogRef.current.querySelectorAll<HTMLElement>(
        'button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])',
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
  }, [busy, onClose, open]);

  if (!open) return null;

  return (
    <div
      className="framework-dialog-backdrop"
      role="presentation"
      onMouseDown={(event) => {
        if (!busy && event.target === event.currentTarget) onClose();
      }}
    >
      <div
        className="framework-dialog art-create-dialog"
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="art-create-dialog-title"
      >
        <header className="framework-dialog__header">
          <h2 id="art-create-dialog-title">创建 Art</h2>
          <button
            className="ghost-button"
            type="button"
            ref={closeButtonRef}
            onClick={onClose}
            disabled={busy}
          >
            关闭
          </button>
        </header>
        <div className="art-create-dialog__scroll">
          {message ? (
            <p className={message.kind === "error" ? "error-text" : "success-text"}>{message.text}</p>
          ) : null}
          {children}
        </div>
      </div>
    </div>
  );
}
