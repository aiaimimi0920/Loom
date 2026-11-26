// Merge persisted settings with product defaults without applying them during loading.
import { DEFAULT_LOOM_SETTINGS } from "../../services/loomApi/defaults.ts";
import type { LoomSettings, LoomShortcutConfig } from "../../services/loomApi.ts";

export function hydrateSettings(
  loaded: LoomSettings,
  shortcuts: LoomShortcutConfig[],
): LoomSettings {
  return {
    ...loaded,
    general: { ...DEFAULT_LOOM_SETTINGS.general, ...loaded.general },
    hook_general: { ...DEFAULT_LOOM_SETTINGS.hook_general, ...loaded.hook_general },
    system: { ...DEFAULT_LOOM_SETTINGS.system, ...loaded.system },
    network: {
      loom: { ...DEFAULT_LOOM_SETTINGS.network.loom, ...loaded.network?.loom },
      hook: { ...DEFAULT_LOOM_SETTINGS.network.hook, ...loaded.network?.hook },
    },
    mcp: { ...DEFAULT_LOOM_SETTINGS.mcp, ...loaded.mcp },
    art_store: { ...DEFAULT_LOOM_SETTINGS.art_store, ...loaded.art_store },
    shortcuts: Object.fromEntries(shortcuts.map((shortcut) => [shortcut.id, shortcut])),
  };
}
