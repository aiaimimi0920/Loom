// Defines settings navigation, shortcut metadata, and shared controls.
import desktopPackage from "../../../package.json";
import { type ReactNode } from "react";
import { Disclosure } from "@neuro/ui";
import { uiText, useUiText } from "../../i18n/index.ts";

export type SettingsAppId = "loom" | "hook";
export type SettingsSectionId = "general" | "shortcuts" | "mcp" | "art-store" | "capabilities" | "network" | "cache" | "about";
export type SettingsSectionIconKind = SettingsSectionId | "system";

export interface ApplicationDiagnosticsInfo {
  app: SettingsAppId;
  appName: string;
  version: string;
  repositoryUrl: string | null;
  commitShort: string | null;
  logDir: string;
  logFile: string | null;
  logFileExists: boolean;
}

export interface OtterCacheEntryInfo {
  key: string;
  label: string;
  path: string;
  bytes: number;
  fileCount: number;
}

export interface OtterCacheSnapshotInfo {
  temporary: OtterCacheEntryInfo;
  recycleBinEntries: number;
  referenceEntries: number;
}

export interface OtterCacheClearResult {
  kind: string;
  freedBytes: number;
  snapshot: OtterCacheSnapshotInfo;
}

export interface LoomCacheSnapshotInfo {
  artRuntime: OtterCacheEntryInfo;
  frameworkTemporary: OtterCacheEntryInfo;
}

export interface LoomCacheClearResult {
  kind: string;
  freedBytes: number;
  snapshot: LoomCacheSnapshotInfo;
}

export type OtterShortcutGroupIconKind = "capture" | "tools" | "sticker" | "transform";

export type OtterShortcutContext = "capture-selecting" | "sticker-editing" | "unit-selected" | "canvas";
export type OtterShortcutGestureAction = "click" | "drag" | "wheel";
export type ShortcutSlot = 0 | 1 | 2;
export type ShortcutSlots = [string, string, string];

export interface OtterShortcutDisplayItem {
  id: string;
  readonly label: string;
  readonly description: string;
  keys: string[];
  sourceId?: string;
  contexts: OtterShortcutContext[];
  gestureAction?: OtterShortcutGestureAction;
  conflictFamily?: string;
}

export interface OtterShortcutDisplayGroup {
  id: string;
  readonly label: string;
  icon: OtterShortcutGroupIconKind;
  items: OtterShortcutDisplayItem[];
}

export interface ShortcutEditorState {
  item: OtterShortcutDisplayItem;
  keys: ShortcutSlots;
  activeSlot: ShortcutSlot;
  slotCount: 1 | 2 | 3;
}

export interface QuickBindingEditorState {
  id: string;
  art: string;
  keys: ShortcutSlots;
  activeSlot: ShortcutSlot;
  slotCount: 1 | 2 | 3;
}

export const ALL_OTTER_SHORTCUT_CONTEXTS: OtterShortcutContext[] = [
  "capture-selecting",
  "sticker-editing",
  "unit-selected",
  "canvas",
];

const shortcutGroup = (
  id: string,
  icon: OtterShortcutGroupIconKind,
  items: OtterShortcutDisplayItem[],
): OtterShortcutDisplayGroup => ({
  id,
  get label() {
    return uiText(`settings.shortcuts.group.${id}`);
  },
  icon,
  items,
});

const shortcutItem = (
  id: string,
  keys: string[],
  contexts: OtterShortcutContext[],
  options: {
    sourceId?: string;
    gestureAction?: OtterShortcutGestureAction;
    conflictFamily?: string;
  } = {},
): OtterShortcutDisplayItem => ({
  id,
  get label() {
    return uiText(`settings.shortcuts.item.${id}.label`);
  },
  get description() {
    return uiText(`settings.shortcuts.item.${id}.description`);
  },
  keys,
  contexts,
  ...options,
});

