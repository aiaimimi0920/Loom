import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const source = (file: string) => readFileSync(new URL(`../components/${file}`, import.meta.url), "utf8");
const workspace = source("devices/DeviceManagementPanel.tsx");
const panel = source("devices/ManagedDevicesPanel.tsx");
const dialogs = source("devices/ManagedDeviceDialogs.tsx");
const controller = source("devices/useManagedDevicesController.ts");
const credentials = source("security/CredentialFieldDialog.tsx");
const security = source("security/PluginSecurityPanel.tsx");

test("device workspaces use standard tabs while retaining per-page mount and key boundaries", () => {
  assert.match(workspace, /<TabsRoot asChild value=\{page\}/);
  assert.match(workspace, /<TabList label=\{t\("devices\.tabs\.ariaLabel"\)\}/);
  assert.match(workspace, /<Tab key=\{item\.value\} value=\{item\.value\}/);
  for (const page of ["devices", "walls"]) {
    assert.match(workspace, new RegExp(`<TabPanel value="${page}" forceMount hidden=\\{page !== "${page}"\\}`));
  }
  for (const page of ["projection", "projection-settings"]) {
    assert.match(workspace, new RegExp(`page === "${page}" &&`));
  }
  assert.match(workspace, /key=\{props\.baseUrl \+ String\(props\.online\)\}/);
  assert.doesNotMatch(workspace, /<button|device-workspace-tabs|[\u4e00-\u9fff]/);
});

test("device cards and forms consume standard components rather than a parallel control skin", () => {
  for (const text of [panel, dialogs, credentials]) {
    assert.match(text, /from "@neuro\/ui"/);
    assert.doesNotMatch(
      text,
      /<(?:button|input|select|textarea)\b|createPortal|framework-dialog|studio-input|ghost-button|signal-button/,
    );
    assert.doesNotMatch(text, /addEventListener|document\.body|setTimeout|autoFocus/);
  }
  assert.match(panel, /aria-pressed=\{selected\}/);
  assert.match(panel, /<Badge tone=\{device\.online \? "success" : "neutral"\}/);
  assert.match(panel, /target\?\.isConnected && !target\.closest\("\[hidden\], \[inert\]"\)/);
  assert.doesNotMatch(panel, /\u6dfb\u52a0\u7535\u8111\u3001\u5e73\u677f\u6216\u5176\u4ed6\u5ba2\u6237\u7aef\u540e\u4f1a\u663e\u793a\u5728\u8fd9\u91cc|[\u4e00-\u9fff]/);
  assert.doesNotMatch(dialogs, /[\u4e00-\u9fff]/);
  assert.doesNotMatch(controller, /[\u4e00-\u9fff]/);
});

test("pending approval uses stable flat checkboxes up to six devices and searchable dropdown beyond that", () => {
  assert.match(dialogs, /pending\.length > 6 \? \(\s*<MultiSelect/);
  assert.match(dialogs, /values=\{selectedPendingIds\}\s+onValuesChange=\{setSelectedPendingIds\}/);
  assert.match(dialogs, /aria-label=\{t\("devices\.managed\.dialog\.pendingLabel"\)\}/);
  assert.match(dialogs, /<Checkbox\s+key=\{device\.id\}/);
  assert.match(dialogs, /checked=\{checked\}\s+disabled=\{busy\}/);
  assert.match(
    dialogs,
    /setSelectedPendingIds\(\(ids\) =>\s*checked \? ids\.filter\(\(id\) => id !== device\.id\) : \[\.\.\.ids, device\.id\],?\s*\)/,
  );
  assert.doesNotMatch(dialogs, /device-request-row--selected/);
});

test("device mutations retain trimming, sequential approvals, local-host protection and polling cleanup", () => {
  assert.match(controller, /const name = editingDevice\.name\.trim\(\)/);
  assert.match(controller, /const address = editingDevice\.address\.trim\(\)/);
  assert.doesNotMatch(controller, /addManagedDevice|submitDevice|draftName/);
  assert.doesNotMatch(dialogs, /\u6dfb\u52a0\u8bbe\u5907|submitDevice|addName/);
  assert.match(
    controller,
    /for \(const deviceId of selectedPendingIds\) \{\s*response = await approveManagedDevice\(baseUrl, deviceId\)/,
  );
  assert.match(controller, /if \(!editingDevice \|\| editingDevice\.isLocal\) return/);
  assert.match(controller, /enabled: editingDevice\.enabled \?\? true/);
  assert.match(controller, /window\.setInterval\(\(\) => void refreshDevices\(true\), 4000\)/);
  assert.match(controller, /window\.clearInterval\(timer\)/);
  assert.match(controller, /if \(!accepted \|\| version !== generation\.current\) return/);
  assert.match(dialogs, /onClick=\{\(\) => void removeDevice\(editingDevice\)\}/);
  assert.match(dialogs, /loading=\{busyId === "approve-selected"\}/);
});

test("credential modal preserves typed draft conversion and delegates focus and busy lifecycle", () => {
  assert.match(credentials, /busy=\{busy\}\s+initialFocusRef=\{nameInput\}/);
  assert.match(credentials, /valueType: event\.target\.value as LoomCredentialValueType, value: ""/);
  assert.match(credentials, /draft\.valueType === "boolean"/);
  assert.match(credentials, /draft\.valueType === "json"/);
  assert.match(credentials, /step=\{draft\.valueType === "integer" \? 1 : "any"\}/);
  assert.match(credentials, /disabled=\{busy \|\| !draft\.name\.trim\(\) \|\| !draft\.value\}/);
  assert.match(credentials, /fallbackFocusRef\?\.current \?\? null/);
  assert.match(credentials, /returnFocusRef=\{returnFocusRef\}/);
  const list = source("security/CredentialFieldList.tsx");
  assert.match(list, /returnFocus\.current = event\.currentTarget;\s*onEdit\(credential\)/);
  assert.match(security, /returnFocus=\{credentialReturnFocus\}/);
  assert.match(security, /onEdit=\{\(credential\) => void openCredential\(credential\)\}/);
  assert.equal(security.match(/await load\(\);\s*setCredentialDraft\(null\)/g)?.length, 2);
});

test("daemon identity changes invalidate late reads, writes and approvals without cancelling issued operations", () => {
  assert.match(controller, /const generation = useRef\(0\)/);
  assert.match(controller, /if \(refreshInFlight\.current === version\) return/);
  assert.match(
    controller,
    /const response = await listManagedDevices\(baseUrl\);\s*if \(version === generation\.current\) applyResponse\(response\)/,
  );
  assert.match(controller, /if \(refreshInFlight\.current === version\) refreshInFlight\.current = null/);
  assert.match(controller, /return \(\) => \{\s*generation\.current \+= 1;\s*window\.clearInterval\(timer\)/);
  assert.match(
    controller,
    /response = await approveManagedDevice\(baseUrl, deviceId\);\s*if \(version !== generation\.current\) return/,
  );
  assert.equal(controller.match(/if \(version === generation\.current\) setBusyId\(null\)/g)?.length, 3);
  assert.match(controller, /if \(!accepted \|\| version !== generation\.current\) return;\s*setBusyId\(device\.id\)/);
  assert.match(
    controller,
    /setApprovalDialogOpen\(false\);\s*setEditingDevice\(null\);\s*setSelectedPendingIds\(\[\]\)/,
  );
});
