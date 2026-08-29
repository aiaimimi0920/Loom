// Presents immutable digest-bound permissions before activation or version changes.
import { useEffect, useRef } from "react";
import { createPortal } from "react-dom";
import { permissionLabel, shortCapabilityDigest } from "./capabilityPresentation";
import type { CapabilityPermissionReview } from "./useCapabilityExtensionsController";

const actionLabel = {
  enable: "批准并启用",
  upgrade: "批准并更新",
  rollback: "批准并回滚",
};

export function CapabilityPermissionDialog({
  review,
  busy,
  onCancel,
  onConfirm,
}: {
  review: CapabilityPermissionReview | null;
  busy: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const dialogRef = useRef<HTMLElement | null>(null);
  const confirmRef = useRef<HTMLButtonElement | null>(null);

  useEffect(() => {
    if (!review) return;
    const previousFocus = document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null;
    confirmRef.current?.focus();
    return () => previousFocus?.focus();
  }, [review]);

  if (!review) return null;
  return createPortal(
    <div className="framework-dialog-backdrop" role="presentation" onMouseDown={(event) => {
      if (!busy && event.target === event.currentTarget) onCancel();
    }}>
      <section
        ref={dialogRef}
        className="framework-dialog capability-permission-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="capability-permission-title"
        onKeyDown={(event) => {
          if (!busy && event.key === "Escape") {
            event.preventDefault();
            onCancel();
            return;
          }
          if (event.key !== "Tab") return;
          const focusable = dialogRef.current?.querySelectorAll<HTMLElement>(
            "button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex='-1'])",
          );
          if (!focusable?.length) return;
          const first = focusable[0];
          const last = focusable[focusable.length - 1];
          if (event.shiftKey && document.activeElement === first) {
            event.preventDefault();
            last.focus();
          } else if (!event.shiftKey && document.activeElement === last) {
            event.preventDefault();
            first.focus();
          }
        }}
      >
        <header className="framework-dialog__header">
          <div>
            <span className="capability-eyebrow">权限审查</span>
            <h2 id="capability-permission-title">{review.name}</h2>
            <p>权限与不可变包摘要绑定；版本变化或权限扩展后必须重新批准。</p>
          </div>
          <button className="art-card-action" type="button" disabled={busy} aria-label="关闭" onClick={onCancel}>×</button>
        </header>
        <div className="capability-permission-dialog__body">
          <dl className="capability-evidence-grid">
            <div><dt>发布者</dt><dd>{review.publisherId}</dd></div>
            <div><dt>版本</dt><dd>{review.version}</dd></div>
            <div><dt>包摘要</dt><dd title={review.digest}>{shortCapabilityDigest(review.digest)}</dd></div>
          </dl>
          <section className="capability-permission-list" aria-labelledby="capability-permission-list-title">
            <h3 id="capability-permission-list-title">请求的权限</h3>
            {review.permissions.length ? (
              <ul>{review.permissions.map((permission) => (
                <li key={permission}>
                  <strong>{permissionLabel(permission)}</strong>
                  <code>{permission}</code>
                </li>
              ))}</ul>
            ) : <p>该版本不请求额外权限。</p>}
          </section>
        </div>
        <footer className="capability-permission-dialog__footer">
          <button className="ghost-button" type="button" disabled={busy} onClick={onCancel}>取消</button>
          <button ref={confirmRef} className="signal-button" type="button" disabled={busy} onClick={onConfirm}>
            {busy ? "处理中…" : actionLabel[review.action]}
          </button>
        </footer>
      </section>
    </div>,
    document.body,
  );
}
