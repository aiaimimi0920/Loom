// Presentation only: the controller retains drafts, conflict checks, capture and apply semantics.
import { Button, Dialog, Field, Notice, Select } from "@neuro/ui";
import { useRef } from "react";
import { ShortcutSlotsEditor } from "./ShortcutSlotsEditor";
import { shortcutSlotCount } from "./settingsModel";
import type { useSettingsPanelController } from "./useSettingsPanelController";

type Controller = ReturnType<typeof useSettingsPanelController>;
function captureEscape(event: KeyboardEvent) {
  // Radix sees Escape in document capture before React's key-capture handler.
  // Modified Escape is a binding only while the app's capture field owns focus.
  if (
    (event.ctrlKey || event.altKey || event.shiftKey || event.metaKey) &&
    event.target instanceof Element &&
    event.target.closest(".shortcut-capture-field")
  )
    event.preventDefault();
}

export function ShortcutEditDialog({ controller }: { controller: Controller }) {
  const { shortcutEditor, setShortcutEditor, shortcutEditorConflict, applyShortcutEditor, handleShortcutCapture } =
    controller;
  const capture = useRef<HTMLButtonElement>(null),
    gesture = useRef<HTMLInputElement>(null);
  const close = () => setShortcutEditor(null);
  return (
    <Dialog
      title="修改快捷键"
      open={Boolean(shortcutEditor)}
      onOpenChange={(open) => {
        if (!open) close();
      }}
      description={shortcutEditor?.item.label}
      className="shortcut-edit-dialog"
      onEscapeKeyDown={captureEscape}
      initialFocusRef={shortcutEditor?.item.gestureAction ? gesture : capture}
    >
      {shortcutEditor ? (
        <>
          <div className="shortcut-edit-dialog__body">
            <ShortcutSlotsEditor
              value={shortcutEditor}
              label={shortcutEditor.item.label}
              gestureAction={shortcutEditor.item.gestureAction}
              captureRef={capture}
              gestureRef={gesture}
              onCapture={handleShortcutCapture}
              onChange={(patch) => setShortcutEditor((current) => (current ? { ...current, ...patch } : current))}
            />
            {shortcutEditorConflict ? <Notice tone="warning">{shortcutEditorConflict}</Notice> : null}
          </div>
          <footer className="shortcut-edit-dialog__footer">
            <Button
              compact
              onClick={() =>
                setShortcutEditor((current) =>
                  current
                    ? {
                        ...current,
                        keys: [current.item.keys[0] || "", current.item.keys[1] || "", current.item.keys[2] || ""],
                        activeSlot: 0,
                        slotCount: shortcutSlotCount(current.item.keys),
                      }
                    : current,
                )
              }
            >
              恢复默认
            </Button>
            <span />
            <Button compact onClick={close}>
              取消
            </Button>
            <Button
              compact
              variant="primary"
              disabled={!shortcutEditor.keys.some((keys) => keys.trim()) || Boolean(shortcutEditorConflict)}
              onClick={applyShortcutEditor}
            >
              应用
            </Button>
          </footer>
        </>
      ) : null}
    </Dialog>
  );
}

export function QuickBindingDialog({ controller }: { controller: Controller }) {
  const {
    quickBindingEditor,
    setQuickBindingEditor,
    quickBindingConflict,
    availableArtTools,
    applyQuickBindingEditor,
    handleQuickBindingCapture,
  } = controller;
  const capture = useRef<HTMLButtonElement>(null);
  const close = () => setQuickBindingEditor(null);
  return (
    <Dialog
      title="Art 快捷键"
      open={Boolean(quickBindingEditor)}
      onOpenChange={(open) => {
        if (!open) close();
      }}
      className="shortcut-edit-dialog quick-binding-dialog"
      initialFocusRef={capture}
      onEscapeKeyDown={captureEscape}
    >
      {quickBindingEditor ? (
        <>
          <div className="shortcut-edit-dialog__body">
            <Field label="Art">
              <Select
                value={quickBindingEditor.art}
                onChange={(event) =>
                  setQuickBindingEditor((current) => (current ? { ...current, art: event.target.value } : current))
                }
              >
                {availableArtTools.map((tool) => (
                  <option value={tool.id} key={tool.id}>
                    {tool.name || tool.id}
                  </option>
                ))}
              </Select>
            </Field>
            <ShortcutSlotsEditor
              value={quickBindingEditor}
              label="Art"
              captureRef={capture}
              onCapture={handleQuickBindingCapture}
              onChange={(patch) => setQuickBindingEditor((current) => (current ? { ...current, ...patch } : current))}
            />
            {quickBindingConflict ? <Notice tone="warning">{quickBindingConflict}</Notice> : null}
          </div>
          <footer className="shortcut-edit-dialog__footer shortcut-edit-dialog__footer--simple">
            <span />
            <span />
            <Button compact onClick={close}>
              取消
            </Button>
            <Button
              compact
              variant="primary"
              disabled={
                !quickBindingEditor.art ||
                !quickBindingEditor.keys.some((keys) => keys.trim()) ||
                Boolean(quickBindingConflict)
              }
              onClick={applyQuickBindingEditor}
            >
              应用
            </Button>
          </footer>
        </>
      ) : null}
    </Dialog>
  );
}