export const HOOK_SHORTCUT_GROUPS: OtterShortcutDisplayGroup[] = [
  shortcutGroup("capture-file", "capture", [
    shortcutItem("capture", ["Ctrl+1"], ALL_OTTER_SHORTCUT_CONTEXTS, { sourceId: "capture" }),
    shortcutItem("live-capture", ["Ctrl+2"], ALL_OTTER_SHORTCUT_CONTEXTS, { sourceId: "live_capture" }),
    shortcutItem("long-capture", ["Ctrl+3"], ALL_OTTER_SHORTCUT_CONTEXTS, { sourceId: "long_capture" }),
    shortcutItem("open-image", ["Ctrl+O"], ALL_OTTER_SHORTCUT_CONTEXTS, { sourceId: "open_image" }),
    shortcutItem("save", ["Ctrl+S"], ["unit-selected"], { sourceId: "save_image" }),
    shortcutItem("toggle-clean-view", ["Ctrl+Shift+4"], ALL_OTTER_SHORTCUT_CONTEXTS, { sourceId: "toggle_clean_view" }),
    shortcutItem("cancel", ["Escape", "Delete", "Backspace"], ["capture-selecting", "sticker-editing", "unit-selected"], {
      sourceId: "cancel",
      conflictFamily: "contextual-cancel-delete",
    }),
    shortcutItem("force-close", ["Esc × 3"], ALL_OTTER_SHORTCUT_CONTEXTS),
  ]),
  shortcutGroup("panels-tools", "tools", [
    shortcutItem("toggle-actions", ["Shift+1"], ["unit-selected"], { sourceId: "toggle_actions" }),
    shortcutItem("toggle-params", ["Tab"], ["unit-selected"], { sourceId: "toggle_params" }),
  ]),
  shortcutGroup("sticker-operation", "sticker", [
    shortcutItem("copy", ["Ctrl+C"], ["unit-selected"], { sourceId: "copy_unit" }),
    shortcutItem("paste", ["Ctrl+V"], ["unit-selected", "canvas"], { sourceId: "paste_unit" }),
    shortcutItem("delete", ["Escape", "Delete", "Backspace"], ["unit-selected"], {
      sourceId: "delete_unit",
      conflictFamily: "contextual-cancel-delete",
    }),
    shortcutItem("sticker-resize", ["Ctrl+Wheel"], ["unit-selected"], { sourceId: "sticker_resize", gestureAction: "wheel" }),
    shortcutItem("sticker-opacity", ["Alt+Wheel"], ["unit-selected"], { sourceId: "sticker_opacity", gestureAction: "wheel" }),
    shortcutItem("drag-align", ["Alt+Drag"], ["unit-selected"], { sourceId: "drag_alignment", gestureAction: "drag" }),
    shortcutItem("drag-out", ["Shift+Drag"], ["unit-selected"], { sourceId: "drag_out", gestureAction: "drag" }),
    shortcutItem("drag-cascade", ["Ctrl+Drag"], ["unit-selected"], { sourceId: "drag_cascade", gestureAction: "drag" }),
  ]),
  shortcutGroup("sticker-edit", "transform", [
    shortcutItem("toggle-sticker-toolbar", ["Ctrl+E"], ALL_OTTER_SHORTCUT_CONTEXTS, { sourceId: "toggle_sticker_toolbar" }),
    shortcutItem("control-copy", ["Ctrl+C"], ["sticker-editing"], { sourceId: "copy_unit" }),
    shortcutItem("control-paste", ["Ctrl+V"], ["sticker-editing"], { sourceId: "paste_unit" }),
    shortcutItem("control-delete", ["Escape", "Delete", "Backspace"], ["sticker-editing"], {
      sourceId: "delete_unit",
      conflictFamily: "contextual-cancel-delete",
    }),
    shortcutItem("undo-edit", ["Ctrl+Z"], ["sticker-editing"], { sourceId: "undo_edit" }),
    shortcutItem("redo-edit", ["Ctrl+Y"], ["sticker-editing"], { sourceId: "redo_edit" }),
    shortcutItem("transform-select", ["Q"], ["unit-selected", "sticker-editing"], { sourceId: "transform_select" }),
    shortcutItem("transform-move", ["W"], ["unit-selected", "sticker-editing"], { sourceId: "transform_move" }),
    shortcutItem("transform-rotate", ["E"], ["unit-selected", "sticker-editing"], { sourceId: "transform_rotate" }),
    shortcutItem("transform-scale", ["R"], ["unit-selected", "sticker-editing"], { sourceId: "transform_scale" }),
    shortcutItem("control-multi-select", ["Shift+Click"], ["sticker-editing"], { sourceId: "control_multi_select", gestureAction: "click" }),
    shortcutItem("control-quick-move", ["Alt+Drag"], ["sticker-editing"], { sourceId: "control_quick_move", gestureAction: "drag" }),
    shortcutItem("control-quick-rotate", ["Ctrl+Drag"], ["sticker-editing"], { sourceId: "control_quick_rotate", gestureAction: "drag" }),
    shortcutItem("control-scale", ["Ctrl+Alt+Wheel"], ["sticker-editing"], { sourceId: "control_scale", gestureAction: "wheel" }),
    shortcutItem("control-scale-own-center", ["Ctrl+Alt+Shift+Wheel"], ["sticker-editing"], {
      sourceId: "control_scale_own_center",
      gestureAction: "wheel",
    }),
  ]),
];

