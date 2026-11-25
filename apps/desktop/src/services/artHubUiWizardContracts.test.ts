import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
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

test("opens local Art creation in a dedicated accessible dialog", () => {
  const dialog = readFileSync(new URL("../components/art/ArtCreationDialog.tsx", import.meta.url), "utf8");
  assert.match(appSource, /const \[createDialogOpen, setCreateDialogOpen\] = useState\(false\)/);
  assert.match(dialog, /from "@neuro\/ui"/);
  assert.match(dialog, /<Dialog\s+open=\{open\}\s+title="创建 Art"\s+busy=\{busy\}/);
  assert.match(dialog, /if \(!next\) onClose\(\)/);
  assert.doesNotMatch(dialog, /addEventListener|querySelectorAll|framework-dialog-backdrop|setTimeout/);
  assert.match(appSource, /<ArtCreationDialog[\s\S]*?<AddArtWizard/);
  assert.match(appSource, /await createAuthoredArtPackage\([\s\S]*?authored\.tool/);
  assert.match(appSource, /已创建并安装 Art \$\{derivedName\}/);
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
  assert.match(appSource, /<h4>参数<\/h4>/);
});

test("binds public workflow Art params to internal node params", () => {
  assert.match(appSource, /getWorkflowBundle\(baseUrl, workflowId\.trim\(\)\)/);
  assert.match(appSource, /collectWorkflowParamBindingCandidates\(workflow, tools\)/);
  assert.match(appSource, /title="绑定到流程节点参数"/);
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
  assert.match(appSource, /<RadioGroup\s+label="预览节点"\s+name=\{name\}/);
  assert.match(appSource, /<CheckboxGroup label="必要节点"/);
  assert.match(appSource, /<MultiSelect\s+label="必要节点"/);
  assert.match(appSource, /options\.length > 6/);
  assert.match(appSource, /disabled: option\.nodeId === output\?\.nodeId/);
  assert.match(appSource, /onNodeRequired\(option\.nodeId, next\.includes\(option\.nodeId\)\)/);
  assert.match(
    appSource,
    /setWorkflowPreviewOutput\(\{ nodeId: option\.nodeId, output: output\.name, kind: "node_result" \}\)/,
  );
  assert.match(appSource, /<Select value=\{selected\?\.nodeId \?\? ""\}/);
  assert.match(appSource, /<Field label="预览端口">/);
});

test("uses four concise Art creator categories with one active creation form", () => {
  const creatorSource = appSource.match(/function AddArtWizard\([\s\S]*?function ArtCreationDialog/);
  assert.ok(creatorSource);
  assert.match(appSource, /id: "cloud_api",\s*title: "云端"/);
  assert.match(appSource, /id: "mcp",\s*title: "MCP"/);
  assert.match(appSource, /id: "process",\s*title: "脚本"/);
  assert.match(appSource, /id: "workflow",\s*title: "流程"/);
  assert.match(appSource, /<TabsRoot\s+className="add-art-wizard"\s+value=\{mode\}/);
  assert.match(appSource, /<TabList label="Art 类型" className="loom-art-mode-tabs">/);
  assert.match(appSource, /artWizardModes\.map\(\(item\) => \(\s*<Tab key=\{item\.id\} value=\{item\.id\}/);
  assert.match(appSource, /<TabPanel className="art-creator-config" value=\{mode\}/);
  assert.match(appSource, /\{item\.title\}/);
  assert.doesNotMatch(appSource, /<strong>\{item\.title\}<\/strong>/);
  assert.doesNotMatch(appSource, /\{item\.subtitle\}[\s\S]*\{item\.executionLabel\}/);
  for (const mode of ["cloud_api", "mcp", "process", "workflow"]) {
    assert.match(appSource, new RegExp(`mode === "${mode}" \\?\\s*(?:\\(\\s*)?<`));
  }
  assert.match(appSource, /<form\s+className="art-creator-panel"/);
  assert.match(appSource, /<Field label="仓库名称">\s*<Input\s+value=\{repositoryName\}/);
  assert.match(appSource, /<Field label="Art 名称">\s*<Input value=\{name\}/);
  assert.doesNotMatch(appSource, />\s*Art ID\s*<input/);
  assert.doesNotMatch(creatorSource[0], /repositoryName[\s\S]*globalId/);
  assert.match(appSource, /<Disclosure\s+className="art-creator-ports"/);
  assert.match(appSource, /scriptEntryKind === "python"/);
  assert.match(appSource, /value: "command", label: "命令"/);
  assert.doesNotMatch(appSource, /<h3>Python 源码导入<\/h3>/);
  assert.doesNotMatch(appSource, /<h3>导入为脚本工具<\/h3>/);
  assert.doesNotMatch(appSource, /打包为 Python Art/);
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
  assert.match(dialog, /<Dialog\s+open=\{open\}\s+title="管理框架"/);
  assert.doesNotMatch(dialog, /addEventListener|querySelectorAll|framework-dialog-backdrop/);
  for (const header of ["框架", "版本", "安装", "更新"]) assert.ok(dialog.includes(`header: "${header}"`));
  assert.match(dialog, /<DataTable\s+label="框架列表"\s+rows=\{frameworks\}\s+getRowKey=\{frameworkIdentity\}/);
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
  assert.match(appSource, /\? "确认卸载"/);
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
