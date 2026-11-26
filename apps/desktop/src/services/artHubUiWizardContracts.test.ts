import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { uiTextFor } from "../i18n/index.ts";
import { artWorkspaceItems, nextArtWorkspaceIndex } from "./artHubUi.ts";
import { appSource, styleSource } from "./artHubUiContractSource.ts";

test("opens local Art creation in a dedicated accessible dialog", () => {
  const dialog = readFileSync(new URL("../components/art/ArtCreationDialog.tsx", import.meta.url), "utf8");
  assert.match(appSource, /const \[createDialogOpen, setCreateDialogOpen\] = useState\(false\)/);
  assert.match(dialog, /from "@neuro\/ui"/);
  assert.match(dialog, /<Dialog\s+open=\{open\}\s+title=\{t\("art\.action\.createArt"\)\}\s+busy=\{busy\}/);
  assert.equal(uiTextFor("en", "art.action.createArt"), "Create Art");
  assert.ok(uiTextFor("zh-Hans", "art.action.createArt").length > 0);
  assert.match(dialog, /if \(!next\) onClose\(\)/);
  assert.doesNotMatch(dialog, /addEventListener|querySelectorAll|framework-dialog-backdrop|setTimeout/);
  assert.match(appSource, /<ArtCreationDialog[\s\S]*?<AddArtWizard/);
  assert.match(appSource, /await createAuthoredArtPackage\([\s\S]*?authored\.tool/);
  assert.match(appSource, /tf\("art\.wizard\.message\.createdAndInstalled", \{ name: derivedName \}\)/);
  assert.doesNotMatch(appSource, /<section className="main-board add-art-wizard"/);
  assert.doesNotMatch(appSource, /className="studio-grid art-create-dialog__source"/);
  assert.match(
    styleSource,
    /\.n-dialog\.loom-art-create-dialog \{[\s\S]*?width: min\(1120px, calc\(100vw - 32px\)\);[\s\S]*?height: min\(820px, calc\(100dvh - 32px\)\);/,
  );
  assert.match(
    styleSource,
    /\.art-creator-panel__scroll \{[\s\S]*?min-height: 0;[\s\S]*?overflow-y: auto;[\s\S]*?scrollbar-width: thin;/,
  );
});

test("routes saved Otter workflows through the prefilled workflow Art creator", () => {
  assert.match(
    appSource,
    /const \[pendingArtCreationRequest, setPendingArtCreationRequest\] = useState<ArtCreationRequest \| null>\(null\)/,
  );
  assert.match(
    appSource,
    /mode: "workflow",[\s\S]*?repositoryName: request\.tool\.id,[\s\S]*?workflowId: request\.workflowId,[\s\S]*?templateTool: request\.tool/,
  );
  assert.match(appSource, /setActiveSection\("registry"\)/);
  assert.match(appSource, /initialRequest=\{createRequest\}/);
  assert.match(appSource, /authored\.tool\.params = draft\.paramPorts\.map\(toolParamFromDraft\)/);
  assert.match(appSource, /const workflowBindings = workflowBindingsFromDraft\(draft\)/);
  assert.match(appSource, /workflowBindings,/);
  assert.match(appSource, /<h4>\{t\("art\.wizard\.ports\.params"\)\}<\/h4>/);
});

test("binds public workflow Art params to internal node params", () => {
  assert.match(appSource, /getWorkflowBundle\(baseUrl, workflowId\.trim\(\)\)/);
  assert.match(appSource, /collectWorkflowParamBindingCandidates\(workflow, tools\)/);
  assert.match(appSource, /title=\{t\("art\.wizard\.ports\.bindNodeParamTitle"\)\}/);
  assert.match(appSource, /bindingNodeId: candidate\.nodeId/);
  assert.match(appSource, /bindingTarget: candidate\.target/);
  assert.match(appSource, /bindingKind: "param"/);
  assert.match(appSource, /widget: candidate\.widget \|\| defaultWidgetForParam\(candidate\.type\)/);
  assert.match(appSource, /dataType: candidate\.dataType \|\| ""/);
  assert.match(appSource, /min: candidate\.min/);
  assert.match(appSource, /max: candidate\.max/);
  assert.match(appSource, /step: candidate\.step/);
  assert.match(appSource, /dataType: stringValue\(port\.data_type\) \|\| stringValue\(port\.dataType\)/);
  assert.match(appSource, /if \(typeof port\.min === "number"\) next\.min = port\.min/);
  assert.match(appSource, /if \(typeof port\.max === "number"\) next\.max = port\.max/);
  assert.match(appSource, /if \(typeof port\.step === "number"\) next\.step = port\.step/);
  assert.match(
    appSource,
    /workflowParam,[\s\S]*?nodeId: port\.bindingNodeId,[\s\S]*?target: port\.bindingTarget,[\s\S]*?kind: "param"/,
  );
});

test("configures necessary nodes and one preview output without growing the Art dialog", () => {
  assert.match(appSource, /collectWorkflowPreviewNodeOptions\(workflowGraph, tools\)/);
  assert.match(appSource, /previewOutput: Some|previewOutput/);
  assert.match(appSource, /previewRequiredNodes/);
  assert.match(appSource, /<RadioGroup\s+label=\{t\("art\.wizard\.preview\.previewNode"\)\}\s+name=\{name\}/);
  assert.match(appSource, /<CheckboxGroup\s+label=\{t\("art\.wizard\.preview\.requiredNodes"\)\}/);
  assert.match(appSource, /<MultiSelect\s+label=\{t\("art\.wizard\.preview\.requiredNodes"\)\}/);
  assert.match(appSource, /options\.length > 6/);
  assert.match(appSource, /disabled: option\.nodeId === output\?\.nodeId/);
  assert.match(appSource, /onNodeRequired\(option\.nodeId, next\.includes\(option\.nodeId\)\)/);
  assert.match(
    appSource,
    /setWorkflowPreviewOutput\(\{ nodeId: option\.nodeId, output: output\.name, kind: "node_result" \}\)/,
  );
  assert.match(appSource, /<Select value=\{selected\?\.nodeId \?\? ""\}/);
  assert.match(appSource, /<Field label=\{t\("art\.wizard\.preview\.previewPort"\)\}>/);
});

test("uses four concise Art creator categories with one active creation form", () => {
  const creatorSource = appSource.match(/function AddArtWizard\([\s\S]*?function ArtCreationDialog/);
  assert.ok(creatorSource);
  for (const modeId of ["cloud_api", "mcp", "process", "workflow"]) {
    assert.match(appSource, new RegExp(`(?:createArtWizardMode\\("${modeId}"|id:\\s*"${modeId}")`));
  }
  assert.match(appSource, /<TabsRoot\s+className="add-art-wizard"\s+value=\{mode\}/);
  assert.match(appSource, /<TabList label=\{t\("art\.wizard\.modeTabsAria"\)\} className="loom-art-mode-tabs">/);
  assert.match(appSource, /artWizardModes\.map\(\(item\) => \(\s*<Tab key=\{item\.id\} value=\{item\.id\}/);
  assert.match(appSource, /<TabPanel className="art-creator-config" value=\{mode\}/);
  assert.match(appSource, /\{item\.title\}/);
  assert.doesNotMatch(appSource, /<strong>\{item\.title\}<\/strong>/);
  assert.doesNotMatch(appSource, /\{item\.subtitle\}[\s\S]*\{item\.executionLabel\}/);
  for (const mode of ["cloud_api", "mcp", "process", "workflow"]) {
    assert.match(appSource, new RegExp(`mode === "${mode}" \\?\\s*(?:\\(\\s*)?<`));
  }
  assert.match(appSource, /<form\s+className="art-creator-panel"/);
  assert.match(appSource, /<Field label=\{t\("art\.wizard\.field\.repositoryName"\)\}>\s*<Input\s+value=\{repositoryName\}/);
  assert.match(appSource, /<Field label=\{t\("art\.wizard\.field\.artName"\)\}>\s*<Input value=\{name\}/);
  assert.doesNotMatch(appSource, />\s*Art ID\s*<input/);
  assert.doesNotMatch(creatorSource[0], /repositoryName[\s\S]*globalId/);
  assert.match(appSource, /<Disclosure\s+className="art-creator-ports"/);
  assert.match(appSource, /scriptEntryKind === "python"/);
  assert.match(appSource, /value: "command", label: t\("art\.wizard\.script\.entryCommand"\)/);
  assert.match(styleSource, /\.loom-art-mode-tabs \{[\s\S]*?grid-template-columns: repeat\(4, minmax\(0, 1fr\)\);/);
  const wizard = readFileSync(new URL("../components/art/AddArtWizard.tsx", import.meta.url), "utf8");
  assert.doesNotMatch(
    wizard,
    /<(?:input|textarea|select|button|details)\b|studio-input|ghost-button|signal-button|art-mode-card/,
  );
  assert.match(
    wizard,
    /<Button\s+variant="primary"\s+loading=\{busy\}\s+type="submit"\s+disabled=\{busy \|\| Boolean\(sourceBusyAction\) \|\| !selectedFrameworkReady\}/,
  );
});

test("opens an accessible framework management dialog with version and package update actions", () => {
  const dialog = readFileSync(new URL("../components/art/FrameworkManagementDialog.tsx", import.meta.url), "utf8");
  assert.match(dialog, /from "@neuro\/ui"/);
  assert.match(dialog, /<Dialog\s+open=\{open\}\s+title=\{t\("art\.action\.manageFrameworks"\)\}/);
  assert.doesNotMatch(dialog, /addEventListener|querySelectorAll|framework-dialog-backdrop/);
  for (const key of [
    "art.framework.column.framework",
    "art.framework.column.version",
    "art.framework.column.install",
    "art.framework.column.update",
  ]) {
    assert.ok(dialog.includes(`header: t("${key}")`), key);
  }
  assert.match(
    dialog,
    /<DataTable\s+label=\{t\("art\.framework\.tableLabel"\)\}\s+rows=\{frameworks\}\s+getRowKey=\{frameworkIdentity\}/,
  );
  assert.match(appSource, /accept="\.zip,application\/zip"/);
  assert.match(appSource, /upgradeFrameworkPackage\(baseUrl, identity, zipBase64\)/);
  assert.match(dialog, /if \(!next\) onClose\(\)/);
  assert.match(styleSource, /\.n-dialog\.loom-framework-dialog \{[\s\S]*?overflow: hidden;/);
});

test("starts framework installs immediately and confirms uninstall inside the dialog", () => {
  const toggleFrameworkSource = appSource.match(
    /const toggleFramework = async \(framework: LoomFramework\) => \{[\s\S]*?\n  \};/,
  );
  assert.ok(toggleFrameworkSource);
  assert.doesNotMatch(toggleFrameworkSource[0], /window\.confirm/);
  assert.match(appSource, /const \[pendingUninstallId, setPendingUninstallId\] = useState<string \| null>\(null\)/);
  assert.match(appSource, /if \(!framework\.installed\) \{\s*void onToggle\(framework\);\s*return;/);
  assert.match(appSource, /\? t\("art\.framework\.confirmUninstall"\)/);
  assert.match(appSource, /disabled=\{busyId !== null\}/);
});

test("moves Art workspace focus with arrow, Home, and End keys", () => {
  const count = artWorkspaceItems.length;
  assert.equal(nextArtWorkspaceIndex("ArrowRight", 2, count), 0);
  assert.equal(nextArtWorkspaceIndex("ArrowLeft", 0, count), 2);
  assert.equal(nextArtWorkspaceIndex("Home", 2, count), 0);
  assert.equal(nextArtWorkspaceIndex("End", 1, count), 2);
  assert.equal(nextArtWorkspaceIndex("Enter", 1, count), null);
  assert.equal(nextArtWorkspaceIndex("ArrowRight", 0, 0), null);
});

test("keeps standard creator layout free from legacy shrink-to-content and decorative skins", () => {
  const legacy = readFileSync(new URL("../styles/workflow-studio.css", import.meta.url), "utf8");
  assert.doesNotMatch(legacy, /\.add-art-wizard\b/);
  const styles = readFileSync(new URL("../styles/art-wizard.css", import.meta.url), "utf8");
  assert.match(
    styles,
    /\.art-create-dialog__scroll,\s*\.add-art-wizard,\s*\.art-creator-panel \{[\s\S]*?display: flex;[\s\S]*?min-height: 0;[\s\S]*?overflow: hidden;/,
  );
  assert.doesNotMatch(styles, /align-self: start|::before|radial-gradient|(?:background|border-radius|box-shadow)\s*:/);
});
