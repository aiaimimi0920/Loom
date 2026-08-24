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

test("opens local Art creation in a dedicated accessible dialog", () => {
  assert.match(appSource, /const \[createDialogOpen, setCreateDialogOpen\] = useState\(false\)/);
  assert.match(appSource, /role="dialog"\s+aria-modal="true"\s+aria-labelledby="art-create-dialog-title"/);
  assert.match(appSource, /<h2 id="art-create-dialog-title">创建 Art<\/h2>/);
  assert.match(appSource, /<ArtCreationDialog[\s\S]*?<AddArtWizard/);
  assert.match(appSource, /await createAuthoredArtPackage\([\s\S]*?authored\.tool/);
  assert.match(appSource, /已创建并安装 Art \$\{derivedName\}/);
  assert.doesNotMatch(appSource, /<section className="main-board add-art-wizard"/);
  assert.doesNotMatch(appSource, /className="studio-grid art-create-dialog__source"/);
  assert.match(styleSource, /\.art-create-dialog \{[\s\S]*?width: min\(1120px, 100%\);[\s\S]*?height: min\(820px, calc\(100vh - 32px\)\);/);
  assert.match(styleSource, /\.art-create-dialog__scroll \{[\s\S]*?min-height: 0;[\s\S]*?overflow-y: auto;[\s\S]*?scrollbar-width: thin;/);
});

test("routes saved Hook workflows through the prefilled workflow Art creator", () => {
  assert.match(appSource, /const \[pendingArtCreationRequest, setPendingArtCreationRequest\] = useState<ArtCreationRequest \| null>\(null\)/);
  assert.match(appSource, /mode: "workflow",[\s\S]*?repositoryName: request\.tool\.id,[\s\S]*?workflowId: request\.workflowId,[\s\S]*?templateTool: request\.tool/);
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
  assert.match(appSource, /workflowParam,[\s\S]*?nodeId: port\.bindingNodeId,[\s\S]*?target: port\.bindingTarget,[\s\S]*?kind: "param"/);
});

test("configures necessary nodes and one preview output without growing the Art dialog", () => {
  assert.match(appSource, /collectWorkflowPreviewNodeOptions\(workflowGraph, tools\)/);
  assert.match(appSource, /previewOutput: Some|previewOutput/);
  assert.match(appSource, /previewRequiredNodes/);
  assert.match(appSource, /name="workflow-preview-output"/);
  assert.match(appSource, /aria-label=\{`\$\{option\.label\} 必要`\}/);
  assert.match(appSource, /setWorkflowPreviewOutput\(\{ nodeId: option\.nodeId, output: output\.name, kind: "node_result" \}\)/);
  assert.match(styleSource, /\.art-workflow-preview-policy__list \{[\s\S]*?max-height: 224px;[\s\S]*?overflow-y: auto;/);
  assert.match(styleSource, /\.art-workflow-preview-policy__row \{[\s\S]*?min-height: 42px;/);
});

test("uses four concise Art creator categories with one active creation form", () => {
  const creatorSource = appSource.match(/function AddArtWizard\([\s\S]*?function ArtCreationDialog/);
  assert.ok(creatorSource);
  assert.match(appSource, /id: "cloud_api",\s*title: "云端"/);
  assert.match(appSource, /id: "mcp",\s*title: "MCP"/);
  assert.match(appSource, /id: "process",\s*title: "脚本"/);
  assert.match(appSource, /id: "workflow",\s*title: "流程"/);
  assert.match(appSource, /<div className="art-mode-grid" role="tablist" aria-label="Art 类型">/);
  assert.match(appSource, /\{artWizardModes\.map\(\(item\) => \(/);
  assert.match(appSource, /aria-selected=\{mode === item\.id\}/);
  assert.match(appSource, /\{item\.title\}/);
  assert.doesNotMatch(appSource, /<strong>\{item\.title\}<\/strong>/);
  assert.doesNotMatch(appSource, /\{item\.subtitle\}[\s\S]*\{item\.executionLabel\}/);
  for (const mode of ["cloud_api", "mcp", "process", "workflow"]) {
    assert.match(appSource, new RegExp(`mode === "${mode}" \\? \\(`));
  }
  assert.match(appSource, /<form className="art-creator-panel"/);
  assert.match(appSource, />\s*仓库名称\s*<input className="studio-input" value=\{repositoryName\}/);
  assert.match(appSource, />\s*Art 名称\s*<input className="studio-input" value=\{name\}/);
  assert.doesNotMatch(appSource, />\s*Art ID\s*<input/);
  assert.doesNotMatch(creatorSource[0], /repositoryName[\s\S]*globalId/);
  assert.match(appSource, /<details className="art-creator-ports">/);
  assert.match(appSource, /scriptEntryKind === "python"/);
  assert.match(appSource, /scriptEntryKind === "command"/);
  assert.doesNotMatch(appSource, /<h3>Python 源码导入<\/h3>/);
  assert.doesNotMatch(appSource, /<h3>导入为脚本工具<\/h3>/);
  assert.doesNotMatch(appSource, /打包为 Python Art/);
  assert.match(styleSource, /\.art-mode-grid \{[\s\S]*?grid-template-columns: repeat\(4, minmax\(0, 1fr\)\);/);
  assert.match(styleSource, /\.art-mode-card \{[\s\S]*?min-height: 48px;/);
});

test("opens an accessible framework management dialog with version and package update actions", () => {
  assert.match(appSource, /role="dialog"\s+aria-modal="true"\s+aria-labelledby="framework-dialog-title"/);
  assert.match(appSource, /<th scope="col">框架<\/th>/);
  assert.match(appSource, /<th scope="col">版本<\/th>/);
  assert.match(appSource, /<th scope="col">安装<\/th>/);
  assert.match(appSource, /<th scope="col">更新<\/th>/);
  assert.match(appSource, /accept="\.zip,application\/zip"/);
  assert.match(appSource, /upgradeFrameworkPackage\(baseUrl, identity, zipBase64\)/);
  assert.match(appSource, /event\.key === "Escape"/);
  assert.match(styleSource, /\.framework-dialog-backdrop \{[\s\S]*?position: fixed;/);
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
