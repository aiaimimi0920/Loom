import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const source = (name: string) => readFileSync(new URL(`../components/settings/${name}`, import.meta.url), "utf8");

test("Otter language uses a device-scoped owner while Loom keeps its existing language setting", () => {
  const sections = source("SettingsPanels.tsx");
  assert.match(sections, /appName === "loom" && <div className="settings-network-row">/);
  assert.match(source("SettingsPanel.tsx"), /<OtterLanguageSettings baseUrl=\{snapshot\.baseUrl\}/);
  assert.doesNotMatch(source("useSettingsPanelController.ts"), /patch\.language/);
  assert.match(source("LoomSettingsSections.tsx"), /patch\.language/);
});

test("language controls use shared primitives and show actual rather than optimistic state", () => {
  const panel = source("OtterLanguageSettings.tsx");
  assert.match(panel, /import \{ Button, Select \} from "@neuro\/ui"/);
  assert.match(panel, /value=\{locale\?\.language \?\? ""\}/);
  assert.match(panel, /locale\?\.supportedLanguages\.map/);
  assert.match(panel, /aria-describedby="otter-language-status"/);
  assert.match(panel, /role="status" aria-live="polite"/);
  assert.doesNotMatch(panel, /hook_general|localStorage/);
});

test("language polling is active-only, non-overlapping, and discards stale identities and reads", () => {
  const controller = source("useOtterLanguageSettings.ts");
  assert.match(controller, /!active \|\| !online \|\| refreshInFlight\.current === version/);
  assert.match(controller, /version !== generation\.current \|\| readReceiptVersion !== receiptVersion\.current/);
  assert.match(controller, /window\.clearInterval\(timer\)/);
  assert.match(controller, /setTracked\(current => current\?\.requestId === request\.requestId/);
  assert.match(controller, /expectedRevision: locale\.revision/);
  assert.equal((controller.match(/await requestOtterLocale\(/g) ?? []).length, 1);
});
