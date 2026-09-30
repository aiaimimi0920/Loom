import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const readSource = (path: string) => readFileSync(new URL(path, import.meta.url), "utf8");
const settingsSections = readSource("../components/settings/LoomSettingsSections.tsx");
const settingsController = readSource("../components/settings/useSettingsPanelController.ts");
const capabilityController = readSource(
  "../components/settings/capabilities/useCapabilityExtensionsController.ts",
);
const capabilityPanel = readSource(
  "../components/settings/capabilities/CapabilityExtensionsPanel.tsx",
);
const capabilitySettings = readSource(
  "../components/settings/capabilities/CapabilitySettingsEditor.tsx",
);
const capabilityApi = readSource("./loomApi/capabilities.ts");
const permissionDialog = readSource(
  "../components/settings/capabilities/CapabilityPermissionDialog.tsx",
);
const officialOcrCard = readSource(
  "../components/settings/capabilities/OfficialOcrCapabilityCard.tsx",
);

test("capability lifecycle owns an independent Loom settings section", () => {
  assert.match(settingsSections, /id="capabilities"[\s\S]*?<CapabilityExtensionsPanel/);
  assert.doesNotMatch(settingsController, /listCapabilityPlugins|installCatalogCapability/);
  assert.doesNotMatch(capabilityController, /saveLoomSettings|setDraft/);
  assert.match(capabilityController, /operationRef\.current/);
  assert.match(capabilityController, /baseUrlRef\.current = baseUrl;\s+setPermissionReview\(null\)/);
});

test("capability UI exposes installed, local ZIP, and signed catalog workflows", () => {
  assert.match(capabilityPanel, /已安装/);
  assert.match(capabilityPanel, /安装本地 ZIP/);
  assert.match(capabilityPanel, /官方目录/);
  assert.match(capabilityPanel, /Ed25519 签名/);
  assert.match(capabilityPanel, /SBOM/);
  assert.match(capabilityPanel, /Provenance/);
  assert.match(capabilityPanel, /hostCompatibility\.loomCapabilityApi/);
  assert.match(capabilityPanel, /role="tabpanel"/);
  assert.match(capabilityPanel, /aria-controls="capability-panel-installed"/);
});

test("official OCR quick install reuses the signed catalog lifecycle", () => {
  assert.match(officialOcrCard, /neuro\.official\/ocr/);
  assert.match(officialOcrCard, /下载并安装 OCR/);
  assert.match(officialOcrCard, /大模型包只通过受信任目录下载/);
  assert.match(capabilityPanel, /controller\.installFromCatalog\(item\)/);
  assert.doesNotMatch(officialOcrCard, /fetch\(|installLocalCapability|https?:\/\//);
  assert.match(officialOcrCard, /aria-live="polite"/);
  assert.match(officialOcrCard, /disabled=\{busy \|\| loading \|\| !canInstall\}/);
});

test("permission review displays digest-bound evidence before activation", () => {
  assert.match(permissionDialog, /权限与不可变包摘要绑定/);
  assert.match(permissionDialog, /review\.digest/);
  assert.match(permissionDialog, /review\.permissions\.map/);
  assert.match(permissionDialog, /aria-modal="true"/);
  assert.match(permissionDialog, /querySelectorAll<HTMLElement>/);
  assert.match(permissionDialog, /previousFocus\?\.focus/);
});

test("plugin settings are manifest-driven and use the isolated settings API", () => {
  assert.match(capabilityPanel, /<CapabilitySettingsEditor/);
  assert.match(capabilitySettings, /snapshot\.fields\.flatMap/);
  assert.match(capabilitySettings, /hasOwnProperty\.call\(snapshot\.settings\.values/);
  assert.match(capabilitySettings, /saveCapabilitySettings/);
  assert.doesNotMatch(capabilitySettings, /saveLoomSettings|dangerouslySetInnerHTML/);
  assert.match(capabilityApi, /pluginPath\(qualifiedId, "settings"\)/);
  assert.match(capabilityApi, /expectedRevision/);
  assert.match(capabilityApi, /packageDigest/);
});
