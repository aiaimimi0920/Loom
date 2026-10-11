// Accessible modal for renaming one saved Otter canvas workflow.

import { useEffect, useRef } from "react";

interface OtterCanvasRenameDialogProps {
  value: string;
  busy: boolean;
  onChange: (value: string) => void;
  onClose: () => void;
  onSubmit: () => void;
}

export function OtterCanvasRenameDialog({
  value,
  busy,
  onChange,
  onClose,
  onSubmit,
}: OtterCanvasRenameDialogProps) {
  const dialogRef = useRef<HTMLDivElement | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;

  useEffect(() => {
    const previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    const timer = window.setTimeout(() => inputRef.current?.focus(), 0);
    const handleKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onCloseRef.current();
        return;
      }
      if (event.key !== "Tab") return;
      const focusable = Array.from(dialogRef.current?.querySelectorAll<HTMLElement>(
        'button:not([disabled]), input:not([disabled]), [tabindex]:not([tabindex="-1"])',
      ) ?? []);
      if (!focusable.length) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      const focusInsideDialog = dialogRef.current?.contains(document.activeElement) === true;
      if (!focusInsideDialog || (event.shiftKey && document.activeElement === first)) {
        event.preventDefault();
        (event.shiftKey ? last : first).focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.clearTimeout(timer);
      window.removeEventListener("keydown", handleKeyDown);
      document.body.style.overflow = previousOverflow;
      previousFocus?.focus();
    };
  }, []);

  return (
    <div
      className="otter-canvas-rename-backdrop"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div
        ref={dialogRef}
        className="otter-canvas-rename-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="otter-canvas-rename-title"
        aria-busy={busy}
      >
        <p id="otter-canvas-rename-title" className="otter-canvas-rename-dialog__title">
          重命名工作流
        </p>
        <input
          ref={inputRef}
          className="otter-canvas-rename-dialog__input"
          value={value}
          aria-labelledby="otter-canvas-rename-title"
          onChange={(event) => onChange(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") onSubmit();
          }}
        />
        <div className="otter-canvas-rename-dialog__actions">
          <button className="ghost-button" type="button" onClick={onClose} disabled={busy}>
            取消
          </button>
          <button className="signal-button" type="button" onClick={onSubmit} disabled={busy || !value.trim()}>
            {busy ? "保存中" : "确定"}
          </button>
        </div>
      </div>
    </div>
  );
}
