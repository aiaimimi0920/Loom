import assert from "node:assert/strict";
import test from "node:test";
import {
  artDisplayIdentity,
  artDisplayLocale,
  artFrameworkReference,
  artPackageIdentity,
  artPublisherIconSource,
  artWorkspaceItems,
  filterArtStoreEntries,
  filterToolsByFrameworks,
  frameworkFilterLabel,
  isLocallyAuthoredTool,
  nextArtWorkspaceIndex,
  officialFrameworkDisplayName,
} from "./artHubUi.ts";
import type { LoomFramework, LoomToolDefinition } from "./loomApi.ts";
import { appSource, styleSource } from "./artHubUiContractSource.ts";

test("renders Loom and Hook as attached application settings tabs", () => {
  assert.match(appSource, /type SettingsAppId = "loom" \| "hook"/);
  assert.match(appSource, /type SettingsSectionId = "general" \| "shortcuts" \| "mcp" \| "art-store" \| "network" \| "cache" \| "about"/);
  assert.match(appSource, /function SettingsAccordionSection\(/);
  assert.match(appSource, /aria-expanded=\{open\}/);
  assert.match(appSource, /aria-controls=\{contentId\}/);
  assert.match(appSource, /className="settings-app-tabs" aria-label="应用设置" role="tablist"/);
  assert.match(appSource, /aria-selected=\{activeSettingsApp === "loom"\}/);
  assert.match(appSource, /aria-selected=\{activeSettingsApp === "hook"\}/);
  assert.match(appSource, /activeSettingsApp === "loom" \? \(/);
  assert.match(appSource, /<SettingsAccordionSection id="general" label="常规"/);
  assert.equal((appSource.match(/<SettingsAccordionSection id="about" label="关于"/g) || []).length, 2);
  assert.match(appSource, /<AboutPanel[\s\S]*?app="loom"/);
  assert.match(appSource, /<AboutPanel[\s\S]*?app="hook"/);
  assert.match(appSource, /<SettingsAccordionSection id="shortcuts" label="快捷键"/);
  assert.doesNotMatch(appSource, /<SettingsAccordionSection id="bindings" label="快速绑定"/);
  assert.match(appSource, /activeSection === "settings"[\s\S]*?app-titlebar__back/);
  assert.doesNotMatch(appSource, /settings-subnav|legacy-settings-grid|settings-card--wide|settings-page__save/);
  assert.match(styleSource, /\.workspace-panel--settings,[\s\S]*?var\(--loom-theme-surface\);/);
  assert.match(styleSource, /\.settings-app-panel \{[\s\S]*?border-top:/);
  assert.match(styleSource, /\.settings-app-tab--active::after \{[\s\S]*?background: var\(--loom-theme-surface\);/);
  assert.match(styleSource, /\.settings-section__trigger \{[\s\S]*?min-height: 66px;/);
  assert.match(styleSource, /\.settings-section__icon \{[\s\S]*?color: var\(--loom-theme-accent-text\);/);
  assert.match(styleSource, /\.settings-section--open \.settings-section__icon \{[\s\S]*?color: var\(--loom-theme-secondary-text\);/);
});

test("shows compact About and diagnostic log content for Loom and Hook", () => {
  assert.match(appSource, /<dt>应用名称<\/dt>/);
  assert.match(appSource, /<dt>版本号<\/dt>/);
  assert.match(appSource, /<dt>检查更新<\/dt>/);
  assert.match(appSource, />立即检查<\/button>/);
  assert.match(appSource, /\$\{repositoryUrl\}\/releases\/latest/);
  assert.match(appSource, /<dt>仓库<\/dt>/);
  assert.match(appSource, /diagnostics\.commitShort\?\.slice\(0, 6\)/);
  assert.match(appSource, /open_external_url/);
  assert.match(appSource, /https:\/\/github\.com\/aiaimimi0920\/Hook/);
  assert.match(appSource, /M250 394V250h144/);
  assert.match(appSource, /M774 630v144H630/);
  assert.match(appSource, /<h3>诊断日志<\/h3>/);
  assert.match(appSource, /<dt>日志级别<\/dt>/);
  assert.match(appSource, /<dt>日志位置<\/dt>/);
  assert.match(appSource, /<dt>查看日志<\/dt>/);
  assert.match(appSource, /resolve_application_diagnostics/);
  assert.match(appSource, /open_application_log_location/);
  assert.match(appSource, /loom_log_level/);
  assert.match(appSource, /hook_log_level/);
  assert.doesNotMatch(appSource, /Telegram 群组|赞助支持|赞助方式/);
  assert.match(styleSource, /\.about-panel__group \{[\s\S]*?border: 0;[\s\S]*?background: transparent;/);
  assert.match(styleSource, /\.about-panel__rows > div \{[\s\S]*?grid-template-columns:/);
  assert.match(styleSource, /\.about-panel__commit \{[\s\S]*?color: var\(--loom-theme-accent-text\);/);
  assert.match(styleSource, /\.about-panel__repository-link \{[\s\S]*?color: var\(--loom-theme-secondary-text\);/);
  assert.match(styleSource, /\.settings-page \.studio-input option \{[\s\S]*?background: var\(--loom-theme-control\);[\s\S]*?color: var\(--loom-theme-text\);/);
  assert.match(styleSource, /\.about-panel__log-level \{[\s\S]*?background: var\(--loom-theme-control\);[\s\S]*?-webkit-text-fill-color: var\(--loom-theme-text\);/);
  assert.match(styleSource, /\.workspace-scroll--settings \{[\s\S]*?scrollbar-gutter: stable;/);
});

test("uses the dark Settings visual baseline for Art and Hook Sync without changing their content", () => {
  assert.match(appSource, /workspace-panel workspace-panel--tooling/);
  assert.match(appSource, /workspace-header workspace-header--tooling/);
  assert.match(appSource, /workspace-scroll workspace-scroll--tooling/);
  assert.match(styleSource, /:root \{[\s\S]*?--neuro-panel: #0e1218;/);
  assert.match(styleSource, /:root \{[\s\S]*?--loom-theme-panel: var\(--neuro-panel\);/);
  assert.match(styleSource, /\.workspace-panel--tooling,[\s\S]*?var\(--loom-theme-surface\)/);
  assert.match(styleSource, /\.art-hub__surface \{[\s\S]*?border: 0;[\s\S]*?background: transparent;/);
  assert.match(styleSource, /\.art-registry-card--enabled \{[\s\S]*?color-mix\(in srgb, var\(--loom-theme-success\) 10%, var\(--loom-theme-panel\)\)/);
  assert.match(styleSource, /\.framework-dialog \{[\s\S]*?background: var\(--loom-theme-panel\)/);
  assert.match(styleSource, /\.hook-canvas-rename-dialog,[\s\S]*?background: var\(--loom-theme-panel\)/);
  assert.match(styleSource, /\.hook-canvas-workspace \{[\s\S]*?border: 0;[\s\S]*?background: transparent;/);
  assert.match(styleSource, /\.hook-canvas-surface \{[\s\S]*?overflow: hidden;/);
});

test("provides independent proxy settings for Loom and Hook", () => {
  assert.equal((appSource.match(/<SettingsAccordionSection id="network" label="网络"/g) || []).length, 2);
  assert.match(appSource, /function NetworkSettingsPanel\(/);
  assert.match(appSource, /appName="Loom"[\s\S]*?value=\{draft\.network\.loom\}/);
  assert.match(appSource, /appName="Hook"[\s\S]*?value=\{draft\.network\.hook\}/);
  assert.match(appSource, /<option value="system">跟随系统<\/option>/);
  assert.match(appSource, /<option value="custom">自定义<\/option>/);
  assert.match(appSource, /<option value="disabled">不使用代理<\/option>/);
  assert.match(appSource, /value\.mode === "custom"/);
  assert.match(appSource, /<option value="http">http:\/\/<\/option>/);
  assert.match(appSource, /<option value="https">https:\/\/<\/option>/);
  assert.match(appSource, /<option value="socks5">socks5:\/\/<\/option>/);
  assert.match(appSource, /placeholder="127\.0\.0\.1:7890"/);
  assert.match(appSource, /const updateNetworkDraft[\s\S]*?\[app\]: \{ \.\.\.current\.network\[app\], \.\.\.patch \}/);
  assert.match(styleSource, /\.settings-network-panel,[\s\S]*?\.settings-general-panel,[\s\S]*?\.settings-mcp-panel,[\s\S]*?\.settings-art-store-panel \{[\s\S]*?border: 0;[\s\S]*?background: transparent;/);
  assert.match(styleSource, /\.settings-network-row \{[\s\S]*?grid-template-columns:/);
});

test("provides independent compact general settings for Loom and Hook", () => {
  assert.equal((appSource.match(/<SettingsAccordionSection id="general" label="常规"/g) || []).length, 2);
  assert.match(appSource, /function GeneralSettingsPanel\(/);
  assert.match(appSource, /appName="loom"[\s\S]*?language: draft\.general\.language[\s\S]*?theme: draft\.general\.theme[\s\S]*?closeToTray: draft\.general\.minimize_to_tray/);
  assert.match(appSource, /appName="hook"[\s\S]*?language: draft\.hook_general\.language[\s\S]*?theme: draft\.hook_general\.theme[\s\S]*?closeToTray: draft\.hook_general\.close_to_tray/);
  assert.match(appSource, /<strong>语言<\/strong>[\s\S]*?<strong>主题<\/strong>[\s\S]*?<strong>关闭到系统托盘<\/strong>/);
  assert.match(appSource, /const updateHookGeneralDraft[\s\S]*?hook_general:/);
  assert.doesNotMatch(appSource, /SettingsAccordionSection id="window"/);
  assert.match(styleSource, /\.settings-network-panel,[\s\S]*?\.settings-general-panel,[\s\S]*?\.settings-mcp-panel,[\s\S]*?\.settings-art-store-panel \{[\s\S]*?border: 0;[\s\S]*?background: transparent;/);
  assert.match(styleSource, /\.settings-general-toggle \{[\s\S]*?justify-self: end;/);
  assert.match(appSource, /applyLoomGeneralSettings\(draft\.general\)/);
  assert.match(appSource, /invoke\("apply_loom_general_settings"/);
  assert.match(styleSource, /:root\[data-loom-theme="light"\]/);
});

test("replaces System and Data with runtime-backed MCP and Art settings", () => {
  assert.doesNotMatch(appSource, /SettingsAccordionSection id="system"|label="系统与数据"/);
  assert.doesNotMatch(appSource, /toggleAutostart|setArtLoomCompatAutostart/);
  assert.match(appSource, /SettingsAccordionSection id="mcp" label="MCP"/);
  assert.match(appSource, /function McpSettingsPanel\(/);
  assert.match(appSource, /MCP 请求超时/);
  assert.match(appSource, /MCP 子进程内存上限/);
  assert.match(appSource, /SettingsAccordionSection id="art-store" label="Art"/);
  assert.match(appSource, /function ArtStoreSettingsPanel\(/);
  assert.doesNotMatch(appSource, /商店地址|settings-art-store-url|value\.base_url/);
  assert.match(appSource, /Art 自动更新/);
  assert.match(appSource, /Art 默认只显示官方/);
  assert.match(appSource, /Art 安装策略/);
  assert.match(appSource, /setPluginTrustPolicy\(snapshot\.baseUrl, policy\)/);
  assert.match(appSource, /setStoreOfficialOnly\(settings\.art_store\?\.official_only === true\)/);
  assert.match(styleSource, /\.settings-mcp-panel/);
  assert.match(styleSource, /\.settings-art-store-panel/);
});

test("manages only rebuildable Loom caches and removes the obsolete engine UI", () => {
  assert.equal((appSource.match(/<SettingsAccordionSection id="cache" label="缓存"/g) || []).length, 2);
  assert.doesNotMatch(appSource, /SettingsAccordionSection id="engine"|label="引擎"|draft\.engine/);
  assert.match(appSource, /function LoomCacheSettingsPanel\(/);
  assert.match(appSource, /get_loom_cache_snapshot/);
  assert.match(appSource, /apply_loom_cache_settings/);
  assert.match(appSource, /clear_loom_cache/);
  assert.match(appSource, /loomCachePreferencesForRuntime\(saved\.loom_cache\)/);
  assert.match(appSource, /Art 运行缓存上限/);
  assert.match(appSource, /Art 运行缓存自动清理周期/);
  assert.match(appSource, /框架临时文件自动清理周期/);
  assert.match(appSource, /不会卸载 Art 或删除工作流/);
  assert.doesNotMatch(appSource, /清空已安装 Art|清空工作流|清空运行记录/);
});

test("manages Hook recycle bin, temporary cache, and reference images from the Hook settings tab", () => {
  assert.equal((appSource.match(/<SettingsAccordionSection id="cache" label="缓存"/g) || []).length, 2);
  assert.match(appSource, /function HookCacheSettingsPanel\(/);
  assert.match(appSource, /get_hook_cache_snapshot/);
  assert.match(appSource, /clear_hook_cache/);
  assert.match(appSource, /wait_for_hook_cache_settings/);
  assert.match(appSource, /hookCachePreferencesForRuntime\(saved\.hook_cache\)/);
  assert.match(appSource, /回收站上限/);
  assert.match(appSource, /回收站自动清理周期/);
  assert.match(appSource, /临时缓存上限/);
  assert.match(appSource, /临时缓存自动清理周期/);
  assert.match(appSource, /清空回收站/);
  assert.match(appSource, /清空临时缓存/);
  assert.match(appSource, /清空参考图/);
  assert.match(appSource, /\[15, 50, 0\]/);
  assert.equal((appSource.match(/\[3, 7, 30, 0\]/g) || []).length, 3);
  assert.match(appSource, /label: "128 MB"/);
  assert.match(appSource, /label: "256 MB"/);
  assert.match(appSource, /label: "1 GB"/);
  assert.match(appSource, /label: "无限制"/);
  assert.doesNotMatch(appSource, /hook-cache-usage-title/);
  assert.equal((appSource.match(/hook-cache-row hook-cache-row--action/g) || []).length, 5);
  assert.doesNotMatch(appSource, /图片搜索缓存/);
  assert.doesNotMatch(appSource, /剪贴板缓存/);
  assert.match(appSource, /requestAppConfirmation\(\{[\s\S]*?title: `清空\$\{labels\[kind\]\}`/);
  assert.match(styleSource, /\.hook-cache-settings \{[\s\S]*?gap: 12px;/);
  assert.match(styleSource, /\.hook-cache-group \{[\s\S]*?border: 0;[\s\S]*?background: transparent;/);
  assert.match(styleSource, /\.hook-cache-row--total b \{[\s\S]*?color: var\(--loom-theme-accent-text\);/);
});

test("auto-saves Loom fields and Hook shortcuts without a manual save action", () => {
  assert.match(appSource, /pendingSettingsRef\.current = draft/);
  assert.match(appSource, /window\.setTimeout\(\(\) => \{[\s\S]*?void flushSettingsQueue\(\);[\s\S]*?\}, 360\)/);
  assert.match(appSource, /while \(pendingSettingsRef\.current\)/);
  assert.match(appSource, /await saveLoomSettings\(baseUrl, nextSettings\)/);
  assert.match(appSource, /const updateShortcutDraft[\s\S]*?shortcuts: Object\.fromEntries\(nextShortcuts\.map/);
  assert.doesNotMatch(appSource, /saveSettingsDraft|saveShortcutDraft|保存兼容设置/);
});

test("groups, collapses, and edits Hook shortcuts without nested scrolling", () => {
  assert.match(appSource, /label: "捕获与操作"/);
  assert.match(appSource, /label: "高级工具"/);
  assert.match(appSource, /label: "贴图操作"/);
  assert.match(appSource, /label: "贴图编辑"/);
  assert.match(appSource, /label: "贴图工具栏"[\s\S]*?label: "复制控件"/);
  assert.match(appSource, /label: "选择模式"/);
  assert.match(appSource, /label: "移动模式"/);
  assert.match(appSource, /label: "旋转模式"/);
  assert.match(appSource, /label: "缩放模式"/);
  assert.match(appSource, /className="hook-shortcut-group__trigger"[\s\S]*?aria-expanded=\{groupOpen\}/);
  assert.match(appSource, /function ShortcutKeySequence\(/);
  assert.match(appSource, /role="dialog"[\s\S]*?aria-labelledby="shortcut-edit-dialog-title"/);
  assert.match(appSource, /type ShortcutSlot = 0 \| 1 \| 2/);
  assert.match(appSource, /keys: \[keys\[0\] \|\| "", keys\[1\] \|\| "", keys\[2\] \|\| ""\]/);
  assert.match(appSource, /shortcutEditor\.slotCount < 3[\s\S]*?添加额外快捷键/);
  assert.match(appSource, /slotCount: \(current\.slotCount \+ 1\) as 2 \| 3/);
  assert.match(appSource, /removeShortcutSlot\(current\.keys, slot\)/);
  assert.match(appSource, /handleShortcutCapture\(event, slot\)/);
  assert.match(appSource, /updateShortcutDraft\([\s\S]*?shortcutEditor\.item\.label,[\s\S]*?keys,/);
  assert.match(appSource, /同一事件的快捷键不能重复/);
  assert.match(appSource, /shortcutContextsOverlap\(candidateContexts, item\.contexts\)/);
  assert.match(appSource, /conflictFamily: "contextual-cancel-delete"/);
  assert.equal((appSource.match(/keys: \["Escape", "Delete", "Backspace"\]/g) || []).length, 3);
  assert.match(appSource, /label: "强行关闭"[\s\S]*?keys: \["Esc × 3"\]/);
  assert.match(appSource, /contexts: \["unit-selected"\][\s\S]*?contexts: \["sticker-editing"\]/);
  assert.match(appSource, /id: "control-quick-move", sourceId: "control_quick_move"/);
  assert.match(appSource, /gestureAction: "拖动"/);
  assert.match(appSource, /shortcut-gesture-picker/);
  assert.match(appSource, /toggleGestureShortcutModifier/);
  assert.match(appSource, /添加 Art 快捷键/);
  assert.match(appSource, /aria-labelledby="quick-binding-dialog-title"/);
  assert.match(appSource, /quickBindingEditor\.slotCount < 3[\s\S]*?添加额外快捷键/);
  assert.match(appSource, /availableArtTools\.map/);
  assert.match(appSource, /quick_bindings: current\.quick_bindings\.some/);
  assert.doesNotMatch(styleSource, /\.hook-shortcut-list \{[^}]*overflow-y:/);
  assert.match(styleSource, /\.shortcut-gesture-picker \{/);
  assert.match(styleSource, /\.shortcut-add-secondary \{/);
});