export function OtterShortcutGroupIcon({ kind }: { kind: OtterShortcutGroupIconKind }) {
  const props = {
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.8,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    "aria-hidden": true,
  };
  switch (kind) {
    case "capture":
      return <svg {...props}><path d="M8 4H5a1 1 0 0 0-1 1v3M16 4h3a1 1 0 0 1 1 1v3M8 20H5a1 1 0 0 1-1-1v-3M16 20h3a1 1 0 0 0 1-1v-3" /><rect x="8" y="8" width="8" height="8" rx="1" /></svg>;
    case "tools":
      return <svg {...props}><path d="M4 7h10M18 7h2M4 17h2M10 17h10" /><circle cx="16" cy="7" r="2" /><circle cx="8" cy="17" r="2" /></svg>;
    case "sticker":
      return <svg {...props}><rect x="3" y="4" width="18" height="16" rx="2" /><path d="m6 16 4-4 3 3 2-2 3 3M8 8h.01" /></svg>;
    case "transform":
      return <svg {...props}><path d="M8 3H3v5M16 3h5v5M8 21H3v-5M16 21h5v-5M3 8l6-5M21 8l-6-5M3 16l6 5M21 16l-6 5" /></svg>;
    default:
      return null;
  }
}

export const splitShortcutAlternatives = (value: string) => value
  .split(/\s*\/\s*/)
  .map((shortcut) => shortcut.trim())
  .filter(Boolean);

export const shortcutKeyParts = (shortcut: string) => shortcut
  .split("+")
  .map((key) => key.trim())
  .filter(Boolean);

const GESTURE_CANONICAL_TOKEN: Record<OtterShortcutGestureAction, string> = {
  click: "Click",
  drag: "Drag",
  wheel: "Wheel",
};

export const gestureActionLabel = (action: OtterShortcutGestureAction): string =>
  uiText(`settings.shortcuts.gesture.${action}`);

export const shortcutKeyDisplayLabel = (key: string) => {
  if (key === "Escape") return "Esc";
  if (key === "Click" || key === "click" || key === "\u70b9\u51fb") return gestureActionLabel("click");
  if (key === "Drag" || key === "drag" || key === "\u62d6\u52a8") return gestureActionLabel("drag");
  if (key === "Wheel" || key === "wheel" || key === "\u6eda\u8f6e") return gestureActionLabel("wheel");
  return key;
};

