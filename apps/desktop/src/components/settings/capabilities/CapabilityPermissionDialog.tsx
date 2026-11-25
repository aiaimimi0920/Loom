// Presents immutable digest-bound permissions before activation or version changes.
import { useRef } from "react";
import { Button, Dialog } from "@neuro/ui";
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
  const confirmRef = useRef<HTMLButtonElement | null>(null);

  if (!review) return null;
  return (
    <Dialog open title={`权限审查 · ${review.name}`} className="capability-permission-dialog"
      description="权限与不可变包摘要绑定；版本变化或权限扩展后必须重新批准。"
      busy={busy} initialFocusRef={confirmRef}
      onOpenChange={(next) => { if (!next) onCancel(); }}>
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
        <footer className="n-dialog-actions">
          <Button type="button" disabled={busy} onClick={onCancel}>取消</Button>
          <Button ref={confirmRef} variant="primary" type="button" loading={busy} onClick={onConfirm}>
            {busy ? "处理中…" : actionLabel[review.action]}
          </Button>
        </footer>
    </Dialog>
  );
}
