import assert from "node:assert/strict";
import test from "node:test";

import { applyLoomGeneralSettings } from "./loomGeneralSettings.ts";
import { DEFAULT_LOOM_SETTINGS } from "./loomApi.ts";

test("uses dark as the product default while retaining explicit system support", () => {
  assert.equal(DEFAULT_LOOM_SETTINGS.appearance_version, 1);
  assert.equal(DEFAULT_LOOM_SETTINGS.general.theme, "dark");
  assert.equal(DEFAULT_LOOM_SETTINGS.hook_general.theme, "dark");
});

for (const [theme, expectedColorScheme] of [
  ["dark", "dark"],
  ["light", "light"],
  ["system", "light dark"],
] as const) {
  test(`applies the ${theme} Loom theme to the document root`, () => {
    const root = {
      lang: "",
      dataset: {} as Record<string, string>,
      style: { colorScheme: "" },
    };
    const documentRef = { documentElement: root } as unknown as Document;

    applyLoomGeneralSettings(
      { language: theme === "dark" ? "zh-Hans" : "en", theme, minimize_to_tray: false },
      documentRef,
    );

    assert.equal(root.lang, theme === "dark" ? "zh-Hans" : "en");
    assert.equal(root.dataset.loomTheme, theme);
    assert.equal(root.style.colorScheme, expectedColorScheme);
  });
}
