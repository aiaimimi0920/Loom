import assert from "node:assert/strict";
import test from "node:test";
import { DEFAULT_LOOM_SETTINGS } from "../../services/loomApi/defaults.ts";
import { hydrateSettings } from "./settingsHydration.ts";

test("settings hydration preserves explicit values and uses the supplied shortcut snapshot", () => {
  const loaded = structuredClone(DEFAULT_LOOM_SETTINGS);
  loaded.general.language = "zh-Hans";
  loaded.general.minimize_to_tray = false;
  const result = hydrateSettings(loaded, []);
  assert.equal(result.general.language, "zh-Hans");
  assert.equal(result.general.minimize_to_tray, false);
  assert.deepEqual(result.shortcuts, {});
  assert.deepEqual(loaded.shortcuts, DEFAULT_LOOM_SETTINGS.shortcuts);
});
