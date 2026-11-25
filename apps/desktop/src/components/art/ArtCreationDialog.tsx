// The standard dialog owns focus, dismissal and scroll locking; Loom owns creation state.
import { Dialog, Notice } from "@neuro/ui";
import type { ReactNode } from "react";
import type { StudioMessage } from "../app/appShell";

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
  return (
    <Dialog
      open={open}
      title="创建 Art"
      busy={busy}
      className="loom-art-create-dialog"
      resolveReturnFocus={(target) => document.querySelector<HTMLButtonElement>(".framework-filter__create") ?? target}
      onOpenChange={(next) => {
        if (!next) onClose();
      }}
    >
      <div className="art-create-dialog__scroll">
        {message ? <Notice tone={message.kind === "error" ? "danger" : "success"}>{message.text}</Notice> : null}
        {children}
      </div>
    </Dialog>
  );
}
