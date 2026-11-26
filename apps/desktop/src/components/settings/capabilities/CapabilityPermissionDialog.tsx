// Presents immutable digest-bound permissions before activation or version changes.
import { useRef } from "react";
import { Button, Dialog } from "@neuro/ui";
import { useUiText } from "../../../i18n/index.ts";
import { permissionLabel, shortCapabilityDigest } from "./capabilityPresentation";
import type { CapabilityPermissionAction, CapabilityPermissionReview } from "./useCapabilityExtensionsController";

const ACTION_LABEL_KEYS: Record<CapabilityPermissionAction, string> = {
  enable: "settings.capabilities.review.actionEnable",
  upgrade: "settings.capabilities.review.actionUpgrade",
  rollback: "settings.capabilities.review.actionRollback",
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
  const { t, tf } = useUiText();
  const confirmRef = useRef<HTMLButtonElement | null>(null);

  if (!review) return null;
  return (
    <Dialog open title={tf("settings.capabilities.review.title", { name: review.name })} className="capability-permission-dialog"
      description={t("settings.capabilities.review.description")}
      busy={busy} initialFocusRef={confirmRef}
      onOpenChange={(next) => { if (!next) onCancel(); }}>
        <div className="capability-permission-dialog__body">
          <dl className="capability-evidence-grid">
            <div><dt>{t("settings.capabilities.review.publisherLabel")}</dt><dd>{review.publisherId}</dd></div>
            <div><dt>{t("settings.capabilities.versionLabel")}</dt><dd>{review.version}</dd></div>
            <div><dt>{t("settings.capabilities.review.packageDigestLabel")}</dt><dd title={review.digest}>{shortCapabilityDigest(review.digest)}</dd></div>
          </dl>
          <section className="capability-permission-list" aria-labelledby="capability-permission-list-title">
            <h3 id="capability-permission-list-title">{t("settings.capabilities.review.requestedHeading")}</h3>
            {review.permissions.length ? (
              <ul>{review.permissions.map((permission) => (
                <li key={permission}>
                  <strong>{permissionLabel(permission)}</strong>
                  <code>{permission}</code>
                </li>
              ))}</ul>
            ) : <p>{t("settings.capabilities.review.noRequestedPermissions")}</p>}
          </section>
        </div>
        <footer className="n-dialog-actions">
          <Button type="button" disabled={busy} onClick={onCancel}>{t("settings.capabilities.review.cancel")}</Button>
          <Button ref={confirmRef} variant="primary" type="button" loading={busy} onClick={onConfirm}>
            {busy ? t("settings.capabilities.review.processing") : t(ACTION_LABEL_KEYS[review.action])}
          </Button>
        </footer>
    </Dialog>
  );
}
