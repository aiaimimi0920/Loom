// The controller owns rename submission; the standard dialog owns modal lifetime.
import { useRef } from "react";
import { Button, Dialog, Field, Input } from "@neuro/ui";
import { useUiText } from "../../i18n/index.ts";

interface OtterCanvasRenameDialogProps {
  value: string;
  busy: boolean;
  onChange: (value: string) => void;
  onClose: () => void;
  onSubmit: () => void;
}

export function OtterCanvasRenameDialog({ value, busy, onChange, onClose, onSubmit }: OtterCanvasRenameDialogProps) {
  const { t } = useUiText();
  const inputRef = useRef<HTMLInputElement>(null);
  return (
    <Dialog
      title={t("otter.rename.title")}
      open
      busy={busy}
      initialFocusRef={inputRef}
      className="loom-canvas-rename-dialog"
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <form
        onSubmit={(event) => {
          event.preventDefault();
          if (!busy && value.trim()) onSubmit();
        }}
      >
        <Field label={t("otter.rename.nameLabel")}>
          <Input ref={inputRef} value={value} disabled={busy} onChange={(event) => onChange(event.target.value)} />
        </Field>
        <div className="n-dialog-actions">
          <Button onClick={onClose} disabled={busy}>
            {t("common.cancel")}
          </Button>
          <Button type="submit" variant="primary" loading={busy} disabled={busy || !value.trim()}>
            {t("common.confirm")}
          </Button>
        </div>
      </form>
    </Dialog>
  );
}
