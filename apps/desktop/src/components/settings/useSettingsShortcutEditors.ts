// Owns shortcut editor drafts, contextual conflicts and capture; persistence stays in the settings controller.
import { useMemo, useState, type Dispatch, type SetStateAction, type KeyboardEvent } from "react";
import type { LoomSettings, LoomShortcutConfig, LoomToolDefinition } from "../../services/loomApi";
import { pushAppToast } from "../feedback/AppFeedback";
import {
  HOOK_SHORTCUT_GROUPS,
  shortcutContextsOverlap,
  shortcutFromKeyboardEvent,
  shortcutSlotCount,
  splitShortcutAlternatives,
  type OtterShortcutContext,
  type OtterShortcutDisplayItem,
  type QuickBindingEditorState,
  type ShortcutEditorState,
  type ShortcutSlot,
  type ShortcutSlots,
} from "./settingsModel";

interface SettingsShortcutEditorOptions {
  draft: LoomSettings;
  shortcuts: LoomShortcutConfig[];
  tools: LoomToolDefinition[];
  updateShortcutDraft: (id: string, label: string, keys: string) => void;
  setDraft: Dispatch<SetStateAction<LoomSettings>>;
}

export function useSettingsShortcutEditors({
  draft,
  shortcuts,
  tools,
  updateShortcutDraft,
  setDraft,
}: SettingsShortcutEditorOptions) {
  const [openShortcutGroups, setOpenShortcutGroups] = useState<Set<string>>(() => new Set());

  const [shortcutEditor, setShortcutEditor] = useState<ShortcutEditorState | null>(null);

  const [quickBindingEditor, setQuickBindingEditor] = useState<QuickBindingEditorState | null>(null);

  const availableArtTools = useMemo(
    () =>
      tools
        .filter((tool) => tool.enabled !== false)
        .sort((left, right) => (left.name || left.id).localeCompare(right.name || right.id, "zh-CN")),
    [tools],
  );

  const availableArtToolById = useMemo(
    () => new Map(availableArtTools.map((tool) => [tool.id, tool])),
    [availableArtTools],
  );

  const resolveShortcutKeys = (item: OtterShortcutDisplayItem) => {
    if (!item.sourceId) return item.keys;
    const configured = shortcuts.find((shortcut) => shortcut.id === item.sourceId)?.keys.trim();
    const normalizedConfigured = configured?.replace(/\s+/g, "").toLocaleLowerCase();
    const contextualDefaultOverride =
      (item.sourceId === "cancel" && normalizedConfigured === "escape") ||
      (item.sourceId === "delete_unit" && normalizedConfigured === "delete/backspace");
    if (contextualDefaultOverride) return item.keys;
    return configured ? splitShortcutAlternatives(configured) : item.keys;
  };

  const toggleShortcutGroup = (groupId: string) => {
    setOpenShortcutGroups((current) => {
      const next = new Set(current);
      if (next.has(groupId)) next.delete(groupId);
      else next.add(groupId);
      return next;
    });
  };

  const openShortcutEditor = (item: OtterShortcutDisplayItem) => {
    if (!item.sourceId) return;
    const keys = resolveShortcutKeys(item);
    setShortcutEditor({
      item,
      keys: [keys[0] || "", keys[1] || "", keys[2] || ""],
      activeSlot: 0,
      slotCount: shortcutSlotCount(keys),
    });
  };

  const handleShortcutCapture = (event: KeyboardEvent<HTMLButtonElement>, slot: ShortcutSlot) => {
    event.preventDefault();
    event.stopPropagation();
    if (event.key === "Escape" && !event.ctrlKey && !event.altKey && !event.shiftKey && !event.metaKey) {
      setShortcutEditor(null);
      return;
    }
    const keys = shortcutFromKeyboardEvent(event.nativeEvent);
    if (!keys) return;
    setShortcutEditor((current) =>
      current
        ? {
            ...current,
            activeSlot: slot,
            keys: current.keys.map((value, index) => (index === slot ? keys : value)) as ShortcutSlots,
          }
        : current,
    );
  };

  const shortcutConflictMessage = (
    candidateKeys: ShortcutSlots,
    candidateContexts: readonly OtterShortcutContext[],
    excludeSourceId?: string,
    excludeQuickBindingId?: string,
    candidateConflictFamily?: string,
  ) => {
    const normalized = candidateKeys.map((keys) => keys.trim().toLocaleLowerCase()).filter(Boolean);
    if (new Set(normalized).size !== normalized.length) return "同一事件的快捷键不能重复";
    const shortcutConflict = HOOK_SHORTCUT_GROUPS.flatMap((group) => group.items).find(
      (item) =>
        item.sourceId &&
        item.sourceId !== excludeSourceId &&
        (!candidateConflictFamily || item.conflictFamily !== candidateConflictFamily) &&
        shortcutContextsOverlap(candidateContexts, item.contexts) &&
        resolveShortcutKeys(item).some((keys) => normalized.includes(keys.toLocaleLowerCase())),
    );
    if (shortcutConflict) return `与“${shortcutConflict.label}”冲突`;
    const quickBindingConflict = draft.quick_bindings.find(
      (binding) =>
        binding.id !== excludeQuickBindingId &&
        availableArtToolById.has(binding.art) &&
        candidateContexts.includes("unit-selected") &&
        splitShortcutAlternatives(binding.key).some((keys) => normalized.includes(keys.toLocaleLowerCase())),
    );
    if (quickBindingConflict) {
      const art = availableArtToolById.get(quickBindingConflict.art);
      return `与“${art?.name || quickBindingConflict.art}”冲突`;
    }
    return null;
  };

  const shortcutEditorConflict = shortcutEditor
    ? shortcutConflictMessage(
        shortcutEditor.keys,
        HOOK_SHORTCUT_GROUPS.flatMap((group) => group.items)
          .filter((item) => item.sourceId === shortcutEditor.item.sourceId)
          .flatMap((item) => item.contexts)
          .filter((context, index, contexts) => contexts.indexOf(context) === index),
        shortcutEditor.item.sourceId,
        undefined,
        shortcutEditor.item.conflictFamily,
      )
    : null;

  const applyShortcutEditor = () => {
    if (!shortcutEditor?.item.sourceId || !shortcutEditor.keys.some((keys) => keys.trim()) || shortcutEditorConflict)
      return;
    const keys = shortcutEditor.keys
      .map((value) => value.trim())
      .filter(Boolean)
      .join(" / ");
    updateShortcutDraft(shortcutEditor.item.sourceId, shortcutEditor.item.label, keys);
    setShortcutEditor(null);
    pushAppToast({ level: "info", text: `${shortcutEditor.item.label}快捷键已更新` });
  };

  const openQuickBindingEditor = (binding?: LoomSettings["quick_bindings"][number]) => {
    const keys = binding ? splitShortcutAlternatives(binding.key) : [];
    setQuickBindingEditor({
      id: binding?.id || `${Date.now()}`,
      art: binding?.art || availableArtTools[0]?.id || "",
      keys: [keys[0] || "", keys[1] || "", keys[2] || ""],
      activeSlot: 0,
      slotCount: shortcutSlotCount(keys),
    });
  };

  const handleQuickBindingCapture = (event: KeyboardEvent<HTMLButtonElement>, slot: ShortcutSlot) => {
    event.preventDefault();
    event.stopPropagation();
    if (event.key === "Escape" && !event.ctrlKey && !event.altKey && !event.shiftKey && !event.metaKey) {
      setQuickBindingEditor(null);
      return;
    }
    const keys = shortcutFromKeyboardEvent(event.nativeEvent);
    if (!keys) return;
    setQuickBindingEditor((current) =>
      current
        ? {
            ...current,
            activeSlot: slot,
            keys: current.keys.map((value, index) => (index === slot ? keys : value)) as ShortcutSlots,
          }
        : current,
    );
  };

  const quickBindingConflict = quickBindingEditor
    ? shortcutConflictMessage(quickBindingEditor.keys, ["unit-selected"], undefined, quickBindingEditor.id)
    : null;

  const applyQuickBindingEditor = () => {
    if (!quickBindingEditor?.art || !quickBindingEditor.keys.some((keys) => keys.trim()) || quickBindingConflict)
      return;
    const nextBinding = {
      id: quickBindingEditor.id,
      art: quickBindingEditor.art,
      key: quickBindingEditor.keys
        .map((value) => value.trim())
        .filter(Boolean)
        .join(" / "),
    };
    setDraft((current) => ({
      ...current,
      quick_bindings: current.quick_bindings.some((binding) => binding.id === nextBinding.id)
        ? current.quick_bindings.map((binding) => (binding.id === nextBinding.id ? nextBinding : binding))
        : [...current.quick_bindings, nextBinding],
    }));
    setQuickBindingEditor(null);
    pushAppToast({ level: "info", text: "Art 快捷键已更新" });
  };
  return {
    openShortcutGroups,
    shortcutEditor,
    setShortcutEditor,
    quickBindingEditor,
    setQuickBindingEditor,
    availableArtTools,
    availableArtToolById,
    resolveShortcutKeys,
    toggleShortcutGroup,
    openShortcutEditor,
    handleShortcutCapture,
    shortcutEditorConflict,
    applyShortcutEditor,
    openQuickBindingEditor,
    handleQuickBindingCapture,
    quickBindingConflict,
    applyQuickBindingEditor,
  };
}
