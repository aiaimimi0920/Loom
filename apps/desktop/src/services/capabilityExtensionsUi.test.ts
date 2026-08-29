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
const permissionDialog = readSource(
  "../components/settings/capabilities/CapabilityPermissionDialog.tsx",
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

test("permission review displays digest-bound evidence before activation", () => {
  assert.match(permissionDialog, /权限与不可变包摘要绑定/);
  assert.match(permissionDialog, /review\.digest/);
  assert.match(permissionDialog, /review\.permissions\.map/);
  assert.match(permissionDialog, /aria-modal="true"/);
  assert.match(permissionDialog, /querySelectorAll<HTMLElement>/);
  assert.match(permissionDialog, /previousFocus\?\.focus/);
});
