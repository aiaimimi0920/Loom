// Defines settings navigation, shortcut metadata, and shared controls.
import desktopPackage from "../../../package.json";
import { type KeyboardEvent, type ReactNode } from "react";

export type SettingsAppId = "loom" | "hook";
export type SettingsSectionId = "general" | "shortcuts" | "mcp" | "art-store" | "network" | "cache" | "about";
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

export interface HookCacheEntryInfo {
  key: string;
  label: string;
  path: string;
  bytes: number;
  fileCount: number;
}

export interface HookCacheSnapshotInfo {
  temporary: HookCacheEntryInfo;
  recycleBinEntries: number;
  referenceEntries: number;
}

export interface HookCacheClearResult {
  kind: string;
  freedBytes: number;
  snapshot: HookCacheSnapshotInfo;
}

export interface LoomCacheSnapshotInfo {
  artRuntime: HookCacheEntryInfo;
  frameworkTemporary: HookCacheEntryInfo;
}

export interface LoomCacheClearResult {
  kind: string;
  freedBytes: number;
  snapshot: LoomCacheSnapshotInfo;
}

export type HookShortcutGroupIconKind = "capture" | "tools" | "sticker" | "transform";

export type HookShortcutContext = "capture-selecting" | "sticker-editing" | "unit-selected" | "canvas";
export type HookShortcutGestureAction = "点击" | "拖动" | "滚轮";
export type ShortcutSlot = 0 | 1 | 2;
export type ShortcutSlots = [string, string, string];

export interface HookShortcutDisplayItem {
  id: string;
  label: string;
  description: string;
  keys: string[];
  sourceId?: string;
  contexts: HookShortcutContext[];
  gestureAction?: HookShortcutGestureAction;
  conflictFamily?: string;
}

export interface HookShortcutDisplayGroup {
  id: string;
  label: string;
  icon: HookShortcutGroupIconKind;
  items: HookShortcutDisplayItem[];
}

