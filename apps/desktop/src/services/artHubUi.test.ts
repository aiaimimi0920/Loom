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

const appSource = readFileSync(new URL("../App.tsx", import.meta.url), "utf8");
const styleSource = readFileSync(new URL("../styles.css", import.meta.url), "utf8");

test("resolves localized Art identity and publisher metadata", () => {
  const tool: LoomToolDefinition = {
    id: "sample-art",
    name: "默认名称",
    description: "默认描述",
    metadata: {
      packageSecurity: {
        publisher: { id: "neuro.official", name: "Neuro", icon: "N" },
      },
      art: {
        qualifiedId: "neuro.official/sample-art",
        englishName: "sample-art",
        globalId: "NA20260802999",
      },
      localization: {
        defaultLocale: "en-US",
        names: { "zh-CN": "示例 Art", "en-US": "Sample Art" },
        descriptions: { "zh-CN": "中文简述", "en-US": "English summary" },
      },
    },
  };

  const chinese = artDisplayIdentity(tool, null, "zh-Hans-CN");
  assert.equal(chinese.locale, "zh-CN");
  assert.equal(chinese.publisher.name, "Neuro");
  assert.equal(chinese.publisher.initials, "N");
  assert.equal(chinese.englishName, "sample-art");
  assert.equal(chinese.globalId, "NA20260802999");
  assert.equal(chinese.localizedName, "示例 Art");
  assert.equal(chinese.localizedDescription, "中文简述");

  const english = artDisplayIdentity(tool, null, "en-GB");
  assert.equal(english.locale, "en-US");
  assert.equal(english.localizedName, "Sample Art");
  assert.equal(english.localizedDescription, "English summary");
});

test("resolves installed Art packages by publisher-qualified identity", () => {
  assert.equal(artPackageIdentity({
    id: "sample-art",
    name: "Sample Art",
    metadata: {
      artPackage: { qualifiedId: "publisher.test/sample-art" },
    },
  }), "publisher.test/sample-art");
  assert.equal(artPackageIdentity({ id: "compat-art", name: "Compat Art" }), null);
});

test("falls back safely for older Art metadata and publisher icons", () => {
  const tool: LoomToolDefinition = {
    id: "custom-local-tool",
    name: "本地工具",
    description: "本地描述",
    metadata: { authoring: { owner: "local-user" } },
  };
  const identity = artDisplayIdentity(tool, "local-user/custom-local-tool", "zh-CN");
  assert.equal(identity.publisher.name, "local-user");
  assert.equal(identity.englishName, "custom-local-tool");
  assert.equal(identity.globalId, null);
  assert.equal(identity.localizedName, "本地工具");
  assert.equal(identity.localizedDescription, "本地描述");
  assert.equal(artDisplayLocale("zh-TW"), "zh-CN");
  assert.equal(artDisplayLocale("fr-FR"), "en-US");
  assert.equal(artPublisherIconSource("N"), null);
  assert.equal(artPublisherIconSource("http://example.com/icon.png"), null);
  assert.equal(artPublisherIconSource("https://example.com/icon.png"), "https://example.com/icon.png");
  assert.equal(
    artPublisherIconSource("data:image/png;base64,AAAA"),
    "data:image/png;base64,AAAA",
  );
});

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
  assert.match(appSource, /await uninstallArtPackage\(baseUrl, packageIdentity\)/);
  assert.match(appSource, /await deleteToolDefinition\(baseUrl, tool\.id\)/);
  assert.match(appSource, /saveToolDefinition\(baseUrl, \{ \.\.\.tool, enabled: nextEnabled \}\)/);
  assert.match(appSource, /role="dialog"\s+aria-modal="true"\s+aria-labelledby="art-edit-dialog-title"/);
  assert.match(appSource, /await saveArtManagementSettings\(baseUrl, artManagement\.artId, input\)/);
  assert.match(appSource, /await updateArtToVersion\(baseUrl, artManagement\.artId, version\)/);
  assert.match(appSource, /aria-pressed=\{enabled\}/);
  assert.match(appSource, /aria-label=\{`编辑 \$\{tool\.name \|\| tool\.id\}`\}/);
  assert.match(appSource, /aria-label=\{`删除 \$\{tool\.name \|\| tool\.id\}`\}/);
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
  assert.match(appSource, /await saveArtLoomCompatSettings\(baseUrl, nextSettings\)/);
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

const framework = (id: string, qualifiedId?: string, name = qualifiedId || id): LoomFramework => ({
  id,
  qualifiedId,
  name,
  description: "",
  installed: true,
  enabled: true,
  ready: true,
  readyDetail: "ready",
});

