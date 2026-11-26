import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { appSource, styleSource } from "./artHubUiContractSource.ts";

const uiStyles = readFileSync(new URL(import.meta.resolve("@neuro/ui/styles.css")), "utf8");

test("renders Loom and Otter as attached application settings tabs", () => {
  assert.match(appSource, /type SettingsAppId = "loom" \| "hook"/);
  assert.match(
    appSource,
    /type SettingsSectionId = "general" \| "shortcuts" \| "mcp" \| "art-store" \| "capabilities" \| "network" \| "cache" \| "about"/,
  );
  assert.match(appSource, /function SettingsAccordionSection\(/);
  assert.match(appSource, /<Disclosure className="settings-section" open=\{open\} onOpenChange=\{onToggle\}/);
  assert.match(appSource, /open \? <div className="settings-section__body" id=\{contentId\}/);
  assert.match(appSource, /<TabsRoot asChild value=\{activeSettingsApp\}/);
  assert.match(appSource, /<TabList className="settings-app-tabs" label=\{t\("settings\.page\.appTabsAria"\)\}/);
  assert.match(appSource, /<Tab value="loom" onClick=\{\(\) => selectSettingsApp\("loom"\)\}/);
  assert.match(appSource, /<Tab value="hook" onClick=\{\(\) => selectSettingsApp\("hook"\)\}/);
  assert.match(appSource, /<TabPanel className="settings-app-panel" value="loom"/);
  assert.match(appSource, /<TabPanel className="settings-app-panel" value="hook"/);
  assert.match(appSource, /<SettingsAccordionSection id="general" label=\{t\("settings\.sections\.general"\)\}/);
  assert.equal(
    (appSource.match(/<SettingsAccordionSection id="about" label=\{t\("settings\.sections\.about"\)\}/g) || []).length,
    2,
  );
  assert.match(appSource, /<AboutPanel[\s\S]*?app="loom"/);
  assert.match(appSource, /<AboutPanel[\s\S]*?app="hook"/);
  assert.match(appSource, /<SettingsAccordionSection id="shortcuts" label=\{t\("settings\.sections\.shortcuts"\)\}/);
  assert.doesNotMatch(appSource, /<SettingsAccordionSection id="bindings"/);
  assert.match(appSource, /activeSection === "settings"[\s\S]*?app-titlebar__back/);
  assert.doesNotMatch(appSource, /settings-subnav|legacy-settings-grid|settings-card--wide|settings-page__save/);
  assert.match(
    styleSource,
    /\.workspace-panel,\s*\.workspace-scroll\s*\{[^}]*background: var\(--loom-theme-surface\);/,
  );
  assert.match(styleSource, /\.settings-app-panel \{[\s\S]*?min-width: 0;/);
  assert.doesNotMatch(
    styleSource,
    /\.settings-app-tab--active|\.settings-section__trigger|\.otter-shortcut-group__trigger/,
  );
  assert.match(styleSource, /\.settings-section__heading,[\s\S]*?display: flex;/);
  assert.match(styleSource, /\.settings-section__icon,[\s\S]*?color: var\(--loom-theme-secondary-text\);/);
});

test("shows compact About and diagnostic log content for Loom and Otter", () => {
  assert.match(appSource, /<dt>\{t\("settings\.about\.appName"\)\}<\/dt>/);
  assert.match(appSource, /<dt>\{t\("settings\.about\.version"\)\}<\/dt>/);
  assert.match(appSource, /<dt>\{t\("settings\.about\.checkUpdates"\)\}<\/dt>/);
  assert.match(appSource, />\{t\("settings\.about\.checkNow"\)\}<\/Button>/);
  assert.match(appSource, /\$\{repositoryUrl\}\/releases\/latest/);
  assert.match(appSource, /<dt>\{t\("settings\.about\.repository"\)\}<\/dt>/);
  assert.match(appSource, /diagnostics\.commitShort\?\.slice\(0, 6\)/);
  assert.match(appSource, /open_external_url/);
  assert.match(appSource, /(?:^|["'`])https:\/\/github\.com\/aiaimimi0920\/Otter(?:["'`]|$)/);
  assert.match(appSource, /M250 394V250h144/);
  assert.match(appSource, /M774 630v144H630/);
  assert.match(appSource, /<h3>\{t\("settings\.about\.diagnosticsHeader"\)\}<\/h3>/);
  assert.match(appSource, /<dt>\{t\("settings\.about\.logLevel"\)\}<\/dt>/);
  assert.match(appSource, /<dt>\{t\("settings\.about\.logLocation"\)\}<\/dt>/);
  assert.match(appSource, /<dt>\{t\("settings\.about\.viewLog"\)\}<\/dt>/);
  assert.match(appSource, /resolve_application_diagnostics/);
  assert.match(appSource, /open_application_log_location/);
  assert.match(appSource, /loom_log_level/);
  assert.match(appSource, /hook_log_level/);
  assert.doesNotMatch(appSource, /Telegram|\u8d5e\u52a9/);
  assert.match(styleSource, /\.about-panel__group \{[\s\S]*?border: 0;[\s\S]*?background: transparent;/);
  assert.match(styleSource, /\.about-panel__rows > div \{[\s\S]*?grid-template-columns:/);
  assert.match(styleSource, /\.about-panel__commit \{[\s\S]*?color: var\(--loom-theme-accent-text\);/);
  assert.match(appSource, /<Button compact variant="quiet"[\s\S]*?className="about-panel__repository-link"/);
  assert.match(uiStyles, /\.n-input\s*\{[^}]*background: var\(--n-control\);[^}]*color: var\(--n-text\);/);
  assert.match(appSource, /<Select[\s\S]*?className="about-panel__log-level"/);
  assert.match(styleSource, /\.workspace-scroll--settings \{[\s\S]*?scrollbar-gutter: stable;/);
});

test("shared settings use standard controls without inheriting legacy skins", () => {
  const source = (name: string) => readFileSync(new URL(`../components/settings/${name}.tsx`, import.meta.url), "utf8");
  const panels = source("SettingsPanels"),
    about = source("AboutPanel");
  assert.doesNotMatch(panels + about, /<(?:button|input|select)\b|studio-input|ghost-button/);
  assert.equal((panels.match(/<Switch\b/g) || []).length, 3);
  assert.equal((panels.match(/<Select\b/g) || []).length, 14);
  assert.equal((panels.match(/<Button compact loading=\{busyKind ===/g) || []).length, 5);
  assert.equal((panels.match(/disabled=\{Boolean\(busyKind\)\}/g) || []).length, 5);
  assert.equal((panels.match(/: Number\(event\.target\.value\)/g) || []).length, 9);
  assert.match(
    panels,
    /<Switch[^>]*aria-label=\{t\("settings\.artStore\.officialOnlyAria"\)\}[\s\S]*?checked=\{value\.official_only\}/,
  );
  assert.match(panels, /disabled=\{trustPolicyBusy\}/);
  assert.match(about, /disabled=\{!diagnostics\.logFileExists\}/);
  const foundation = readFileSync(new URL("../styles/foundation.css", import.meta.url), "utf8");
  assert.doesNotMatch(foundation, /studio-input|:is\([^)]*\binput\b|:not\([^)]*\.n-input/);
  assert.match(foundation, /\.neuro-root\s*\{[^}]*--loom-theme-control: var\(--n-control\);/);
});

test("settings navigation and shortcut dialogs use the standard runtime without replacing key capture", () => {
  const source = (name: string) => readFileSync(new URL(`../components/settings/${name}.tsx`, import.meta.url), "utf8");
  const panel = source("SettingsPanel"),
    dialogs = source("SettingsShortcutDialogs"),
    groups = source("OtterShortcutSettings"),
    slots = source("ShortcutSlotsEditor");
  assert.doesNotMatch(panel + dialogs + groups, /<(?:button|input|select)\b|createPortal|framework-dialog|autoFocus/);
  assert.equal((dialogs.match(/<Dialog\b/g) || []).length, 2);
  assert.equal((slots.match(/<button\b/g) || []).length, 1);
  assert.match(slots, /onKeyDown=\{event => onCapture\(event, slot\)\}/);
  assert.match(slots, /<Checkbox key=\{modifier\}/);
  assert.match(dialogs, /event\.target\.closest\("\.shortcut-capture-field"\)/);
  assert.match(dialogs, /onEscapeKeyDown=\{captureEscape\}/);
  assert.doesNotMatch(slots, /autoFocus|addEventListener/);
});

test("uses the dark Settings visual baseline for Art and Otter Sync without changing their content", () => {
  assert.match(appSource, /workspace-panel workspace-panel--tooling/);
  assert.match(appSource, /workspace-header workspace-header--tooling/);
  assert.match(appSource, /workspace-scroll workspace-scroll--tooling/);
  assert.match(uiStyles, /\.neuro-root\s*\{[^}]*--n-panel: #0e1218;/);
  assert.match(styleSource, /\.neuro-root\s*\{[^}]*--loom-theme-panel: var\(--n-panel\);/);
  assert.match(styleSource, /\.workspace-panel--tooling,[\s\S]*?var\(--loom-theme-surface\)/);
  assert.match(styleSource, /\.art-hub__surface \{[\s\S]*?border: 0;[\s\S]*?background: transparent;/);
  assert.doesNotMatch(styleSource, /\.art-(?:registry|store)-card(?:--(?:enabled|disabled))?\s*\{/);
  assert.match(uiStyles, /\.n-dialog\s*\{[^}]*background: var\(--n-panel\)/);
  const renameSource = readFileSync(
    new URL("../components/otter/OtterCanvasRenameDialog.tsx", import.meta.url),
    "utf8",
  );
  assert.match(renameSource, /<Dialog\b[\s\S]*?\btitle=/);
  assert.doesNotMatch(styleSource, /\.otter-canvas-rename-(?:dialog|backdrop)/);
  assert.match(styleSource, /\.otter-canvas-thumbnail \{[^}]*display: grid;[^}]*min-width: 0;/);
  assert.match(styleSource, /\.otter-canvas-surface \{[\s\S]*?overflow: hidden;/);
});

test("provides independent proxy settings for Loom and Otter", () => {
  assert.equal(
    (appSource.match(/<SettingsAccordionSection id="network" label=\{t\("settings\.sections\.network"\)\}/g) || [])
      .length,
    2,
  );
  assert.match(appSource, /function NetworkSettingsPanel\(/);
  assert.match(appSource, /appName="Loom"[\s\S]*?value=\{draft\.network\.loom\}/);
  assert.match(appSource, /appName="Otter"[\s\S]*?value=\{draft\.network\.hook\}/);
  assert.match(appSource, /<option value="system">\{t\("settings\.network\.proxySystem"\)\}<\/option>/);
  assert.match(appSource, /<option value="custom">\{t\("settings\.network\.proxyCustom"\)\}<\/option>/);
  assert.match(appSource, /<option value="disabled">\{t\("settings\.network\.proxyDisabled"\)\}<\/option>/);
  assert.match(appSource, /value\.mode === "custom"/);
  assert.match(appSource, /<option value="http">http:\/\/<\/option>/);
  assert.match(appSource, /<option value="https">https:\/\/<\/option>/);
  assert.match(appSource, /<option value="socks5">socks5:\/\/<\/option>/);
  assert.match(appSource, /placeholder="127\.0\.0\.1:7890"/);
  assert.match(appSource, /const updateNetworkDraft[\s\S]*?\[app\]: \{ \.\.\.current\.network\[app\], \.\.\.patch \}/);
  assert.match(
    styleSource,
    /\.settings-network-panel,[\s\S]*?\.settings-general-panel,[\s\S]*?\.settings-mcp-panel,[\s\S]*?\.settings-art-store-panel \{[\s\S]*?border: 0;[\s\S]*?background: transparent;/,
  );
  assert.match(styleSource, /\.settings-network-row \{[\s\S]*?grid-template-columns:/);
});

test("provides independent compact general settings for Loom and Otter", () => {
  assert.equal(
    (appSource.match(/<SettingsAccordionSection id="general" label=\{t\("settings\.sections\.general"\)\}/g) || [])
      .length,
    2,
  );
  assert.match(appSource, /function GeneralSettingsPanel\(/);
  assert.match(
    appSource,
    /appName="loom"[\s\S]*?language: draft\.general\.language[\s\S]*?theme: draft\.general\.theme[\s\S]*?closeToTray: draft\.general\.minimize_to_tray/,
  );
  assert.match(appSource, /DISPLAY_LANGUAGES\.map/);
  assert.match(appSource, /displayLanguageName\(lang\)/);
  assert.match(appSource, /parsePersistedDisplayLanguage\(value\.language\)/);
  assert.match(appSource, /setLanguage\(nextLanguage\)/);
  assert.match(
    appSource,
    /<OtterLanguageSettings[\s\S]*?appName="hook"[\s\S]*?theme: draft\.hook_general\.theme[\s\S]*?closeToTray: draft\.hook_general\.close_to_tray/,
  );
  assert.match(
    appSource,
    /<strong>\{t\("settings\.general\.languageTitle"\)\}<\/strong>[\s\S]*?<strong>\{t\("settings\.general\.themeTitle"\)\}<\/strong>[\s\S]*?<strong>\{t\("settings\.general\.closeToTrayTitle"\)\}<\/strong>/,
  );
  assert.match(appSource, /const updateOtterGeneralDraft[\s\S]*?hook_general:/);
  assert.doesNotMatch(appSource, /SettingsAccordionSection id="window"/);
  assert.match(
    styleSource,
    /\.settings-network-panel,[\s\S]*?\.settings-general-panel,[\s\S]*?\.settings-mcp-panel,[\s\S]*?\.settings-art-store-panel \{[\s\S]*?border: 0;[\s\S]*?background: transparent;/,
  );
  assert.match(styleSource, /\.settings-general-toggle \{[\s\S]*?justify-self: end;/);
  assert.match(appSource, /applyLoomGeneralSettings\(draft\.general\)/);
  assert.match(appSource, /invoke\("apply_loom_general_settings"/);
  assert.match(styleSource, /:root\[data-loom-theme="light"\]/);
});

test("replaces System and Data with runtime-backed MCP and Art settings", () => {
  assert.doesNotMatch(appSource, /SettingsAccordionSection id="system"|\u7cfb\u7edf\u4e0e\u6570\u636e/);
  assert.doesNotMatch(appSource, /toggleAutostart|setArtLoomCompatAutostart/);
  assert.match(appSource, /SettingsAccordionSection id="mcp" label=\{t\("settings\.sections\.mcp"\)\}/);
  assert.match(appSource, /function McpSettingsPanel\(/);
  assert.match(appSource, /settings\.mcp\.timeoutAria/);
  assert.match(appSource, /settings\.mcp\.memoryLimitAria/);
  assert.match(appSource, /SettingsAccordionSection id="art-store" label=\{t\("settings\.sections\.artStore"\)\}/);
  assert.match(appSource, /function ArtStoreSettingsPanel\(/);
  assert.doesNotMatch(appSource, /\u5546\u5e97\u5730\u5740|settings-art-store-url|value\.base_url/);
  assert.match(appSource, /settings\.artStore\.autoUpdateAria/);
  assert.match(appSource, /settings\.artStore\.officialOnlyAria/);
  assert.match(appSource, /settings\.artStore\.trustPolicyAria/);
  assert.match(appSource, /setPluginTrustPolicy\(snapshot\.baseUrl, policy\)/);
  assert.match(appSource, /setStoreOfficialOnly\(settings\.art_store\?\.official_only === true\)/);
  assert.match(styleSource, /\.settings-mcp-panel/);
  assert.match(styleSource, /\.settings-art-store-panel/);
});

test("manages only rebuildable Loom caches and removes the obsolete engine UI", () => {
  assert.equal(
    (appSource.match(/<SettingsAccordionSection id="cache" label=\{t\("settings\.sections\.cache"\)\}/g) || []).length,
    2,
  );
  assert.doesNotMatch(appSource, /SettingsAccordionSection id="engine"|draft\.engine/);
  assert.match(appSource, /function LoomCacheSettingsPanel\(/);
  assert.match(appSource, /get_loom_cache_snapshot/);
  assert.match(appSource, /apply_loom_cache_settings/);
  assert.match(appSource, /clear_loom_cache/);
  assert.match(appSource, /loomCachePreferencesForRuntime\(saved\.loom_cache\)/);
  assert.match(appSource, /settings\.cache\.artMaxBytesTitle/);
  assert.match(appSource, /settings\.cache\.artRetentionTitle/);
  assert.match(appSource, /settings\.cache\.frameworkTempRetentionTitle/);
  assert.match(appSource, /settings\.cache\.confirmClearArtRuntimeBody/);
  assert.doesNotMatch(
    appSource,
    /\u6e05\u7a7a\u5df2\u5b89\u88c5 Art|\u6e05\u7a7a\u5de5\u4f5c\u6d41|\u6e05\u7a7a\u8fd0\u884c\u8bb0\u5f55/,
  );
});

test("manages Otter recycle bin, temporary cache, and reference images from the Otter settings tab", () => {
  assert.equal(
    (appSource.match(/<SettingsAccordionSection id="cache" label=\{t\("settings\.sections\.cache"\)\}/g) || []).length,
    2,
  );
  assert.match(appSource, /function OtterCacheSettingsPanel\(/);
  assert.match(appSource, /get_otter_cache_snapshot/);
  assert.match(appSource, /clear_otter_cache/);
  assert.match(appSource, /wait_for_otter_cache_settings/);
  assert.match(appSource, /otterCachePreferencesForRuntime\(saved\.hook_cache\)/);
  assert.match(appSource, /settings\.cache\.recycleBinMaxTitle/);
  assert.match(appSource, /settings\.cache\.recycleBinRetentionTitle/);
  assert.match(appSource, /settings\.cache\.tempMaxTitle/);
  assert.match(appSource, /settings\.cache\.tempRetentionTitle/);
  assert.match(appSource, /settings\.cache\.clearRecycleBinTitle/);
  assert.match(appSource, /settings\.cache\.clearTempTitle/);
  assert.match(appSource, /settings\.cache\.clearReferenceTitle/);
  assert.match(appSource, /\[15, 50, 0\]/);
  assert.equal((appSource.match(/\[3, 7, 30, 0\]/g) || []).length, 3);
  assert.match(appSource, /label: "128 MB"/);
  assert.match(appSource, /label: "256 MB"/);
  assert.match(appSource, /label: "1 GB"/);
  assert.match(appSource, /label: t\("settings\.cache\.unlimited"\)/);
  assert.doesNotMatch(appSource, /otter-cache-usage-title/);
  assert.equal((appSource.match(/otter-cache-row otter-cache-row--action/g) || []).length, 5);
  assert.doesNotMatch(appSource, /\u56fe\u7247\u641c\u7d22\u7f13\u5b58|\u526a\u8d34\u677f\u7f13\u5b58/);
  assert.match(
    appSource,
    /requestAppConfirmation\(\{[\s\S]*?title: uiFormat\("settings\.cache\.confirmClearTitle", \{ label: labels\[kind\] \}\)/,
  );
  assert.match(styleSource, /\.otter-cache-settings \{[\s\S]*?gap: 12px;/);
  assert.match(styleSource, /\.otter-cache-group \{[\s\S]*?border: 0;[\s\S]*?background: transparent;/);
  assert.match(styleSource, /\.otter-cache-row b \{[^}]*color: var\(--loom-theme-text\);/);
});

test("auto-saves Loom fields and Otter shortcuts without a manual save action", () => {
  assert.match(appSource, /draftRef\.current = next;\s*pendingSettingsRef\.current = next/);
  assert.match(appSource, /window\.setTimeout\(\(\) => \{[\s\S]*?void flushSettingsQueue\(\);[\s\S]*?\}, 360\)/);
  assert.match(appSource, /while \(pendingSettingsRef\.current\)/);
  assert.match(appSource, /await saveLoomSettings\(baseUrl, nextSettings\)/);
  assert.match(appSource, /const updateShortcutDraft[\s\S]*?shortcuts: Object\.fromEntries\(nextShortcuts\.map/);
  assert.doesNotMatch(appSource, /saveSettingsDraft|saveShortcutDraft|\u4fdd\u5b58\u517c\u5bb9\u8bbe\u7f6e/);
});

test("groups, collapses, and edits Otter shortcuts without nested scrolling", () => {
  assert.match(appSource, /shortcutGroup\("capture-file"/);
  assert.match(appSource, /shortcutGroup\("panels-tools"/);
  assert.match(appSource, /shortcutGroup\("sticker-operation"/);
  assert.match(appSource, /shortcutGroup\("sticker-edit"/);
  assert.match(appSource, /shortcutItem\("toggle-sticker-toolbar"[\s\S]*?shortcutItem\("control-copy"/);
  assert.match(appSource, /shortcutItem\("transform-select"/);
  assert.match(appSource, /shortcutItem\("transform-move"/);
  assert.match(appSource, /shortcutItem\("transform-rotate"/);
  assert.match(appSource, /shortcutItem\("transform-scale"/);
  assert.match(appSource, /<Disclosure key=\{group\.id\} className="otter-shortcut-group" open=\{open\}/);
  assert.match(appSource, /function ShortcutKeySequence\(/);
  assert.match(appSource, /<Dialog\s+title=\{t\("settings\.shortcuts\.editDialogTitle"\)\}/);
  assert.match(appSource, /type ShortcutSlot = 0 \| 1 \| 2/);
  assert.match(appSource, /keys: \[keys\[0\] \|\| "", keys\[1\] \|\| "", keys\[2\] \|\| ""\]/);
  assert.match(appSource, /value\.slotCount < 3[\s\S]*?settings\.shortcuts\.addSecondary/);
  assert.match(appSource, /slotCount: \(value\.slotCount \+ 1\) as 2 \| 3/);
  assert.match(appSource, /removeShortcutSlot\(value\.keys, slot\)/);
  assert.match(appSource, /onCapture=\{handleShortcutCapture\}/);
  assert.match(appSource, /onKeyDown=\{event => onCapture\(event, slot\)\}/);
  assert.match(appSource, /updateShortcutDraft\(shortcutEditor\.item\.sourceId, shortcutEditor\.item\.label, keys\)/);
  assert.match(appSource, /settings\.shortcuts\.duplicateInAction/);
  assert.match(appSource, /shortcutContextsOverlap\(candidateContexts, item\.contexts\)/);
  assert.match(appSource, /conflictFamily: "contextual-cancel-delete"/);
  assert.equal((appSource.match(/keys,?\s*\["Escape", "Delete", "Backspace"\]|\["Escape", "Delete", "Backspace"\]/g) || []).length, 3);
  assert.match(appSource, /shortcutItem\("force-close", \["Esc × 3"\]/);
  assert.match(appSource, /\["unit-selected"\][\s\S]*?\["sticker-editing"\]/);
  assert.match(
    appSource,
    /shortcutItem\("control-quick-move", \["Alt\+Drag"\], \["sticker-editing"\], \{ sourceId: "control_quick_move", gestureAction: "drag" \}\)/,
  );
  assert.match(appSource, /gestureAction: "drag"/);
  assert.match(appSource, /shortcut-gesture-picker/);
  assert.match(appSource, /toggleGestureShortcutModifier/);
  assert.match(appSource, /settings\.shortcuts\.addArtShortcut/);
  assert.match(appSource, /<Dialog\s+title=\{t\("settings\.shortcuts\.artDialogTitle"\)\}/);
  assert.match(appSource, /<ShortcutSlotsEditor\s+value=\{quickBindingEditor\}/);
  assert.match(appSource, /availableArtTools\.map/);
  assert.match(appSource, /quick_bindings: current\.quick_bindings\.some/);
  assert.doesNotMatch(styleSource, /\.otter-shortcut-list \{[^}]*overflow-y:/);
  assert.match(styleSource, /\.shortcut-gesture-picker \{/);
  assert.match(styleSource, /\.shortcut-add-secondary \{/);
});