export interface ShortcutEditorState {
  item: HookShortcutDisplayItem;
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

export const ALL_HOOK_SHORTCUT_CONTEXTS: HookShortcutContext[] = [
  "capture-selecting",
  "sticker-editing",
  "unit-selected",
  "canvas",
];

export const HOOK_SHORTCUT_GROUPS: HookShortcutDisplayGroup[] = [
  {
    id: "capture-file",
    label: "捕获与操作",
    icon: "capture",
    items: [
      { id: "capture", sourceId: "capture", label: "截图", description: "截取屏幕区域", keys: ["Ctrl+1"], contexts: ALL_HOOK_SHORTCUT_CONTEXTS },
      { id: "long-capture", sourceId: "long_capture", label: "长截图", description: "开始或结束长截图", keys: ["Ctrl+3"], contexts: ALL_HOOK_SHORTCUT_CONTEXTS },
      { id: "open-image", sourceId: "open_image", label: "打开图片", description: "导入图片并创建贴图", keys: ["Ctrl+O"], contexts: ALL_HOOK_SHORTCUT_CONTEXTS },
      { id: "save", sourceId: "save_image", label: "保存图片", description: "保存当前贴图的正式输出", keys: ["Ctrl+S"], contexts: ["unit-selected"] },
      { id: "toggle-clean-view", sourceId: "toggle_clean_view", label: "清爽视图", description: "显示或隐藏界面辅助控件", keys: ["Ctrl+4"], contexts: ALL_HOOK_SHORTCUT_CONTEXTS },
      { id: "cancel", sourceId: "cancel", label: "取消 / 退出", description: "根据当前状态取消、删除或退出", keys: ["Escape", "Delete", "Backspace"], contexts: ["capture-selecting", "sticker-editing", "unit-selected"], conflictFamily: "contextual-cancel-delete" },
      { id: "force-close", label: "强行关闭", description: "连续按下 3 次 Esc 强行退出 Hook", keys: ["Esc × 3"], contexts: ALL_HOOK_SHORTCUT_CONTEXTS },
    ],
  },
  {
    id: "panels-tools",
    label: "高级工具",
    icon: "tools",
    items: [
      { id: "toggle-actions", sourceId: "toggle_actions", label: "Art 菜单", description: "显示或隐藏添加 Art 面板", keys: ["Shift+1"], contexts: ["unit-selected"] },
      { id: "toggle-params", sourceId: "toggle_params", label: "参数面板", description: "显示或隐藏当前节点参数", keys: ["Tab"], contexts: ["unit-selected"] },
    ],
  },
  {
    id: "sticker-operation",
    label: "贴图操作",
    icon: "sticker",
    items: [
      { id: "copy", sourceId: "copy_unit", label: "复制贴图", description: "复制当前选中的完整贴图", keys: ["Ctrl+C"], contexts: ["unit-selected"] },
      { id: "paste", sourceId: "paste_unit", label: "粘贴贴图", description: "在鼠标位置粘贴完整贴图", keys: ["Ctrl+V"], contexts: ["unit-selected", "canvas"] },
      { id: "delete", sourceId: "delete_unit", label: "删除贴图", description: "删除当前选中的完整贴图", keys: ["Escape", "Delete", "Backspace"], contexts: ["unit-selected"], conflictFamily: "contextual-cancel-delete" },
      { id: "sticker-resize", sourceId: "sticker_resize", label: "调整尺寸", description: "缩放当前贴图的整体尺寸", keys: ["Ctrl+滚轮"], contexts: ["unit-selected"], gestureAction: "滚轮" },
      { id: "sticker-opacity", sourceId: "sticker_opacity", label: "调整透明度", description: "调整当前贴图的整体透明度", keys: ["Alt+滚轮"], contexts: ["unit-selected"], gestureAction: "滚轮" },
      { id: "drag-align", sourceId: "drag_alignment", label: "吸附对齐", description: "拖动贴图时启用吸附与对齐", keys: ["Alt+拖动"], contexts: ["unit-selected"], gestureAction: "拖动" },
      { id: "drag-out", sourceId: "drag_out", label: "拖出文件", description: "将贴图拖出为本地文件", keys: ["Shift+拖动"], contexts: ["unit-selected"], gestureAction: "拖动" },
      { id: "drag-cascade", sourceId: "drag_cascade", label: "层叠放置", description: "拖动贴图时采用层叠放置", keys: ["Ctrl+拖动"], contexts: ["unit-selected"], gestureAction: "拖动" },
    ],
  },
  {
    id: "sticker-edit",
    label: "贴图编辑",
    icon: "transform",
    items: [
      { id: "toggle-sticker-toolbar", sourceId: "toggle_sticker_toolbar", label: "贴图工具栏", description: "显示或隐藏贴图编辑工具栏", keys: ["Ctrl+E"], contexts: ALL_HOOK_SHORTCUT_CONTEXTS },
      { id: "control-copy", sourceId: "copy_unit", label: "复制控件", description: "复制贴图内当前选中的控件", keys: ["Ctrl+C"], contexts: ["sticker-editing"] },
      { id: "control-paste", sourceId: "paste_unit", label: "粘贴控件", description: "粘贴已复制的贴图控件", keys: ["Ctrl+V"], contexts: ["sticker-editing"] },
      { id: "control-delete", sourceId: "delete_unit", label: "删除控件", description: "删除贴图内当前选中的控件", keys: ["Escape", "Delete", "Backspace"], contexts: ["sticker-editing"], conflictFamily: "contextual-cancel-delete" },
      { id: "undo-edit", sourceId: "undo_edit", label: "撤销编辑", description: "撤销上一次控件编辑", keys: ["Ctrl+Z"], contexts: ["sticker-editing"] },
      { id: "redo-edit", sourceId: "redo_edit", label: "重做编辑", description: "恢复上一次撤销的控件编辑", keys: ["Ctrl+Y"], contexts: ["sticker-editing"] },
      { id: "transform-select", sourceId: "transform_select", label: "选择模式", description: "切换到控件选择模式", keys: ["Q"], contexts: ["unit-selected", "sticker-editing"] },
      { id: "transform-move", sourceId: "transform_move", label: "移动模式", description: "切换到控件移动模式", keys: ["W"], contexts: ["unit-selected", "sticker-editing"] },
      { id: "transform-rotate", sourceId: "transform_rotate", label: "旋转模式", description: "切换到控件旋转模式", keys: ["E"], contexts: ["unit-selected", "sticker-editing"] },
      { id: "transform-scale", sourceId: "transform_scale", label: "缩放模式", description: "切换到控件缩放模式", keys: ["R"], contexts: ["unit-selected", "sticker-editing"] },
      { id: "control-multi-select", sourceId: "control_multi_select", label: "多选控件", description: "添加或移除当前控件选择", keys: ["Shift+点击"], contexts: ["sticker-editing"], gestureAction: "点击" },
      { id: "control-quick-move", sourceId: "control_quick_move", label: "快速移动控件", description: "在选择模式下直接移动控件", keys: ["Alt+拖动"], contexts: ["sticker-editing"], gestureAction: "拖动" },
      { id: "control-quick-rotate", sourceId: "control_quick_rotate", label: "快速旋转控件", description: "在选择模式下直接旋转控件", keys: ["Ctrl+拖动"], contexts: ["sticker-editing"], gestureAction: "拖动" },
      { id: "control-scale", sourceId: "control_scale", label: "缩放选中控件", description: "以选中控件组的中心进行缩放", keys: ["Ctrl+Alt+滚轮"], contexts: ["sticker-editing"], gestureAction: "滚轮" },
      { id: "control-scale-own-center", sourceId: "control_scale_own_center", label: "独立中心缩放", description: "分别以每个控件自身中心缩放", keys: ["Ctrl+Alt+Shift+滚轮"], contexts: ["sticker-editing"], gestureAction: "滚轮" },
    ],
  },
];

export function HookShortcutGroupIcon({ kind }: { kind: HookShortcutGroupIconKind }) {
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

export const shortcutKeyDisplayLabel = (key: string) => key === "Escape" ? "Esc" : key;

export const SHORTCUT_GESTURE_MODIFIERS = ["Ctrl", "Alt", "Shift", "Meta"] as const;
export type ShortcutGestureModifier = typeof SHORTCUT_GESTURE_MODIFIERS[number];

export const gestureShortcutModifiers = (shortcut: string, action: HookShortcutGestureAction) => new Set(
  shortcutKeyParts(shortcut).filter((part): part is ShortcutGestureModifier => (
    part !== action && SHORTCUT_GESTURE_MODIFIERS.includes(part as ShortcutGestureModifier)
  )),
);

export const toggleGestureShortcutModifier = (
  shortcut: string,
  action: HookShortcutGestureAction,
  modifier: ShortcutGestureModifier,
) => {
  const selected = gestureShortcutModifiers(shortcut, action);
  if (selected.has(modifier)) selected.delete(modifier);
  else selected.add(modifier);
  return [
    ...SHORTCUT_GESTURE_MODIFIERS.filter((candidate) => selected.has(candidate)),
    action,
  ].join("+");
};

export const shortcutContextsOverlap = (
  left: readonly HookShortcutContext[],
  right: readonly HookShortcutContext[],
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
  return (
    <span className="hook-shortcut-key-sequences">
      {shortcuts.map((shortcut, shortcutIndex) => (
        <span className="hook-shortcut-key-sequence" key={`${shortcut}-${shortcutIndex}`}>
          {shortcutKeyParts(shortcut).map((key, keyIndex) => (
            <span className="hook-shortcut-key-part" key={`${key}-${keyIndex}`}>
              {keyIndex > 0 ? <span className="hook-shortcut-key-plus" aria-hidden="true">+</span> : null}
              <kbd>{shortcutKeyDisplayLabel(key)}</kbd>
            </span>
          ))}
          {shortcutIndex < shortcuts.length - 1 ? <span className="hook-shortcut-key-or">或</span> : null}
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
    appName: "Hook",
    version: "0.1.7",
    repositoryUrl: "https://github.com/aiaimimi0920/Hook",
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
    <section className={open ? "settings-section settings-section--open" : "settings-section"}>
      <h2 className="settings-section__heading">
        <button
          className="settings-section__trigger"
          type="button"
          aria-expanded={open}
          aria-controls={contentId}
          onClick={onToggle}
        >
          <SettingsSectionIcon kind={id} />
          <span>{label}</span>
          <svg className="settings-section__chevron" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden="true">
            <path d="m7 9 5 5 5-5" />
          </svg>
        </button>
      </h2>
      {open ? <div className="settings-section__body" id={contentId}>{children}</div> : null}
    </section>
  );
}
