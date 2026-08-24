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

test("exposes one Art navigation entry instead of separate registry and framework pages", () => {
  assert.match(appSource, /id: "registry", label: "Art", eyebrow: ""/);
  assert.doesNotMatch(appSource, /id: "frameworks", label: "框架"/);
  assert.doesNotMatch(appSource, /activeSection === "frameworks"/);
  assert.match(appSource, /activeSection === "registry" && \(\s*<ArtPanel/);
});

test("removes Overview and Agents from the Loom shell", () => {
  assert.doesNotMatch(appSource, /id: "overview"|label: "总览"|OverviewPanel/);
  assert.doesNotMatch(appSource, /id: "agents"|label: "智能体"|AgentsPanel/);
  assert.match(appSource, /useState<SectionId>\("mcp"\)/);
  assert.match(appSource, /aria-label="返回 MCP"/);
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
  const cardSource = appSource.match(
    /\{visibleTools\.map\(\(tool\) => \{[\s\S]*?<ArtEditDialog/,
  );
  assert.ok(cardSource);
  assert.match(cardSource[0], /art-registry-card--enabled/);
  assert.match(cardSource[0], /art-registry-card--disabled/);
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
  assert.match(styleSource, /\.art-registry-card--enabled \{[\s\S]*?border-color: rgba\(42, 151, 91/);
  assert.match(styleSource, /\.art-registry-card--disabled \{[\s\S]*?border-color: rgba\(88, 97, 92/);
  assert.match(styleSource, /\.art-registry-card \{[\s\S]*?height: 210px;[\s\S]*?grid-template-rows: auto minmax\(0, 1fr\) auto;/);
  assert.match(styleSource, /\.art-registry-card__description \{[\s\S]*?overflow: hidden;[\s\S]*?-webkit-line-clamp: 2;/);
  assert.match(styleSource, /\.art-registry-card__actions \{[\s\S]*?grid-row: 3;/);
});

test("Art card edit and enable controls persist through dedicated management APIs", () => {
  assert.match(appSource, /const packageIdentity = artPackageIdentity\(tool\)/);
  assert.match(appSource, /await uninstallArtPackage\(baseUrl, packageIdentity, \{[\s\S]*?removeUnusedMcpServers:/);
  assert.match(appSource, /await deleteToolDefinition\(baseUrl, tool\.id\)/);
  assert.match(appSource, /saveToolDefinition\(baseUrl, \{ \.\.\.tool, enabled: nextEnabled \}\)/);
  assert.match(appSource, /role="dialog"\s+aria-modal="true"\s+aria-labelledby="art-edit-dialog-title"/);
  assert.match(appSource, /await saveArtManagementSettings\(baseUrl, artManagement\.artId, input\)/);
  assert.match(appSource, /await updateArtToVersion\(baseUrl, artManagement\.artId, version\)/);
  assert.match(appSource, /aria-pressed=\{enabled\}/);
  assert.match(appSource, /aria-label=\{`编辑 \$\{tool\.name \|\| tool\.id\}`\}/);
  assert.match(appSource, /aria-label=\{`删除 \$\{tool\.name \|\| tool\.id\}`\}/);
});

test("Art cards expose independent MCP dependency readiness and configuration", () => {
  assert.match(appSource, /resolveArtMcpDependencies\(tool, mcpServers\)/);
  assert.match(appSource, /需要配置 MCP 凭据/);
  assert.match(appSource, /MCP 依赖已禁用/);
  assert.match(appSource, /MCP 依赖未安装/);
  assert.match(appSource, /MCP 依赖就绪/);
  assert.match(appSource, /await updateMcpServerCredentials\(baseUrl, credentialServer\.id, values, clear\)/);
  assert.match(appSource, /<McpCredentialDialog[\s\S]*?server=\{credentialServer\}/);
  assert.match(styleSource, /\.art-registry-card__mcp-configuration \{[\s\S]*?var\(--loom-theme-warning\)/);
  assert.match(styleSource, /\.art-registry-card__mcp-state--ready \{[\s\S]*?var\(--loom-theme-success\)/);
  assert.match(styleSource, /\.art-registry-card__mcp-state--missing \{[\s\S]*?var\(--loom-theme-danger\)/);
});

test("Art editor keeps content while removing redundant field and section labels", () => {
  assert.match(appSource, /<h2 id="art-edit-dialog-title">编辑<\/h2>/);
  assert.doesNotMatch(appSource, />编辑 Art<\/h2>/);
  assert.match(appSource, /disabled=\{busy \|\| !management\.canEditIdentity\}/);
  assert.match(appSource, /aria-label="Art 信息"/);
  assert.match(appSource, /aria-label="名称"/);
  assert.match(appSource, /aria-label="描述"/);
  assert.match(appSource, /<ArtPublisherIcon publisher=\{displayIdentity\.publisher\} \/>/);
  assert.match(appSource, /<strong>\{displayIdentity\.publisher\.name\}<\/strong>/);
  assert.match(appSource, /className="art-edit-dialog__english-name"[\s\S]*?\{displayIdentity\.englishName\}/);
  assert.match(appSource, /\{displayIdentity\.globalId \? \([\s\S]*?className="art-edit-dialog__id"[\s\S]*?\{displayIdentity\.globalId\}/);
  assert.match(appSource, /value=\{name\}[\s\S]*?disabled=\{busy \|\| !management\.canEditIdentity\}/);
  assert.match(appSource, /value=\{description\}[\s\S]*?disabled=\{busy \|\| !management\.canEditIdentity\}/);
  assert.doesNotMatch(appSource, /<h3>基本信息<\/h3>/);
  assert.doesNotMatch(appSource, /<h3>版本<\/h3>/);
  assert.doesNotMatch(appSource, />当前 <strong>/);
  assert.doesNotMatch(appSource, />最高 <strong>/);
  assert.doesNotMatch(appSource, /management\.currentVersion\}\s*→\s*<strong/);
  assert.match(appSource, /className="art-edit-dialog__current-version"[\s\S]*?\{management\.currentVersion\}/);
  assert.doesNotMatch(appSource, /title="最高版本"/);
  assert.match(appSource, /aria-label="自动更新"/);
  assert.match(appSource, />自动更新<\/span>/);
  assert.match(appSource, /aria-label="目标版本"/);
  assert.match(appSource, /management\.updateAvailable[\s\S]*?className="art-edit-dialog__version-new"[\s\S]*?>new<\/span>/);
  assert.match(appSource, /disabled=\{busy \|\| autoUpdate\}/);
  assert.match(appSource, /disabled=\{busy \|\| autoUpdate \|\| !targetVersion \|\| targetVersion === management\.currentVersion\}/);
  assert.doesNotMatch(appSource, /<h3>必须参数<\/h3>/);
  assert.doesNotMatch(appSource, /<summary>可选参数<\/summary>/);
  assert.doesNotMatch(appSource, /<h3>机密参数<\/h3>/);
  assert.match(appSource, /<summary aria-label="可选参数">更多<\/summary>/);
  assert.match(appSource, /const \[valueBindings, setValueBindings\] = useState<Record<string, string>>\(\{\}\)/);
  assert.match(appSource, /const \[useGlobalValues, setUseGlobalValues\] = useState<Record<string, boolean>>\(\{\}\)/);
  assert.match(appSource, /aria-label=\{`\$\{parameter\.label\} 引用全局值`\}/);
  assert.match(appSource, /aria-label=\{`\$\{parameter\.label\} 引用全局机密`\}/);
  assert.match(appSource, /暂无匹配的全局值/);
  assert.match(appSource, /暂无匹配的全局机密/);
  assert.match(appSource, /credentialMatchesArtParameter\(parameter, credential\)/);
  assert.match(appSource, /case "number":[\s\S]*?valueType === "number" \|\| valueType === "integer"/);
  assert.match(appSource, /case "integer":[\s\S]*?valueType === "integer"/);
  assert.match(appSource, /case "boolean":[\s\S]*?valueType === "boolean"/);
  assert.match(appSource, /case "json":[\s\S]*?valueType === "json"/);
  assert.match(appSource, /const numericInput = parameter\.parameterType === "number" \|\| parameter\.parameterType === "integer"/);
  assert.match(appSource, /art-edit-dialog__value-input--number/);
  assert.match(appSource, /inputMode=\{numericInput \? parameter\.parameterType === "integer" \? "numeric" : "decimal" : undefined\}/);
  assert.doesNotMatch(appSource, /art-edit-dialog__number-mark/);
  assert.doesNotMatch(appSource, />123<\/span>/);
  assert.doesNotMatch(appSource, /<option value="global">引用全局机密<\/option>/);
  assert.doesNotMatch(appSource, /<option value="manual">直接填写<\/option>/);
  assert.match(appSource, /aria-label=\{`\$\{parameter\.label\} 的默认机密值`\}/);
  assert.doesNotMatch(appSource, /aria-label="机密参数 Key"/);
  assert.match(appSource, /secretValues\[parameter\.id\] = draft\.value/);
  assert.match(styleSource, /\.art-edit-dialog \{[\s\S]*?width: min\(920px, 100%\)/);
  assert.match(styleSource, /\.art-edit-dialog \{[\s\S]*?height: min\(720px, calc\(100vh - 32px\)\);[\s\S]*?flex-direction: column;/);
  assert.match(appSource, /<div className="art-edit-dialog__scroll">[\s\S]*?className="art-edit-dialog__overview"[\s\S]*?className=\{`art-edit-dialog__workspace/);
  assert.match(styleSource, /\.art-edit-dialog__form \{[\s\S]*?flex: 1 1 auto;[\s\S]*?overflow: hidden;/);
  assert.match(styleSource, /\.art-edit-dialog__scroll \{[\s\S]*?min-height: 0;[\s\S]*?align-content: start;[\s\S]*?grid-auto-rows: max-content;[\s\S]*?overflow-y: auto;[\s\S]*?scrollbar-width: thin;/);
  assert.match(styleSource, /\.art-edit-dialog__actions \{[\s\S]*?flex: 0 0 auto;/);
  assert.doesNotMatch(styleSource, /\.art-edit-dialog__actions \{[\s\S]*?position: sticky;/);
  assert.match(styleSource, /\.art-edit-dialog__overview \{[\s\S]*?align-content: start;[\s\S]*?grid-template-columns:[\s\S]*?grid-auto-rows: max-content;/);
  assert.match(styleSource, /\.art-edit-dialog__identity-meta \{[\s\S]*?grid-template-columns: auto minmax\(0, 1fr\) minmax\(120px, 0\.9fr\);/);
  assert.match(styleSource, /\.art-edit-dialog__publisher-icon \{[\s\S]*?width: 28px;[\s\S]*?height: 28px;/);
  assert.match(styleSource, /\.art-edit-dialog__version-primary \{[\s\S]*?justify-content: space-between;/);
  assert.match(styleSource, /\.art-edit-dialog__version-action \{[\s\S]*?grid-template-columns: minmax\(104px, 0\.82fr\) minmax\(116px, 1\.18fr\);/);
  assert.match(styleSource, /\.art-edit-dialog__version-new \{[\s\S]*?position: absolute;[\s\S]*?top: -8px;[\s\S]*?right: 8px;/);
  assert.match(styleSource, /\.art-edit-dialog__version-update \{[\s\S]*?padding-inline: 16px;/);
  assert.match(styleSource, /\.art-edit-dialog__workspace \{[\s\S]*?grid-template-columns:[\s\S]*?align-content: start;[\s\S]*?grid-auto-rows: max-content;/);
  assert.match(appSource, /className="art-edit-dialog__section art-edit-dialog__optional"[\s\S]*?art-edit-dialog__fields art-edit-dialog__fields--optional/);
  assert.match(styleSource, /\.art-edit-dialog__optional \{[\s\S]*?grid-column: 1 \/ -1;/);
  assert.match(styleSource, /\.art-edit-dialog__fields--optional \{[\s\S]*?repeat\(3, minmax\(0, 1fr\)\)/);
  assert.match(styleSource, /\.art-edit-dialog__parameter \{[\s\S]*?display: grid;/);
  assert.match(styleSource, /\.art-edit-dialog__parameter-head \{[\s\S]*?justify-content: space-between;/);
  assert.match(styleSource, /\.art-edit-dialog__binding-toggle input,[\s\S]*?accent-color:/);
  assert.match(styleSource, /\.art-edit-dialog__value-input--number \{[\s\S]*?box-shadow: inset -28px 0 0[\s\S]*?font-variant-numeric: tabular-nums;[\s\S]*?text-align: right;/);
  assert.match(styleSource, /\.art-edit-dialog__value-input--number::\-webkit-inner-spin-button,[\s\S]*?opacity: 1;[\s\S]*?cursor: pointer;/);
  assert.doesNotMatch(styleSource, /\.art-edit-dialog__number-mark/);
  assert.match(styleSource, /\.art-edit-dialog__boolean \{[\s\S]*?justify-self: stretch;[\s\S]*?justify-content: center;[\s\S]*?width: 100%;/);
  assert.match(styleSource, /@media \(max-width: 820px\) \{[\s\S]*?\.art-edit-dialog__overview,[\s\S]*?grid-template-columns: 1fr;/);
  assert.match(styleSource, /@media \(max-width: 820px\) \{[\s\S]*?\.art-edit-dialog__fields--optional \{[\s\S]*?repeat\(2, minmax\(0, 1fr\)\)/);
  assert.match(styleSource, /@media \(max-width: 720px\) \{[\s\S]*?\.art-edit-dialog__fields,[\s\S]*?\.art-edit-dialog__fields--optional \{[\s\S]*?grid-template-columns: 1fr;/);
});

test("keeps concise Chinese labels and visibly separated tabs in the Art workspace", () => {
  assert.deepEqual(artWorkspaceItems.map((item) => item.id), ["registry", "store", "security"]);
  assert.deepEqual(artWorkspaceItems.map((item) => item.label), ["注册表", "商店", "密钥与安全"]);
  assert.equal(artWorkspaceItems.some((item) => "eyebrow" in item), false);
  assert.match(appSource, /role="tablist"\s+aria-label="Art 工作区"/);
  for (const item of artWorkspaceItems) {
    assert.match(appSource, new RegExp(`id="art-panel-${item.id}"`));
    assert.match(appSource, new RegExp(`aria-labelledby="art-tab-${item.id}"`));
    assert.match(appSource, new RegExp(`hidden=\\{activeWorkspace !== "${item.id}"\\}`));
  }
  assert.doesNotMatch(appSource, /art-panel-frameworks/);
  assert.doesNotMatch(appSource, /执行框架/);
  assert.match(styleSource, /\.art-hub__tabs \{[\s\S]*?gap: 4px;[\s\S]*?border: 0;[\s\S]*?background: transparent;[\s\S]*?padding: 0 0 5px;/);
  assert.match(styleSource, /\.art-hub__tab \{[\s\S]*?border: 1px solid rgba\(255, 255, 255, 0\.14\);/);
  assert.match(styleSource, /\.art-hub__tab--active,[\s\S]*?background: var\(--loom-theme-surface\);[\s\S]*?color: var\(--loom-theme-accent-text\);/);
});

test("trust and credentials use editable field rows plus persistent install policy", () => {
  assert.match(appSource, /const credentialValueTypeLabels: Record<LoomCredentialValueType, string>/);
  assert.match(appSource, /string: "文本"/);
  assert.match(appSource, /number: "数字"/);
  assert.match(appSource, /integer: "整数"/);
  assert.match(appSource, /boolean: "布尔"/);
  assert.match(appSource, /json: "JSON"/);
  assert.doesNotMatch(appSource, /<h3>密钥字段<\/h3>/);
  assert.match(appSource, /aria-label="添加字段"/);
  assert.match(appSource, /credential-field-row credential-field-row--head[\s\S]*?<span>名字<\/span><span>格式<\/span><span>值<\/span>[\s\S]*?aria-label="添加字段"/);
  assert.match(appSource, /role="dialog" aria-modal="true" aria-labelledby="credential-field-dialog-title"/);
  assert.match(appSource, /<input autoFocus className="studio-input" aria-label="字段名字"/);
  assert.doesNotMatch(appSource, /function CredentialFieldDialog[\s\S]*?closeButtonRef\.current\?\.focus[\s\S]*?function PluginSecurityPanel/);
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
  assert.match(appSource, /className="danger-button" type="button" disabled=\{busy\} onClick=\{onDelete\}>删除/);
  assert.match(appSource, /<div className="security-policy-row">\s*<strong>安装策略<\/strong>/);
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
  assert.match(appSource, /entry\.level === "error" \? 5200 : entry\.level === "warning" \? 4200 : 3200/);
  assert.match(appSource, /onClick=\{\(\) => dismiss\(entry\.id\)\}/);
  assert.match(appSource, /entry\.leaving \? " app-toast--leaving" : ""/);
  assert.match(styleSource, /\.credential-field-list \{[\s\S]*?max-height: 320px;[\s\S]*?overflow-y: auto;/);
  assert.match(styleSource, /\.credential-field-row \{[\s\S]*?grid-template-columns:/);
  assert.match(styleSource, /\.trusted-user-library__list \{[\s\S]*?max-height: 180px;[\s\S]*?overflow-y: auto;/);
  assert.match(styleSource, /\.security-policy-row \{[\s\S]*?grid-template-columns: auto minmax\(0, 720px\);/);
  assert.match(styleSource, /\.publisher-identity__keys \{[\s\S]*?grid-template-columns: repeat\(2, minmax\(0, 1fr\)\);/);
  assert.match(appSource, /className="studio-input mono-line" readOnly type=\{showPrivateKey \? "text" : "password"\}/);
  assert.match(styleSource, /\.art-hub :is\(\.studio-input, \.studio-textarea, \.studio-json\) \{[\s\S]*?color: var\(--loom-theme-text\);[\s\S]*?-webkit-text-fill-color: var\(--loom-theme-text\);/);
  assert.match(styleSource, /\.publisher-identity__secret \.studio-input\[type="password"\] \{[\s\S]*?-webkit-text-fill-color: var\(--loom-theme-text\);[\s\S]*?opacity: 1;/);
  assert.match(styleSource, /\.art-hub \.publisher-identity label,[\s\S]*?color: var\(--loom-theme-muted\);/);
  assert.match(styleSource, /\.art-hub \.publisher-identity__id,[\s\S]*?color: var\(--loom-theme-text\);/);
  assert.match(styleSource, /\.app-toast-stack \{[\s\S]*?position: fixed;[\s\S]*?top: 64px;[\s\S]*?right: 18px;/);
  assert.match(styleSource, /\.app-toast \{[\s\S]*?transform: translateY\(var\(--toast-offset\)\);[\s\S]*?pointer-events: auto;/);
  assert.match(styleSource, /\.app-toast--error/);
  assert.match(styleSource, /\.app-toast--warning/);
  assert.match(styleSource, /\.app-toast--info/);
  assert.match(styleSource, /\.app-toast--leaving \{[\s\S]*?opacity: 0;/);
});

test("keeps framework filtering beside a modal management trigger", () => {
  assert.match(appSource, /<div className="framework-filter" role="group" aria-label="按框架筛选 Art">/);
  assert.match(appSource, /activeWorkspace === "registry" \? \(\s*<FrameworkFilter/);
  assert.match(appSource, /activeWorkspace === "store" \? \(\s*<FrameworkFilter/);
  assert.doesNotMatch(appSource, /<legend>框架<\/legend>/);
  assert.match(appSource, /checked=\{checked\}/);
  assert.match(appSource, /visibleTools\.map/);
  assert.match(appSource, /className="ghost-button framework-filter__create"/);
  assert.match(appSource, /className="ghost-button framework-filter__manage"/);
  assert.match(appSource, /className="ghost-button framework-filter__publish"/);
  assert.match(appSource, />\s*创建 Art\s*<\/button>/);
  assert.match(appSource, />\s*管理框架\s*<\/button>/);
  assert.match(appSource, />\s*发布 Art\s*<\/button>/);
  assert.match(appSource, /type="search"\s+aria-label="搜索 Art"\s+placeholder="搜索 Art"/);
  assert.match(appSource, /aria-label="只显示官方"/);
  assert.match(appSource, /<span title="只显示官方">官<\/span>/);
  assert.doesNotMatch(appSource, /<summary>管理框架<\/summary>/);
  assert.match(styleSource, /\.framework-filter \{[\s\S]*?flex-wrap: nowrap;/);
  assert.match(styleSource, /\.framework-filter \{[\s\S]*?border: 0;[\s\S]*?border-bottom: 1px solid var\(--loom-theme-line\);[\s\S]*?background: transparent;/);
  assert.match(styleSource, /\.framework-filter__search \{[\s\S]*?width: clamp\(120px, 14vw, 190px\);/);
  assert.match(styleSource, /\.framework-filter__options \{[\s\S]*?overflow-x: auto;/);
  assert.match(styleSource, /\.framework-filter__option \{[\s\S]*?flex: 0 0 auto;/);
  const filterRule = styleSource.match(/\.framework-filter__options \{([^}]*)\}/);
  assert.ok(filterRule);
  assert.doesNotMatch(filterRule[1], /flex-wrap/);
});

test("publishes only locally authored Arts from an accessible dialog", () => {
  assert.match(appSource, /role="dialog"\s+aria-modal="true"\s+aria-labelledby="art-publish-dialog-title"/);
  assert.match(appSource, /<h2 id="art-publish-dialog-title">发布 Art<\/h2>/);
  assert.match(appSource, /tools\.filter\(isLocallyAuthoredTool\)/);
  assert.match(appSource, /await publishArt\(baseUrl, tool\.id\)/);
  assert.match(appSource, /暂无本地创建的 Art/);
  assert.match(styleSource, /\.art-publish-dialog \{[\s\S]*?max-height: calc\(100vh - 32px\);/);
  assert.match(styleSource, /\.art-publish-dialog__list \{[\s\S]*?max-height: min\(500px, calc\(100vh - 150px\)\);[\s\S]*?overflow-y: auto;/);
});

test("keeps the Art store compact and warns before installing unverified packages", () => {
  assert.doesNotMatch(appSource, /<p className="section-kicker">art 商店<\/p>/);
  assert.doesNotMatch(appSource, /从商店安装 art/);
  assert.doesNotMatch(appSource, /商店地址（可留空）/);
  assert.match(appSource, /filterArtStoreEntries\(/);
  assert.match(appSource, /art\.official !== true && !await requestAppConfirmation/);
  assert.match(appSource, /title: "安装未认证 Art"/);
  assert.match(appSource, /tone: "warning"/);
  assert.doesNotMatch(appSource, /window\.confirm|globalThis\.confirm/);
  assert.match(appSource, /未经官方认证，可能包含恶意代码/);
  assert.match(appSource, /art\.official === true \? "官方" : "未认证"/);
  assert.match(styleSource, /\.art-store-card \{[\s\S]*?height: 210px;[\s\S]*?grid-template-rows: auto minmax\(0, 1fr\) auto;/);
});

test("uses one accessible in-app confirmation dialog for destructive and risky actions", () => {
  assert.match(appSource, /function requestAppConfirmation\(/);
  assert.match(appSource, /function AppConfirmViewport\(\)/);
  assert.match(appSource, /role="alertdialog"\s+aria-modal="true"/);
  assert.match(appSource, /event\.key === "Escape"/);
  assert.match(appSource, /event\.key !== "Tab"/);
  assert.match(appSource, /document\.body\.style\.overflow = "hidden"/);
  assert.match(appSource, /restoreFocusRef\.current\?\.focus\(\)/);
  assert.match(appSource, /<AppConfirmViewport \/>/);
  assert.match(styleSource, /\.app-confirm-backdrop \{[\s\S]*?z-index: 260;/);
  assert.match(styleSource, /\.app-confirm-dialog--danger/);
  assert.match(styleSource, /\.app-confirm-dialog--warning/);
});
