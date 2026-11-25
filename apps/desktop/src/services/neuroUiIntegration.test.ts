import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import test from "node:test";

test("consumes the independent local UI package without a second React instance", () => {
  const manifest = JSON.parse(readFileSync(new URL("../../package.json", import.meta.url), "utf8"));
  assert.equal(manifest.dependencies["@neuro/ui"], "file:../../../../nas_home/NeuroDesign/packages/react/neuro-ui");
  assert.equal(manifest.scripts["ui:sync"], "node ../../../../nas_home/NeuroDesign/scripts/sync-consumer.mjs --package @neuro/ui");
  const lock = JSON.parse(readFileSync(new URL("../../package-lock.json", import.meta.url), "utf8"));
  assert.equal(lock.packages[""].dependencies["@neuro/ui"], manifest.dependencies["@neuro/ui"]);
  assert.equal(lock.packages["node_modules/@neuro/ui"].resolved, manifest.dependencies["@neuro/ui"]);
  assert.notEqual(lock.packages["node_modules/@neuro/ui"].link, true);
  const app = createRequire(import.meta.url);
  const ui = createRequire(new URL(import.meta.resolve("@neuro/ui")));
  assert.equal(ui.resolve("react"), app.resolve("react"));
  assert.equal(ui.resolve("react-dom"), app.resolve("react-dom"));
  const owners = ["@radix-ui/react-dialog", "@radix-ui/react-popover", "@radix-ui/react-select"];
  for (const shared of ["@radix-ui/react-focus-scope", "@radix-ui/react-dismissable-layer"]) {
    assert.equal(new Set(owners.map(owner => createRequire(ui.resolve(owner)).resolve(shared))).size, 1, shared);
  }
});

test("MCP panels and navigation use standard components, retaining domain layout and native package input", () => {
  const source = (file: string) => readFileSync(new URL(`../components/mcp/${file}.tsx`, import.meta.url), "utf8");
  const toolbar = source("McpHubToolbar");
  const services = source("McpServicesPanel");
  const store = source("McpStorePanel");
  for (const text of [toolbar, services, store]) {
    assert.match(text, /from "@neuro\/ui"/);
    assert.doesNotMatch(text, /<(?:button|select|textarea)\b|className="(?:studio-input|ghost-button|signal-button|art-card-action)/);
  }
  assert.match(toolbar, /hidden\s+type="file"/);
  assert.match(toolbar, /event\.currentTarget\.value = ""/);
  assert.match(services, /disabled=\{\s*operationBusy \|\| !enabled \|\| Boolean\(server\.credentialRequired && !server\.credentialBound\)\s*\}/);
  assert.match(services, /role=\{snapshot\.status === "error" \? "alert" : "status"\}/);
  assert.match(services, /className="mcp-card__status-text"/);
  for (const text of [services, store]) {
    assert.match(text, /<Panel\b/);
    assert.doesNotMatch(text, /glass-card|art-(?:registry|store)-card|<article\b/);
  }
  assert.match(services, /<Badge tone=\{enabled \? "success" : "neutral"\}>\s*\{enabled \? "已启用" : "已禁用"\}\s*<\/Badge>/);
  const layout = readFileSync(new URL("../styles/mcp-hub.css", import.meta.url), "utf8");
  assert.match(layout, /\.mcp-card__status > \.n-badge \{[^}]*max-width: 100%/);
  assert.match(layout, /\.mcp-card__status-text \{[^}]*overflow: hidden;\s*text-overflow: ellipsis/);
  assert.match(store, /aria-current=\{item === resolvedMarketPage \? "page" : undefined\}/);
});

test("MCP editors delegate modal lifecycle while retaining validation and secret mutation ownership", () => {
  const server = readFileSync(new URL("../components/mcp/McpServerDialog.tsx", import.meta.url), "utf8");
  const credentials = readFileSync(new URL("../components/mcp/McpCredentialDialog.tsx", import.meta.url), "utf8");
  for (const source of [server, credentials]) {
    assert.match(source, /from "@neuro\/ui"/);
    assert.match(source, /<Dialog\s+open/);
    assert.match(source, /busy=\{busy\}\s+initialFocusRef=/);
    assert.doesNotMatch(source, /createPortal|document\.body\.style\.overflow|addEventListener\("keydown"/);
    assert.doesNotMatch(source, /className="(?:studio-input|studio-textarea|ghost-button|signal-button)"/);
  }
  assert.match(server, /if \(!canSave \|\| busy\) return/);
  assert.match(server, /id: normalizedServerId\(id\)/);
  assert.match(server, /invalidEnvironmentLines.length === 0\s*&&\s*invalidHeaderLines.length === 0/);
  assert.match(credentials, /void onSave\(enteredValues, \[\.\.\.clear\]\)/);
  assert.match(credentials, /type="password"/);
});

test("bootstraps the real theme provider and package CSS while retaining application feedback ownership", () => {
  const entry = readFileSync(new URL("../main.tsx", import.meta.url), "utf8");
  const theme = readFileSync(new URL("../components/LoomUiProvider.tsx", import.meta.url), "utf8");
  const feedback = readFileSync(new URL("../components/feedback/AppFeedback.tsx", import.meta.url), "utf8");
  assert.match(entry, /import "@neuro\/ui\/styles\.css"/);
  assert.match(entry, /<LoomUiProvider><App \/><\/LoomUiProvider>/);
  assert.match(theme, /<NeuroProvider mode=\{mode\} inheritTypography/);
  assert.match(theme, /observer\.disconnect\(\)/);
  assert.match(theme, /removeEventListener\("change", onChange\)/);
  assert.match(feedback, /from "@neuro\/ui"/);
  assert.match(feedback, /appToastSubscribers\.add\(receive\)/);
  assert.match(feedback, /appToastSubscribers\.delete\(receive\)/);
  assert.doesNotMatch(feedback, /createPortal|document\.body\.style\.overflow|addEventListener\("keydown"/);
});

test("framework and permission dialogs share presentation without moving lifecycle approval into the UI library", () => {
  const framework = readFileSync(new URL("../components/art/FrameworkManagementDialog.tsx", import.meta.url), "utf8");
  const permission = readFileSync(new URL("../components/settings/capabilities/CapabilityPermissionDialog.tsx", import.meta.url), "utf8");
  for (const source of [framework, permission]) {
    assert.match(source, /from "@neuro\/ui"/);
    assert.doesNotMatch(source, /createPortal|querySelectorAll|addEventListener|document\.body/);
  }
  assert.match(framework, /if \(!open\) setPendingUninstallId\(null\)/);
  assert.match(framework, /if \(!confirmingUninstall\) \{\s*setPendingUninstallId\(identity\)/);
  assert.match(framework, /event\.currentTarget\.value = ""/);
  assert.match(framework, /if \(file\) void onUpgrade\(framework, file\)/);
  assert.match(permission, /title=\{review\.digest\}/);
  assert.match(permission, /onClick=\{onConfirm\}/);
  assert.doesNotMatch(permission, /approveCapabilityPermissions|enableCapability|upgradeCapability|rollbackCapability/);
});
