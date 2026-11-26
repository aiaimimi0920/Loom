// Captures application key sequences; ordinary actions and modifiers use standard controls.
import { Button, Checkbox } from "@neuro/ui";
import { useLayoutEffect, useRef, type KeyboardEvent, type RefObject } from "react";
import { useUiText } from "../../i18n/index.ts";
import {
  gestureActionLabel, gestureShortcutModifiers, removeShortcutSlot, SHORTCUT_GESTURE_MODIFIERS, ShortcutKeySequence,
  toggleGestureShortcutModifier, type OtterShortcutGestureAction, type ShortcutSlot, type ShortcutSlots,
} from "./settingsModel";

interface SlotsValue { keys: ShortcutSlots; activeSlot: ShortcutSlot; slotCount: 1 | 2 | 3 }
export function ShortcutSlotsEditor({ value, label, gestureAction, onChange, onCapture, captureRef, gestureRef }: {
  value: SlotsValue;
  label: string;
  gestureAction?: OtterShortcutGestureAction;
  onChange(patch: Partial<SlotsValue>): void;
  onCapture(event: KeyboardEvent<HTMLButtonElement>, slot: ShortcutSlot): void;
  captureRef: RefObject<HTMLButtonElement | null>;
  gestureRef?: RefObject<HTMLInputElement | null>;
}) {
  const { t, tf } = useUiText();
  const previousCount = useRef(value.slotCount);
  useLayoutEffect(() => {
    // Dialog owns initial focus; only a later add/remove moves to the remaining active slot.
    if (previousCount.current !== value.slotCount) (gestureAction ? gestureRef?.current : captureRef.current)?.focus();
    previousCount.current = value.slotCount;
  }, [value.slotCount, gestureAction, captureRef, gestureRef]);
  return <>
    {Array.from({ length: value.slotCount }, (_, index) => index as ShortcutSlot).map(slot => {
      const keys = value.keys[slot];
      const slotIndex = slot + 1;
      return <div className="shortcut-capture-slot" key={slot}>
        <span>{tf("settings.shortcuts.slotLabel", { index: slotIndex })}</span>
        <div className="shortcut-capture-slot__controls">
          {gestureAction ? <div className="shortcut-gesture-picker" role="group" aria-label={tf("settings.shortcuts.slotAria", { label, index: slotIndex })}>
            <span className="shortcut-gesture-picker__action">{gestureActionLabel(gestureAction)}</span>
            <div className="shortcut-gesture-picker__modifiers">
              {SHORTCUT_GESTURE_MODIFIERS.map((modifier, index) => <Checkbox key={modifier} label={modifier}
                ref={slot === value.activeSlot && index === 0 ? gestureRef : undefined}
                checked={gestureShortcutModifiers(keys, gestureAction).has(modifier)} onChange={() => onChange({
                  activeSlot: slot,
                  keys: value.keys.map((key, keyIndex) => keyIndex === slot ? toggleGestureShortcutModifier(key, gestureAction, modifier) : key) as ShortcutSlots,
                })} />)}
            </div>
          </div> : <button className={value.activeSlot === slot ? "shortcut-capture-field shortcut-capture-field--active" : "shortcut-capture-field"}
            type="button" ref={slot === value.activeSlot ? captureRef : undefined} aria-label={tf("settings.shortcuts.slotAria", { label, index: slotIndex })}
            onFocus={() => onChange({ activeSlot: slot })} onKeyDown={event => onCapture(event, slot)}>
            {keys ? <ShortcutKeySequence shortcuts={[keys]} /> : <strong>{t("settings.shortcuts.notSet")}</strong>}
            <small>{t("settings.shortcuts.pressNewShortcut")}</small>
          </button>}
          <Button compact variant="quiet" disabled={slot === 0 && !keys} onClick={() => onChange({
            activeSlot: slot > 0 ? 0 : value.activeSlot,
            slotCount: slot > 0 ? Math.max(1, value.slotCount - 1) as 1 | 2 | 3 : value.slotCount,
            keys: removeShortcutSlot(value.keys, slot),
          })}>{slot > 0 ? t("settings.shortcuts.deleteSlot") : t("settings.shortcuts.clearSlot")}</Button>
        </div>
      </div>;
    })}
    {value.slotCount < 3 ? <Button compact className="shortcut-add-secondary" onClick={() => onChange({
      activeSlot: value.slotCount as ShortcutSlot, slotCount: (value.slotCount + 1) as 2 | 3,
    })}>{t("settings.shortcuts.addSecondary")}</Button> : null}
  </>;
}
