// Composes typed credential drafts; validation, revealing and persistence remain in the security owner.
import { Button, Dialog, Input, Select, Textarea } from "@neuro/ui";
import { useRef, type RefObject } from "react";
import { useUiText } from "../../i18n/index.ts";
import type { LoomCredentialValueType } from "../../services/loomApi";
import { credentialValueTypeLabels, type CredentialFieldDraft } from "../feedback/AppFeedback";

export function CredentialFieldDialog({
  draft,
  busy,
  onChange,
  onSave,
  onDelete,
  onClose,
  fallbackFocusRef,
  returnFocusRef,
}: {
  draft: CredentialFieldDraft | null;
  busy: boolean;
  onChange: (draft: CredentialFieldDraft) => void;
  onSave: () => void;
  onDelete: () => void;
  onClose: () => void;
  fallbackFocusRef?: RefObject<HTMLElement | null>;
  returnFocusRef?: RefObject<HTMLElement | null>;
}) {
  const { t } = useUiText();
  const nameInput = useRef<HTMLInputElement>(null);
  if (!draft) return null;
  return (
    <Dialog
      open
      title={draft.original ? t("security.credential.editTitle") : t("security.credential.addTitle")}
      className="loom-credential-dialog"
      busy={busy}
      initialFocusRef={nameInput}
      returnFocusRef={returnFocusRef}
      resolveReturnFocus={(target) =>
        target?.isConnected && !target.matches(":disabled") ? target : (fallbackFocusRef?.current ?? null)
      }
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <div className="loom-credential-dialog__fields">
        <Input
          ref={nameInput}
          aria-label={t("security.credential.nameAria")}
          placeholder={t("security.credential.namePlaceholder")}
          value={draft.name}
          onChange={(event) => onChange({ ...draft, name: event.target.value })}
        />
        <Select
          aria-label={t("security.credential.formatAria")}
          value={draft.valueType}
          onChange={(event) =>
            onChange({ ...draft, valueType: event.target.value as LoomCredentialValueType, value: "" })
          }
        >
          {(Object.entries(credentialValueTypeLabels) as Array<[LoomCredentialValueType, string]>).map(
            ([valueType, label]) => (
              <option key={valueType} value={valueType}>
                {label}
              </option>
            ),
          )}
        </Select>
        {draft.valueType === "boolean" ? (
          <Select
            aria-label={t("security.credential.valueAria")}
            value={draft.value}
            onChange={(event) => onChange({ ...draft, value: event.target.value })}
          >
            <option value="">{t("security.credential.selectOption")}</option>
            <option value="true">true</option>
            <option value="false">false</option>
          </Select>
        ) : draft.valueType === "json" ? (
          <Textarea
            className="loom-credential-dialog__value"
            aria-label={t("security.credential.valueAria")}
            placeholder="{}"
            value={draft.value}
            onChange={(event) => onChange({ ...draft, value: event.target.value })}
          />
        ) : (
          <Input
            aria-label={t("security.credential.valueAria")}
            type={draft.valueType === "string" ? "text" : "number"}
            step={draft.valueType === "integer" ? 1 : "any"}
            placeholder={t("security.credential.valuePlaceholder")}
            value={draft.value}
            onChange={(event) => onChange({ ...draft, value: event.target.value })}
          />
        )}
      </div>
      <div className="loom-device-dialog__actions loom-device-dialog__actions--split">
        {draft.original ? (
          <Button variant="danger" disabled={busy} onClick={onDelete}>
            {t("common.delete")}
          </Button>
        ) : (
          <span />
        )}
        <Button variant="primary" loading={busy} disabled={busy || !draft.name.trim() || !draft.value} onClick={onSave}>
          {t("common.save")}
        </Button>
      </div>
    </Dialog>
  );
}
