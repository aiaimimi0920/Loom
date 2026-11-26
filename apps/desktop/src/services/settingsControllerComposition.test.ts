import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const read = (name: string) => readFileSync(new URL(`../components/settings/${name}.ts`, import.meta.url), "utf8");
const controller = read("useSettingsPanelController");
const cache = read("useSettingsCacheControls");
const shortcuts = read("useSettingsShortcutEditors");

test("settings persistence remains the owner of hydration, ordered writes and runtime application", () => {
  assert.match(controller, /useSettingsCacheControls\(settingsMountedRef\)/);
  assert.match(
    controller,
    /useSettingsShortcutEditors\(\{[\s\S]*?tools: snapshot.tools,[\s\S]*?updateShortcutDraft,[\s\S]*?setDraft,/,
  );
  assert.match(controller, /while \(pendingSettingsRef.current\)/);
  assert.match(controller, /await saveLoomSettings\(baseUrl, nextSettings\)/);
  assert.match(controller, /settingsGenerationRef.current !== generation/);
  assert.match(controller, /settingsGenerationRef.current === generation/);
  assert.match(controller, /if \(connectionSwitchingRef.current\) return/);
  assert.match(controller, /if \(!settingsHydratedRef.current \|\| connectionSwitchingRef.current\) return/);
  assert.match(controller, /draftRef.current = next;\s*pendingSettingsRef.current = next;\s*setDraftState\(next\)/);
  assert.doesNotMatch(controller, /\[draft, flushSettingsQueue\]/);
  assert.match(controller, /draftRef.current = rollback/);
  assert.match(controller, /shortcutsRef.current = rollbackShortcuts/);
  assert.match(controller, /void flushSettingsQueue\(\);[\s\S]*?\}, 360\)/);
  assert.match(controller, /apply_loom_general_settings/);
  assert.match(controller, /apply_loom_cache_settings/);
  assert.match(controller, /wait_for_otter_cache_settings/);
});

test("shortcut editor composition delegates persistence and keeps domain capture/conflict rules", () => {
  assert.match(shortcuts, /updateShortcutDraft\(shortcutEditor.item.sourceId, shortcutEditor.item.label, keys\)/);
  assert.match(shortcuts, /currentLanguage\(\)/);
  assert.match(shortcuts, /event.preventDefault\(\)/);
  assert.match(shortcuts, /event.stopPropagation\(\)/);
  assert.match(
    shortcuts,
    /event.key === "Escape" && !event.ctrlKey && !event.altKey && !event.shiftKey && !event.metaKey/,
  );
  assert.match(shortcuts, /shortcutContextsOverlap\(candidateContexts, item.contexts\)/);
  assert.match(shortcuts, /availableArtToolById.has\(binding.art\)/);
  assert.match(shortcuts, /candidateContexts.includes\("unit-selected"\)/);
  assert.doesNotMatch(shortcuts, /invoke\(|saveLoomSettings|setTimeout|addEventListener|from "@neuro\/ui"/);
});

test("cache commands retain confirmations, payloads and application ownership", () => {
  assert.match(cache, /requestAppConfirmation\(/);
  assert.match(cache, /settings\.cache\.confirmClearArtRuntimeBody/);
  assert.match(cache, /settings\.cache\.confirmClearReferenceBody/);
  assert.match(cache, /invoke<LoomCacheClearResult>\("clear_loom_cache", \{ kind \}\)/);
  assert.match(cache, /invoke<OtterCacheClearResult>\("clear_otter_cache", \{ kind \}\)/);
  assert.equal((cache.match(/if \(!accepted \|\| !settingsMountedRef.current\) return/g) ?? []).length, 2);
  assert.doesNotMatch(cache, /saveLoomSettings|from "@neuro\/ui"/);
});

test("cache follow-up reads are bounded by the settings owner's lifetime", () => {
  assert.match(cache, /for \(const timer of refreshTimers.current\) window.clearTimeout\(timer\)/);
  assert.match(cache, /refreshTimers.current.clear\(\)/);
  assert.match(cache, /kind !== "temporary" && settingsMountedRef.current/);
  assert.match(cache, /refreshTimers.current.delete\(timer\);\s*void refreshOtterCache\(\)/);
  assert.match(cache, /\}, 500\);\s*refreshTimers.current.add\(timer\)/);
});
