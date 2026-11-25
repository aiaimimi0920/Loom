import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
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
const uiStyles = readFileSync(new URL(import.meta.resolve("@neuro/ui/styles.css")), "utf8");

test("exposes one Art navigation entry instead of separate registry and framework pages", () => {
  assert.match(appSource, /id: "registry", label: "Art", icon: "registry"/);
  assert.doesNotMatch(appSource, /id: "frameworks", label: "框架"/);
  assert.doesNotMatch(appSource, /activeSection === "frameworks"/);
  assert.match(appSource, /activeSection === "registry" && \(\s*<ArtPanel/);
});

test("removes Overview and Agents from the Loom shell", () => {
  assert.doesNotMatch(appSource, /id: "overview"|label: "总览"|OverviewPanel/);
  assert.doesNotMatch(appSource, /id: "agents"|label: "智能体"|AgentsPanel/);
  assert.match(appSource, /useState<SectionId>\("mcp"\)/);
  assert.match(appSource, /<IconButton\s+className="app-titlebar__back"[\s\S]*?label="返回 MCP"/);
});

test("keeps the Art workspace compact without a descriptive hero", () => {
  assert.doesNotMatch(appSource, /art-hub__hero/);
  assert.doesNotMatch(appSource, /Art 运行与注册中心/);
  assert.doesNotMatch(appSource, /Layer 2 · Art runtime/);
  assert.doesNotMatch(appSource, /Art 状态摘要/);
  assert.doesNotMatch(appSource, /Art \/ 工具注册表/);
  assert.doesNotMatch(appSource, /查看注册表 JSON/);
  assert.doesNotMatch(appSource, /已保存 Art \/ 工具定义/);
  assert.doesNotMatch(appSource, /<span className="mini-chip">\{visibleTools\.length\}<\/span>/);
  assert.doesNotMatch(appSource, /已删除工具 \$\{tool\.name \|\| tool\.id\}/);
});

test("removes the legacy Art registry and Python engine compatibility cards", () => {
  assert.doesNotMatch(appSource, /旧 Art 注册表别名/);
  assert.doesNotMatch(appSource, /ArtLoom 注册表兼容/);
  assert.doesNotMatch(appSource, /python_engine_status \/ prefetch_shader/);
  assert.doesNotMatch(appSource, />list_arts</);
  assert.doesNotMatch(appSource, />sync_user_arts</);
  assert.doesNotMatch(appSource, /暂无工具/);
  assert.doesNotMatch(appSource, /所选框架下暂无 Art/);
  assert.doesNotMatch(appSource, /Python Art 目录/);
  assert.doesNotMatch(appSource, /已安装 Python Art/);
  assert.doesNotMatch(appSource, /刷新 Python Art/);
  assert.doesNotMatch(appSource, /查看 Python Art JSON/);
  assert.doesNotMatch(appSource, /pythonArts\.length/);
  assert.doesNotMatch(appSource, /const importPythonArt/);
  assert.match(appSource, /bootstrapPackagedArts\(snapshot\.baseUrl\)/);
});

test("renders compact Art cards with framework icons and icon-only actions", () => {
  const cardSource = appSource.match(/\{visibleTools\.map\(\(tool\) => \{[\s\S]*?<ArtEditDialog/);
  assert.ok(cardSource);
  assert.match(cardSource[0], /<Panel className="art-registry-entry" data-enabled=\{enabled\}/);
  assert.match(cardSource[0], /<Badge tone=\{enabled \? "success" : "neutral"\}>\{enabled \? "已启用" : "已禁用"\}/);
  assert.match(cardSource[0], /art-registry-card__framework-icon/);
  assert.match(cardSource[0], /<ArtIcon kind=\{artFrameworkIconKind\(frameworkReference\)\}/);
  assert.match(cardSource[0], /<ArtIcon kind="edit"/);
  assert.match(cardSource[0], /<ArtIcon kind="power"/);
  assert.match(cardSource[0], /<ArtIcon kind="trash"/);
  assert.doesNotMatch(cardSource[0], /<EnabledChip/);
  assert.doesNotMatch(cardSource[0], /className="card-kicker"/);
  assert.doesNotMatch(cardSource[0], /className="port-summary"/);
  assert.doesNotMatch(cardSource[0], />输入 \{/);
  assert.doesNotMatch(cardSource[0], />输出 \{/);
  assert.doesNotMatch(cardSource[0], />参数 \{/);
  assert.doesNotMatch(cardSource[0], /删除工具/);
  assert.match(styleSource, /\.art-registry-entry\[data-enabled="true"\] \{\s*border-color: var\(--n-success\)/);
  assert.match(
    styleSource,
    /\.art-registry-entry,\s*\.art-market-entry \{[\s\S]*?min-height: 210px;[\s\S]*?grid-template-rows: auto minmax\(0, 1fr\) auto;/,
  );
  assert.match(
    styleSource,
    /\.art-registry-card__description \{[\s\S]*?overflow: hidden;[\s\S]*?-webkit-line-clamp: 2;/,
  );
  assert.match(styleSource, /\.art-registry-card__actions \{[\s\S]*?grid-row: 3;/);
});

test("Art card edit and enable controls persist through dedicated management APIs", () => {
  assert.match(appSource, /const packageIdentity = artPackageIdentity\(tool\)/);
  assert.match(appSource, /await uninstallArtPackage\(baseUrl, packageIdentity, \{[\s\S]*?removeUnusedMcpServers:/);
  assert.match(appSource, /await deleteToolDefinition\(baseUrl, tool\.id\)/);
  assert.match(appSource, /saveToolDefinition\(baseUrl, \{ \.\.\.tool, enabled: nextEnabled \}\)/);
  assert.match(appSource, /<Dialog\s+open\s+title="编辑"/);
  assert.match(appSource, /await saveArtManagementSettings\(baseUrl, artManagement\.artId, input\)/);
  assert.match(appSource, /await updateArtToVersion\(baseUrl, artManagement\.artId, version\)/);
  assert.match(appSource, /aria-pressed=\{enabled\}/);
  assert.match(appSource, /<IconButton\s+label=\{`编辑 \$\{tool\.name \|\| tool\.id\}`\}/);
  assert.match(appSource, /label=\{`删除 \$\{tool\.name \|\| tool\.id\}`\}/);
});

test("Art cards expose independent MCP dependency readiness and configuration", () => {
  assert.match(appSource, /resolveArtMcpDependencies\(tool, mcpServers\)/);
  assert.match(appSource, /需要配置 MCP 凭据/);
  assert.match(appSource, /MCP 依赖已禁用/);
  assert.match(appSource, /MCP 依赖未安装/);
  assert.match(appSource, /MCP 依赖就绪/);
  assert.match(appSource, /await updateMcpServerCredentials\(baseUrl, credentialServer\.id, values, clear\)/);
  assert.match(appSource, /<McpCredentialDialog[\s\S]*?server=\{credentialServer\}/);
  assert.match(
    appSource,
    /<Button\s+compact\s+wrap[\s\S]*?onClick=\{\(\) => setCredentialServer\(displayedMcpDependency\.server\)\}[\s\S]*?disabled=\{toolBusy \|\| credentialBusy\}/,
  );
  assert.match(
    appSource,
    /<Badge\s+tone=\{mcpStatus === "ready" \? "success" : mcpStatus === "missing" \? "danger" : "warning"\}/,
  );
  assert.doesNotMatch(styleSource, /\.art-registry-card__mcp-(?:configuration|state)/);
});

test("Art editor composes the standard modal and controls without redundant labels", () => {
  assert.match(appSource, /<Dialog\s+open\s+title="编辑"[\s\S]*?busy=\{busy\}[\s\S]*?initialFocusRef=\{nameInputRef\}/);
  assert.match(
    appSource,
    /<Panel className="art-edit-dialog__section art-edit-dialog__identity" aria-label="Art 信息"/,
  );
  for (const label of ["名称", "描述", "自动更新", "目标版本"]) assert.ok(appSource.includes(`aria-label="${label}"`));
  assert.match(appSource, /<ArtPublisherIcon publisher=\{displayIdentity\.publisher\} \/>/);
  assert.match(appSource, /<strong>\{displayIdentity\.publisher\.name\}<\/strong>/);
  assert.match(appSource, /className="art-edit-dialog__english-name"[\s\S]*?\{displayIdentity\.englishName\}/);
  assert.match(appSource, /className="art-edit-dialog__id"[\s\S]*?\{displayIdentity\.globalId\}/);
  assert.match(appSource, /value=\{name\}[\s\S]*?disabled=\{busy \|\| !management\.canEditIdentity\}/);
  assert.match(appSource, /value=\{description\}[\s\S]*?disabled=\{busy \|\| !management\.canEditIdentity\}/);
  assert.match(appSource, /<Switch\s+aria-label="自动更新"[\s\S]*?checked=\{autoUpdate\}/);
  assert.match(appSource, /management\.updateAvailable[\s\S]*?<Badge tone="info" aria-label="有新版本">/);
  assert.match(appSource, /disabled=\{busy \|\| autoUpdate\}/);
  assert.match(
    appSource,
    /disabled=\{busy \|\| autoUpdate \|\| !targetVersion \|\| targetVersion === management\.currentVersion\}/,
  );
  assert.match(
    appSource,
    /<Disclosure className="art-edit-dialog__optional" title=\{<span aria-label="可选参数">更多<\/span>\}>/,
  );
  assert.match(appSource, /loading=\{busyAction === "update"\}/);
  assert.match(appSource, /loading=\{busyAction === "save"\}/);
  const editor = appSource.slice(
    appSource.indexOf("// Art-specific form composition"),
    appSource.indexOf("// Typed parameter"),
  );
  assert.doesNotMatch(editor, /framework-dialog|studio-input|document\.addEventListener|setTimeout|role="dialog"/);
  assert.doesNotMatch(editor, /<h3>|<summary|>new<|编辑 Art/);
});

test("Art parameter controls retain typed drafts, binding validation and secret ownership", () => {
  assert.match(appSource, /const \[valueBindings, setValueBindings\] = useState<Record<string, string>>\(\{\}\)/);
  assert.match(appSource, /const \[useGlobalValues, setUseGlobalValues\] = useState<Record<string, boolean>>\(\{\}\)/);
  for (const label of ["引用全局值", "引用全局机密", "的默认机密值"]) assert.ok(appSource.includes(label));
  assert.match(appSource, /<Checkbox[\s\S]*?label="引用"/);
  assert.match(appSource, /credentialMatchesArtParameter\(parameter, credential\)/);
  assert.match(appSource, /case "number":[\s\S]*?valueType === "number" \|\| valueType === "integer"/);
  assert.match(appSource, /case "integer":[\s\S]*?valueType === "integer"/);
  assert.match(appSource, /case "boolean":[\s\S]*?valueType === "boolean"/);
  assert.match(appSource, /case "json":[\s\S]*?valueType === "json"/);
  assert.match(
    appSource,
    /inputMode=\{numericInput \? \(parameter\.parameterType === "integer" \? "numeric" : "decimal"\) : undefined\}/,
  );
  for (const bound of ["minimum", "maximum", "step"])
    assert.ok(appSource.includes(`typeof parameter.${bound} === "number"`));
  assert.match(appSource, /type="password"\s+autoComplete="new-password"/);
  assert.match(appSource, /secretValues\[parameter\.id\] = draft\.value/);
  assert.match(appSource, /management\.canEditIdentity \? \{ name: name\.trim\(\), description \} : \{\}/);
  assert.match(appSource, /disabled=\{busy \|\| usingGlobal\}/);
  assert.match(appSource, /disabled=\{busy \|\| !usingGlobal\}/);
  assert.match(appSource, /aria-hidden=\{usingGlobal\} inert=\{usingGlobal\}/);
  assert.match(appSource, /aria-hidden=\{!usingGlobal\} inert=\{!usingGlobal\}/);
  assert.match(appSource, /label="启用"/);
  assert.match(styleSource, /\.art-edit-dialog__value-slot > div \{\s*grid-area: 1 \/ 1;/);
  assert.match(styleSource, /\.art-edit-dialog__value-slot > \[data-active="false"\] \{\s*visibility: hidden;/);
  assert.match(styleSource, /\.art-edit-dialog__secret-status \{\s*min-height: 24px;/);
  assert.match(
    appSource,
    /className="art-edit-dialog__secret-status" data-active=\{showStoredStatus\} aria-hidden=\{!showStoredStatus\}/,
  );
  assert.match(styleSource, /\.art-edit-dialog__secret-status\[data-active="false"\] \{\s*visibility: hidden;/);
  assert.doesNotMatch(
    styleSource,
    /art-edit-dialog__(?:binding-toggle input|boolean|value-input--number|version-new|toggle input)/,
  );
});

test("Art editor keeps one bounded scroll region, fixed actions and responsive domain layout", () => {
  assert.match(
    styleSource,
    /\.n-dialog\.art-edit-dialog \{[\s\S]*?width: min\(920px, calc\(100vw - 32px\)\);[\s\S]*?height: min\(720px, calc\(100dvh - 32px\)\);/,
  );
  assert.match(styleSource, /\.art-edit-dialog__form \{[\s\S]*?flex: 1 1 auto;[\s\S]*?overflow: hidden;/);
  assert.match(
    styleSource,
    /\.art-edit-dialog__scroll \{[\s\S]*?min-height: 0;[\s\S]*?align-content: start;[\s\S]*?grid-auto-rows: max-content;[\s\S]*?overflow-y: auto;[\s\S]*?scrollbar-gutter: stable;/,
  );
  assert.match(styleSource, /\.art-edit-dialog__actions \{[\s\S]*?flex: 0 0 auto;/);
  assert.match(
    styleSource,
    /\.art-edit-dialog__overview \{[\s\S]*?grid-template-columns:[\s\S]*?grid-auto-rows: max-content;/,
  );
  assert.match(styleSource, /\.art-edit-dialog__publisher-icon \{[\s\S]*?width: 28px;[\s\S]*?height: 28px;/);
  assert.match(
    styleSource,
    /\.art-edit-dialog__numeric \{[\s\S]*?font-variant-numeric: tabular-nums;[\s\S]*?text-align: right;/,
  );
  assert.match(styleSource, /\.art-edit-dialog__optional \{[\s\S]*?grid-column: 1 \/ -1;/);
  assert.match(styleSource, /\.art-edit-dialog__fields--optional \{[\s\S]*?repeat\(3, minmax\(0, 1fr\)\)/);
  assert.match(
    styleSource,
    /@media \(max-width: 820px\) \{[\s\S]*?\.art-edit-dialog__overview,[\s\S]*?grid-template-columns: 1fr;/,
  );
  assert.match(
    styleSource,
    /@media \(max-width: 820px\) \{[\s\S]*?\.art-edit-dialog__fields--optional \{[\s\S]*?repeat\(2, minmax\(0, 1fr\)\)/,
  );
  assert.match(
    styleSource,
    /@media \(max-width: 720px\) \{[\s\S]*?\.art-edit-dialog__fields,[\s\S]*?\.art-edit-dialog__fields--optional \{[\s\S]*?grid-template-columns: 1fr;/,
  );
});

test("keeps concise Chinese labels and visibly separated tabs in the Art workspace", () => {
  assert.deepEqual(
    artWorkspaceItems.map((item) => item.id),
    ["registry", "store", "security"],
  );
  assert.deepEqual(
    artWorkspaceItems.map((item) => item.label),
    ["注册表", "商店", "密钥与安全"],
  );
  assert.equal(
    artWorkspaceItems.some((item) => "eyebrow" in item),
    false,
  );
  assert.match(appSource, /<TabList className="loom-art-tabs" label="Art 工作区"/);
  assert.match(appSource, /<Tab key=\{item\.id\} value=\{item\.id\}/);
  for (const item of artWorkspaceItems) {
    assert.match(appSource, new RegExp(`id="art-panel-${item.id}"`));
    assert.match(appSource, new RegExp(`aria-labelledby="art-tab-${item.id}"`));
    assert.match(appSource, new RegExp(`hidden=\\{activeWorkspace !== "${item.id}"\\}`));
    assert.match(appSource, new RegExp(`<TabPanel\\s+value="${item.id}"\\s+forceMount`));
  }
  assert.doesNotMatch(appSource, /art-panel-frameworks/);
  assert.doesNotMatch(appSource, /执行框架/);
  assert.match(
    styleSource,
    /\.loom-art-tabs \{\s*display: grid;\s*grid-template-columns: repeat\(3, minmax\(0, 1fr\)\);/,
  );
  assert.doesNotMatch(styleSource, /\.art-hub__tab(?:--|\s*\{)/);
});

test("trust and credentials use editable field rows plus persistent install policy", () => {
  assert.match(appSource, /const credentialValueTypeLabels: Record<LoomCredentialValueType, string>/);
  assert.match(appSource, /string: "文本"/);
  assert.match(appSource, /number: "数字"/);
  assert.match(appSource, /integer: "整数"/);
  assert.match(appSource, /boolean: "布尔"/);
  assert.match(appSource, /json: "JSON"/);
  assert.doesNotMatch(appSource, /<h3>密钥字段<\/h3>/);
  assert.match(appSource, /label="添加字段"/);
  assert.match(appSource, /<DataTable\s+label="全局凭据字段"/);
  for (const header of ["名字", "格式", "值"]) assert.ok(appSource.includes(`header: "${header}"`));
  assert.match(appSource, /getRowKey=\{credentialFieldId\}/);
  assert.match(appSource, /<Dialog\s+open\s+title=\{draft\.original \? "编辑字段" : "添加字段"\}/);
  assert.match(appSource, /<Input\s+ref=\{nameInput\}\s+aria-label="字段名字"/);
  assert.doesNotMatch(
    appSource,
    /function CredentialFieldDialog[\s\S]*?closeButtonRef\.current\?\.focus[\s\S]*?function PluginSecurityPanel/,
  );
  assert.doesNotMatch(appSource, /aria-label="框架作用域"/);
  assert.doesNotMatch(appSource, /aria-label="Art 作用域"/);
  assert.doesNotMatch(appSource, /aria-label="过期时间"/);
  assert.doesNotMatch(appSource, /credential-field-dialog__more/);
  assert.match(appSource, /!credential\.scope\.frameworkId[\s\S]*?&& !credential\.scope\.artId/);
  assert.match(appSource, /scope: \{\}/);
  assert.doesNotMatch(appSource, /visibleSummaries\.map\([\s\S]*?revealPluginCredential/);
  assert.doesNotMatch(appSource, /<code title=\{credential\.value\}>\{credential\.value\}<\/code>/);
  assert.match(appSource, /<code aria-label=\{`\$\{credential\.name\} 已安全保存`\}>••••••••<\/code>/);
  assert.match(appSource, /const togglePrivateKey = async \(\) =>/);
  assert.match(appSource, /draft\.valueType === "boolean"/);
  assert.match(appSource, /draft\.valueType === "json"/);
  assert.match(appSource, /draft\.valueType === "string" \? "text" : "number"/);
  assert.match(appSource, /<Button variant="danger" disabled=\{busy\} onClick=\{onDelete\}>\s*删除/);
  assert.match(appSource, /<Field label="安装策略">/);
  assert.match(appSource, /value="require_signed">安装签名认证成功的/);
  assert.match(appSource, /value="require_trusted">安装签名认证成功且在信任库中用户发布的 Art/);
  assert.match(appSource, /value="allow_unsigned">可安装无签名 Art/);
  assert.match(appSource, /trust\.policy === "require_trusted"/);
  assert.match(appSource, /placeholder="L0000000000"/);
  assert.match(appSource, /<strong>用户 ID：<\/strong>/);
  assert.match(appSource, /identity\?\.userId \?\? "L0000000000"/);
  assert.match(appSource, /重置密钥/);
  assert.doesNotMatch(appSource, /申请用户 ID/);
  assert.doesNotMatch(appSource, /更换密钥/);
  assert.doesNotMatch(appSource, /className="main-board plugin-security"/);
  assert.doesNotMatch(appSource, /className="glass-card security-section security-section--credentials"/);
  assert.match(appSource, /type AppToastLevel = "error" \| "warning" \| "info"/);
  assert.match(appSource, /function AppToastViewport/);
  assert.match(appSource, /setEntries\(\(current\) => \[\.\.\.current, entry\]\)/);
  assert.match(appSource, /<ToastViewport entries=\{notifications\} onRemove=\{remove\}/);
  assert.match(appSource, /message: entry\.text, tone: entry\.level === "error" \? "danger" : entry\.level/);
  assert.match(styleSource, /\.credential-field-list > \.n-table-scroll \{[\s\S]*?max-height: 320px;/);
  assert.match(uiStyles, /\.n-table-scroll\s*\{[^}]*overflow: auto/);
  assert.match(styleSource, /\.trusted-user-library__list \{[\s\S]*?max-height: 180px;[\s\S]*?overflow-y: auto;/);
  assert.match(styleSource, /\.security-policy-row \{[\s\S]*?width: min\(720px, 100%\);/);
  assert.match(
    styleSource,
    /\.publisher-identity__keys \{[\s\S]*?grid-template-columns: repeat\(2, minmax\(0, 1fr\)\);/,
  );
  assert.match(appSource, /<Input\s+className="mono-line"\s+readOnly\s+type=\{showPrivateKey \? "text" : "password"\}/);
  assert.match(uiStyles, /\.n-input\s*\{[^}]*background: var\(--n-control\);[^}]*color: var\(--n-text\);/);
  assert.match(appSource, /<Field label="私钥">/);
  assert.match(appSource, /<Field label="公钥">/);
  assert.doesNotMatch(styleSource, /\.publisher-identity__secret \.studio-input/);
  assert.match(uiStyles, /\.n-toast-stack\s*\{[^}]*position: fixed;/);
  assert.match(uiStyles, /\.n-toast\s*\{[^}]*pointer-events: auto;/);
  assert.match(uiStyles, /\.n-toast\[data-exiting\]\s*\{[^}]*opacity: 0;/);
});

test("keeps framework filtering beside a modal management trigger", () => {
  assert.match(appSource, /<Toolbar className="framework-filter" role="group" aria-label="按框架筛选 Art">/);
  assert.match(appSource, /activeWorkspace === "registry" \? \(\s*<FrameworkFilter/);
  assert.match(appSource, /activeWorkspace === "store" \? \(\s*<FrameworkFilter/);
  assert.doesNotMatch(appSource, /<legend>框架<\/legend>/);
  assert.match(appSource, /frameworks\.length > 6 \? \(\s*<MultiSelect/);
  assert.match(appSource, /values=\{selected\}/);
  assert.match(appSource, /<Checkbox[\s\S]*?checked=\{selected\.includes\(option\.value\)\}/);
  assert.match(
    appSource,
    /selected\.includes\(option\.value\) !== values\.includes\(option\.value\)\) onToggle\(option\.value\)/,
  );
  assert.match(appSource, /visibleTools\.map/);
  for (const action of ["create", "manage", "publish"])
    assert.match(appSource, new RegExp(`<Button\\s+className="framework-filter__${action}"`));
  for (const label of ["创建 Art", "管理框架", "发布 Art"])
    assert.match(appSource, new RegExp(`>\\s*${label}\\s*</Button>`));
  assert.match(appSource, /type="search"\s+aria-label="搜索 Art"\s+placeholder="搜索 Art"/);
  assert.match(appSource, /aria-label="只显示官方"/);
  assert.match(appSource, /<Checkbox\s+className="loom-art-official"\s+label="官"/);
  assert.doesNotMatch(appSource, /<summary>管理框架<\/summary>/);
  assert.match(styleSource, /\.framework-filter__actions \{[\s\S]*?flex-wrap: wrap;/);
  assert.match(styleSource, /\.framework-filter \{[\s\S]*?border-bottom: 1px solid var\(--n-line\);/);
  assert.match(styleSource, /\.loom-art-store-search \{[\s\S]*?width: clamp\(120px, 14vw, 190px\);/);
  assert.match(styleSource, /\.loom-art-framework-picker \{\s*width: min\(240px, 100%\);/);
  assert.doesNotMatch(appSource, /editButtonRefs|selectAdjacentWorkspace/);
});

test("publishes only locally authored Arts from an accessible dialog", () => {
  assert.match(appSource, /<Dialog\s+open\s+title="发布 Art"[\s\S]*?busy=\{busyArtId !== null\}/);
  assert.match(appSource, /tools\.filter\(isLocallyAuthoredTool\)/);
  assert.match(appSource, /await publishArt\(baseUrl, tool\.id\)/);
  assert.match(appSource, /暂无本地创建的 Art/);
  assert.match(appSource, /<Panel className="art-publish-dialog__row"/);
  assert.match(
    appSource,
    /loading=\{busyArtId === tool\.id\}[\s\S]*?disabled=\{busyArtId !== null\}[\s\S]*?>\s*发布\s*<\/Button>/,
  );
  assert.match(
    styleSource,
    /\.n-dialog\.loom-art-publish-dialog \{[\s\S]*?height: min\(600px, calc\(100dvh - 32px\)\);/,
  );
  assert.match(
    styleSource,
    /\.art-publish-dialog__list \{[\s\S]*?min-height: 0;[\s\S]*?overflow-y: auto;[\s\S]*?scrollbar-gutter: stable;/,
  );
});

test("keeps the Art store compact and warns before installing unverified packages", () => {
  assert.doesNotMatch(appSource, /<p className="section-kicker">art 商店<\/p>/);
  assert.doesNotMatch(appSource, /从商店安装 art/);
  assert.doesNotMatch(appSource, /商店地址（可留空）/);
  assert.match(appSource, /filterArtStoreEntries\(/);
  assert.match(appSource, /art\.official !== true &&\s*!\(await requestAppConfirmation/);
  assert.match(appSource, /title: "安装未认证 Art"/);
  assert.match(appSource, /tone: "warning"/);
  assert.doesNotMatch(appSource, /window\.confirm|globalThis\.confirm/);
  assert.match(appSource, /未经官方认证，可能包含恶意代码/);
  assert.match(appSource, /art\.official === true \? "官方" : "未认证"/);
  assert.match(appSource, /<Panel className="art-market-entry"/);
  assert.match(appSource, /loading=\{installing\}/);
});

test("publish feedback belongs to one modal session while its pending action stays locked", () => {
  const publishSource = readFileSync(new URL("../components/art/ArtPublishDialog.tsx", import.meta.url), "utf8");
  assert.match(publishSource, /const messageVersion = useRef\(0\)/);
  assert.match(publishSource, /const version = messageVersion\.current/);
  assert.equal((publishSource.match(/if \(version === messageVersion\.current\)\s*setMessage/g) ?? []).length, 2);
  assert.match(publishSource, /return \(\) => \{\s*messageVersion\.current \+= 1;\s*\};\s*\}, \[open\]\)/);
  assert.match(publishSource, /await publishArt\(baseUrl, tool\.id\);\s*await onPublished\(\)/);
  assert.match(publishSource, /finally \{\s*setBusyArtId\(null\)/);
  assert.doesNotMatch(publishSource, /document\.addEventListener|setTimeout|framework-dialog/);
});

test("registry edit reads cannot redirect another session's settings mutation", () => {
  const registry = readFileSync(new URL("../components/art/RegistryPanel.tsx", import.meta.url), "utf8");
  assert.match(registry, /const editLoadVersion = useRef\(0\)/);
  assert.match(registry, /const version = \+\+editLoadVersion\.current/);
  assert.equal((registry.match(/editLoadVersion\.current \+= 1/g) ?? []).length, 2);
  assert.match(registry, /await autoUpdateArts\(baseUrl\);\s*if \(version !== editLoadVersion\.current\) return;/);
  assert.match(registry, /if \(version === editLoadVersion\.current\) setArtManagement\(management\)/);
  assert.match(registry, /if \(version === editLoadVersion\.current\)\s*setEditError/);
  assert.match(registry, /finally \{\s*if \(version === editLoadVersion\.current\) setArtManagementLoading\(false\)/);
});

test("uses one accessible in-app confirmation dialog for destructive and risky actions", () => {
  assert.match(appSource, /function requestAppConfirmation\(/);
  assert.match(appSource, /function AppConfirmViewport\(\)/);
  assert.match(appSource, /<ConfirmDialog open title=\{active\.title\} description=\{active\.message\}/);
  assert.match(appSource, /focusResetKey=\{active\.id\} returnFocusRef=\{restoreFocusRef\}/);
  assert.match(appSource, /onOpenChange=\{open => \{ if \(!open\) settle\(false, optionSelected\); \}\}/);
  assert.match(appSource, /queueRef\.current\.forEach\(\(request\) => request\.resolve\(\{/);
  assert.match(appSource, /<AppConfirmViewport \/>/);
  assert.match(uiStyles, /\.n-overlay\s*\{[^}]*position: fixed;/);
  assert.match(uiStyles, /\.n-dialog\s*\{[^}]*max-height:/);
});