test("uses unified official framework display names", () => {
  assert.equal(frameworkFilterLabel(framework("cloud_api", undefined, "云 API 框架")), "云端");
  assert.equal(frameworkFilterLabel(framework("mcp", undefined, "MCP Framework")), "MCP");
  assert.equal(frameworkFilterLabel(framework("process", undefined, "本地进程框架")), "脚本");
  assert.equal(frameworkFilterLabel(framework("workflow", undefined, "Workflow Framework")), "流程");
  assert.equal(frameworkFilterLabel(framework("process", "neuro.official/process", "Process Framework")), "脚本");
  assert.equal(frameworkFilterLabel(framework("custom", undefined, "自定义框架")), "自定义");
  assert.equal(frameworkFilterLabel(framework("process", "publisher.test/process", "第三方进程框架")), "第三方进程");
});

test("resolves official framework display names from Art references", () => {
  assert.equal(officialFrameworkDisplayName("cloud_api"), "云端");
  assert.equal(officialFrameworkDisplayName("neuro.official/mcp"), "MCP");
  assert.equal(officialFrameworkDisplayName("process"), "脚本");
  assert.equal(officialFrameworkDisplayName("workflow"), "流程");
  assert.equal(officialFrameworkDisplayName("publisher.test/process"), null);
});

test("resolves authored, official, and third-party Art framework references", () => {
  assert.equal(artFrameworkReference({
    id: "authored",
    name: "Authored",
    execution: { type: "framework_art", framework: "fallback" },
    metadata: { dependencies: { framework: "neuro.official/process" } },
  }), "neuro.official/process");
  assert.equal(artFrameworkReference({
    id: "official",
    name: "Official",
    execution: { type: "framework_art", framework: "process" },
  }), "process");
  assert.equal(artFrameworkReference({
    id: "third-party",
    name: "Third Party",
    execution: { type: "framework_art", framework: "publisher.alpha/shared" },
  }), "publisher.alpha/shared");
});

test("filters registry Arts by exact framework identity", () => {
  const frameworks = [
    framework("process", "neuro.official/process"),
    framework("shared", "publisher.alpha/shared"),
    framework("shared", "publisher.beta/shared"),
  ];
  const tools: LoomToolDefinition[] = [
    {
      id: "authored-process",
      name: "Authored Process",
      execution: { type: "framework_art", framework: "process" },
      metadata: { dependencies: { framework: "neuro.official/process" } },
    },
    { id: "process-art", name: "Process Art", execution: { type: "framework_art", framework: "process" } },
    {
      id: "alpha-art",
      name: "Alpha Art",
      execution: { type: "framework_art", framework: "publisher.alpha/shared" },
    },
    { id: "ambiguous-art", name: "Ambiguous Art", execution: { type: "framework_art", framework: "shared" } },
    { id: "unclassified", name: "Unclassified", execution: { type: "manual" } },
  ];

  assert.deepEqual(
    filterToolsByFrameworks(tools, frameworks, new Set(["neuro.official/process"])).map((tool) => tool.id),
    ["authored-process", "process-art"],
  );
  assert.deepEqual(
    filterToolsByFrameworks(tools, frameworks, new Set(["publisher.alpha/shared"])).map((tool) => tool.id),
    ["alpha-art"],
  );
  assert.equal(filterToolsByFrameworks(tools, frameworks, new Set()).length, 0);
  assert.equal(filterToolsByFrameworks(tools, frameworks, null).length, tools.length);
});

test("recognizes only unpublished locally authored Arts", () => {
  assert.equal(isLocallyAuthoredTool({
    id: "local",
    name: "Local",
    metadata: { authoring: { origin: "local", owner: "local-user" } },
  }), true);
  assert.equal(isLocallyAuthoredTool({
    id: "published",
    name: "Published",
    metadata: {
      authoring: { origin: "local", owner: "local-user" },
      packageSecurity: { publisher: { id: "publisher.example" } },
    },
  }), false);
  assert.equal(isLocallyAuthoredTool({ id: "legacy", name: "Legacy" }), false);
});

test("filters the Art store by framework search text and server-certified status", () => {
  const frameworks = [
    framework("process", "neuro.official/process"),
    framework("mcp", "neuro.official/mcp"),
  ];
  const entries = [
    {
      id: "official-script",
      qualifiedId: "neuro.official/official-script",
      globalId: "NA40000000000",
      name: "图像压缩",
      description: "Compress images",
      framework: "process",
      official: true,
    },
    {
      id: "community-search",
      qualifiedId: "community.tools/community-search",
      name: "Image Search",
      description: "Search images",
      framework: "mcp",
      official: false,
    },
  ];

  assert.deepEqual(
    filterArtStoreEntries(entries, frameworks, new Set(["neuro.official/process"]), "", false)
      .map((entry) => entry.id),
    ["official-script"],
  );
  assert.deepEqual(
    filterArtStoreEntries(entries, frameworks, null, "community", false).map((entry) => entry.id),
    ["community-search"],
  );
  assert.deepEqual(
    filterArtStoreEntries(entries, frameworks, null, "", true).map((entry) => entry.id),
    ["official-script"],
  );
  assert.equal(filterArtStoreEntries(entries, frameworks, new Set(), "", false).length, 0);
});