export const normalizeShortcutComparisonKey = (shortcut: string): string =>
  shortcutKeyParts(shortcut)
    .map((part) => {
      if (part === "\u70b9\u51fb") return "click";
      if (part === "\u62d6\u52a8") return "drag";
      if (part === "\u6eda\u8f6e") return "wheel";
      return part.toLocaleLowerCase();
    })
    .join("+");

export const SHORTCUT_GESTURE_MODIFIERS = ["Ctrl", "Alt", "Shift", "Meta"] as const;
export type ShortcutGestureModifier = typeof SHORTCUT_GESTURE_MODIFIERS[number];

export const gestureShortcutModifiers = (shortcut: string, _action: OtterShortcutGestureAction) => new Set(
  shortcutKeyParts(shortcut).filter((part): part is ShortcutGestureModifier => (
    SHORTCUT_GESTURE_MODIFIERS.includes(part as ShortcutGestureModifier)
  )),
);

export const toggleGestureShortcutModifier = (
  shortcut: string,
  action: OtterShortcutGestureAction,
  modifier: ShortcutGestureModifier,
) => {
  const selected = gestureShortcutModifiers(shortcut, action);
  if (selected.has(modifier)) selected.delete(modifier);
  else selected.add(modifier);
  return [
    ...SHORTCUT_GESTURE_MODIFIERS.filter((candidate) => selected.has(candidate)),
    GESTURE_CANONICAL_TOKEN[action],
  ].join("+");
};

export const shortcutContextsOverlap = (
  left: readonly OtterShortcutContext[],
  right: readonly OtterShortcutContext[],
) => left.some((context) => right.includes(context));

export const shortcutSlotCount = (keys: readonly string[]): 1 | 2 | 3 => (
  keys[2] ? 3 : keys[1] ? 2 : 1
);

export const removeShortcutSlot = (keys: ShortcutSlots, slot: ShortcutSlot): ShortcutSlots => {
  if (slot === 0) return ["", keys[1], keys[2]];
  if (slot === 1) return [keys[0], keys[2], ""];
  return [keys[0], keys[1], ""];
};

export function ShortcutKeySequence({ shortcuts }: { shortcuts: string[] }) {
  const { t } = useUiText();
  return (
    <span className="otter-shortcut-key-sequences">
      {shortcuts.map((shortcut, shortcutIndex) => (
        <span className="otter-shortcut-key-sequence" key={`${shortcut}-${shortcutIndex}`}>
          {shortcutKeyParts(shortcut).map((key, keyIndex) => (
            <span className="otter-shortcut-key-part" key={`${key}-${keyIndex}`}>
              {keyIndex > 0 ? <span className="otter-shortcut-key-plus" aria-hidden="true">+</span> : null}
              <kbd>{shortcutKeyDisplayLabel(key)}</kbd>
            </span>
          ))}
          {shortcutIndex < shortcuts.length - 1 ? <span className="otter-shortcut-key-or">{t("settings.shortcuts.or")}</span> : null}
        </span>
      ))}
    </span>
  );
}

export const shortcutKeyFromKeyboardEvent = (event: globalThis.KeyboardEvent) => {
  if (/^Key[A-Z]$/.test(event.code)) return event.code.slice(3);
  if (/^Digit[0-9]$/.test(event.code)) return event.code.slice(5);
  if (/^Numpad[0-9]$/.test(event.code)) return `Num${event.code.slice(6)}`;
  const aliases: Record<string, string> = {
    " ": "Space",
    Control: "Ctrl",
    Meta: "Meta",
  };
  return aliases[event.key] ?? event.key;
};

export const shortcutFromKeyboardEvent = (event: globalThis.KeyboardEvent) => {
  const key = shortcutKeyFromKeyboardEvent(event);
  if (["Ctrl", "Alt", "Shift", "Meta"].includes(key)) return null;
  const parts: string[] = [];
  if (event.ctrlKey) parts.push("Ctrl");
  if (event.altKey) parts.push("Alt");
  if (event.shiftKey) parts.push("Shift");
  if (event.metaKey) parts.push("Meta");
  parts.push(key);
  return parts.join("+");
};

