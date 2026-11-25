// The controller owns rename submission; the standard dialog owns modal lifetime.
import { useRef } from "react";
import { Button, Dialog, Field, Input } from "@neuro/ui";

interface OtterCanvasRenameDialogProps {
  value: string;
  busy: boolean;
  onChange: (value: string) => void;
  onClose: () => void;
  onSubmit: () => void;
}

export function OtterCanvasRenameDialog({ value, busy, onChange, onClose, onSubmit }: OtterCanvasRenameDialogProps) {
  const inputRef = useRef<HTMLInputElement>(null);
  return (
    <Dialog
      title="重命名工作流"
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
        <Field label="工作流名称">
          <Input ref={inputRef} value={value} disabled={busy} onChange={(event) => onChange(event.target.value)} />
        </Field>
        <div className="n-dialog-actions">
          <Button onClick={onClose} disabled={busy}>
            取消
          </Button>
          <Button type="submit" variant="primary" loading={busy} disabled={busy || !value.trim()}>
            确定
          </Button>
        </div>
      </form>
    </Dialog>
  );
}