export const FALLBACK_APPLICATION_DIAGNOSTICS: Record<SettingsAppId, ApplicationDiagnosticsInfo> = {
  loom: {
    app: "loom",
    appName: "Loom",
    version: desktopPackage.version,
    repositoryUrl: "https://github.com/aiaimimi0920/Loom",
    commitShort: null,
    logDir: "",
    logFile: null,
    logFileExists: false,
  },
  hook: {
    app: "hook",
    appName: "Otter",
    version: "0.1.7",
    repositoryUrl: "https://github.com/aiaimimi0920/Otter",
    commitShort: null,
    logDir: "",
    logFile: null,
    logFileExists: false,
  },
};

export function SettingsSectionIcon({ kind }: { kind: SettingsSectionIconKind }) {
  const iconProps = {
    className: "settings-section__icon",
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.8,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    "aria-hidden": true,
  };
  switch (kind) {
    case "general":
      return <svg {...iconProps}><path d="M4 7h10M18 7h2M4 17h2M10 17h10" /><circle cx="16" cy="7" r="2" /><circle cx="8" cy="17" r="2" /></svg>;
    case "shortcuts":
      return <svg {...iconProps}><rect x="3" y="5" width="18" height="14" rx="2" /><path d="M7 9h.01M11 9h.01M15 9h.01M7 13h.01M11 13h6M7 16h10" /></svg>;
    case "mcp":
      return <svg {...iconProps}><rect x="3" y="5" width="7" height="5" rx="1" /><rect x="14" y="14" width="7" height="5" rx="1" /><path d="M10 7.5h4a3 3 0 0 1 3 3V14M14 16.5h-4a3 3 0 0 1-3-3V10" /></svg>;
    case "art-store":
      return <svg {...iconProps}><path d="M4 9h16l-1-4H5L4 9Z" /><path d="M5 9v10h14V9M9 19v-6h6v6" /><path d="M4 9c0 2 3 2 4 0 1 2 3 2 4 0 1 2 3 2 4 0 1 2 4 2 4 0" /></svg>;
    case "system":
      return <svg {...iconProps}><path d="M12 3v4M12 17v4M3 12h4M17 12h4M5.6 5.6l2.8 2.8M15.6 15.6l2.8 2.8M18.4 5.6l-2.8 2.8M8.4 15.6l-2.8 2.8" /><circle cx="12" cy="12" r="3" /></svg>;
    case "network":
      return <svg {...iconProps}><circle cx="12" cy="12" r="9" /><path d="M3.5 9h17M3.5 15h17M12 3c2.2 2.4 3.3 5.4 3.3 9S14.2 18.6 12 21M12 3C9.8 5.4 8.7 8.4 8.7 12s1.1 6.6 3.3 9" /></svg>;
    case "cache":
      return <svg {...iconProps}><path d="M4 7h16v10H4zM7 10h10M7 14h6" /></svg>;
    case "about":
      return <svg {...iconProps}><circle cx="12" cy="12" r="9" /><path d="M12 11v5M12 8h.01" /></svg>;
    default:
      return null;
  }
}

export function SettingsAccordionSection({
  id,
  label,
  open,
  onToggle,
  children,
}: {
  id: SettingsSectionId;
  label: string;
  open: boolean;
  onToggle: () => void;
  children: ReactNode;
}) {
  const contentId = `settings-section-${id}`;
  return (
    <Disclosure className="settings-section" open={open} onOpenChange={onToggle}
      title={<h2 className="settings-section__heading"><SettingsSectionIcon kind={id} /><span>{label}</span></h2>}>
      {open ? <div className="settings-section__body" id={contentId}>{children}</div> : null}
    </Disclosure>
  );
}
