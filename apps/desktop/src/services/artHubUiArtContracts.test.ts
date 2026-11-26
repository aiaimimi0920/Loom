import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { currentLanguage, setLanguage, uiTextFor } from "../i18n/index.ts";
import { artWorkspaceItems } from "./artHubUi.ts";
import { appSource, styleSource } from "./artHubUiContractSource.ts";

const uiStyles = readFileSync(new URL(import.meta.resolve("@neuro/ui/styles.css")), "utf8");

test("exposes one Art navigation entry instead of separate registry and framework pages", () => {
  assert.match(appSource, /createNavigationItem\("registry",\s*"shell\.nav\.art",\s*"registry"\)|id:\s*"registry"[\s\S]{0,80}icon:\s*"registry"/);
  assert.doesNotMatch(appSource, /createNavigationItem\("frameworks"|id:\s*"frameworks"/);
  assert.doesNotMatch(appSource, /activeSection === "frameworks"/);
  assert.match(appSource, /activeSection === "registry" && \(\s*<ArtPanel/);
});

test("removes Overview and Agents from the Loom shell", () => {
  assert.doesNotMatch(appSource, /createNavigationItem\("overview"|id:\s*"overview"|OverviewPanel/);
  assert.doesNotMatch(appSource, /createNavigationItem\("agents"|id:\s*"agents"|AgentsPanel/);
  assert.match(appSource, /useState<SectionId>\("mcp"\)/);
  assert.match(appSource, /<IconButton\s+className="app-titlebar__back"[\s\S]*?label=/);
});

test("keeps the Art workspace compact without a descriptive hero", () => {
  assert.doesNotMatch(appSource, /art-hub__hero/);
  assert.doesNotMatch(appSource, /Layer 2 · Art runtime/);
  assert.doesNotMatch(appSource, /<span className="mini-chip">\{visibleTools\.length\}<\/span>/);
});

test("removes the legacy Art registry and Python engine compatibility cards", () => {
  assert.doesNotMatch(appSource, /python_engine_status \/ prefetch_shader/);
  assert.doesNotMatch(appSource, />list_arts</);
  assert.doesNotMatch(appSource, />sync_user_arts</);
  assert.doesNotMatch(appSource, /pythonArts\.length/);
  assert.doesNotMatch(appSource, /const importPythonArt/);
  assert.match(appSource, /bootstrapPackagedArts\(snapshot\.baseUrl\)/);
});

test("renders compact Art cards with framework icons and icon-only actions", () => {
  const cardSource = appSource.match(/\{visibleTools\.map\(\(tool\) => \{[\s\S]*?<ArtEditDialog/);
  assert.ok(cardSource);
  assert.match(cardSource[0], /<Panel className="art-registry-entry" data-enabled=\{enabled\}/);
  assert.match(
    cardSource[0],
    /<Badge tone=\{enabled \? "success" : "neutral"\}>[\s\S]*?enabled \? t\("art\.status\.enabled"\) : t\("art\.status\.disabled"\)/,
  );
  assert.equal(uiTextFor("en", "art.status.enabled"), "Enabled");
  assert.equal(uiTextFor("en", "art.status.disabled"), "Disabled");
  assert.ok(uiTextFor("zh-Hans", "art.status.enabled").length > 0);
  assert.match(cardSource[0], /art-registry-card__framework-icon/);
  assert.match(cardSource[0], /<ArtIcon kind=\{artFrameworkIconKind\(frameworkReference\)\}/);
  assert.match(cardSource[0], /<ArtIcon kind="edit"/);
  assert.match(cardSource[0], /<ArtIcon kind="power"/);
  assert.match(cardSource[0], /<ArtIcon kind="trash"/);
  assert.doesNotMatch(cardSource[0], /<EnabledChip|className="card-kicker"|className="port-summary"/);
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
  assert.match(appSource, /<Dialog\s+open\s+title=\{t\("art\.action\.edit"\)\}/);
  assert.match(appSource, /await saveArtManagementSettings\(baseUrl, artManagement\.artId, input\)/);
  assert.match(appSource, /await updateArtToVersion\(baseUrl, artManagement\.artId, version\)/);
  assert.match(appSource, /aria-pressed=\{enabled\}/);
  assert.match(appSource, /<IconButton\s+label=\{tf\("art\.action\.editFor", \{ name: displayName \}\)\}/);
  assert.match(appSource, /label=\{tf\("art\.action\.deleteFor", \{ name: displayName \}\)\}/);
});

test("Art cards expose independent MCP dependency readiness and configuration", () => {
  assert.match(appSource, /resolveArtMcpDependencies\(tool, mcpServers\)/);
  for (const key of [
    "art.registry.mcpStatus.credentialsRequired",
    "art.registry.mcpStatus.disabled",
    "art.registry.mcpStatus.missing",
    "art.registry.mcpStatus.ready",
  ]) {
    assert.ok(appSource.includes(`t("${key}")`), key);
    assert.ok(uiTextFor("en", key).length > 0);
    assert.ok(uiTextFor("zh-Hans", key).length > 0);
  }
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
  assert.match(
    appSource,
    /<Dialog\s+open\s+title=\{t\("art\.action\.edit"\)\}[\s\S]*?busy=\{busy\}[\s\S]*?initialFocusRef=\{nameInputRef\}/,
  );
  assert.match(
    appSource,
    /<Panel\s+className="art-edit-dialog__section art-edit-dialog__identity"\s+aria-label=\{t\("art\.edit\.section\.identity"\)\}/,
  );
  for (const key of [
    "art.edit.field.name",
    "art.edit.field.description",
    "art.edit.autoUpdate",
    "art.edit.targetVersionAria",
  ]) {
    assert.ok(appSource.includes(`aria-label={t("${key}")}`), key);
  }
  assert.match(appSource, /<ArtPublisherIcon publisher=\{displayIdentity\.publisher\} \/>/);
  assert.match(appSource, /<strong>\{displayIdentity\.publisher\.name\}<\/strong>/);
  assert.match(appSource, /className="art-edit-dialog__english-name"[\s\S]*?\{displayIdentity\.englishName\}/);
  assert.match(appSource, /className="art-edit-dialog__id"[\s\S]*?\{displayIdentity\.globalId\}/);
  assert.match(appSource, /value=\{name\}[\s\S]*?disabled=\{busy \|\| !management\.canEditIdentity\}/);
  assert.match(appSource, /value=\{description\}[\s\S]*?disabled=\{busy \|\| !management\.canEditIdentity\}/);
  assert.match(appSource, /<Switch\s+aria-label=\{t\("art\.edit\.autoUpdate"\)\}[\s\S]*?checked=\{autoUpdate\}/);
  assert.match(
    appSource,
    /management\.updateAvailable[\s\S]*?<Badge tone="info" aria-label=\{t\("art\.edit\.updateAvailableAria"\)\}>/,
  );
  assert.match(appSource, /disabled=\{busy \|\| autoUpdate\}/);
  assert.match(
    appSource,
    /disabled=\{busy \|\| autoUpdate \|\| !targetVersion \|\| targetVersion === management\.currentVersion\}/,
  );
  assert.match(
    appSource,
    /<Disclosure\s+className="art-edit-dialog__optional"\s+title=\{<span aria-label=\{t\("art\.edit\.section\.optionalAria"\)\}>\{t\("art\.edit\.more"\)\}<\/span>\}/,
  );
  assert.match(appSource, /loading=\{busyAction === "update"\}/);
  assert.match(appSource, /loading=\{busyAction === "save"\}/);
  const start = appSource.indexOf("// Art-specific form composition");
  const end = appSource.indexOf("// Typed parameter");
  assert.ok(start >= 0 && end > start, "editor source boundaries must exist in order");
  const editor = appSource.slice(start, end);
  assert.doesNotMatch(editor, /framework-dialog|studio-input|document\.addEventListener|setTimeout|role="dialog"/);
  assert.doesNotMatch(editor, /<h3>|<summary|>new</);
});

test("Art parameter controls retain typed drafts, binding validation and secret ownership", () => {
  assert.match(appSource, /const \[valueBindings, setValueBindings\] = useState<Record<string, string>>\(\{\}\)/);
  assert.match(appSource, /const \[useGlobalValues, setUseGlobalValues\] = useState<Record<string, boolean>>\(\{\}\)/);
  for (const key of [
    "art.edit.param.referenceGlobalValueAria",
    "art.edit.param.referenceGlobalSecretAria",
    "art.edit.param.defaultSecretAria",
  ]) {
    assert.ok(appSource.includes(key), key);
  }
  assert.match(appSource, /<Checkbox[\s\S]*?label=\{t\("art\.edit\.param\.reference"\)\}/);
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
  assert.match(appSource, /label=\{t\("art\.action\.enable"\)\}/);
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

test("keeps concise localized labels and visibly separated tabs in the Art workspace", (context) => {
  const previousLanguage = currentLanguage();
  context.after(() => setLanguage(previousLanguage));

  assert.deepEqual(
    artWorkspaceItems.map((item) => item.id),
    ["registry", "store", "security"],
  );
  setLanguage("en");
  assert.deepEqual(
    artWorkspaceItems.map((item) => item.label),
    ["Registry", "Store", "Keys & Security"],
  );
  setLanguage("zh-Hans");
  assert.deepEqual(
    artWorkspaceItems.map((item) => item.label),
    [
      uiTextFor("zh-Hans", "art.workspace.registry"),
      uiTextFor("zh-Hans", "art.workspace.store"),
      uiTextFor("zh-Hans", "art.workspace.security"),
    ],
  );
  assert.equal(
    artWorkspaceItems.some((item) => "eyebrow" in item),
    false,
  );
  assert.match(appSource, /<TabList className="loom-art-tabs" label=\{t\("art\.workspace\.ariaLabel"\)\}>/);
  assert.match(appSource, /<Tab key=\{item\.id\} value=\{item\.id\}/);
  for (const item of artWorkspaceItems) {
    assert.match(appSource, new RegExp(`id="art-panel-${item.id}"`));
    assert.match(appSource, new RegExp(`aria-labelledby="art-tab-${item.id}"`));
    assert.match(appSource, new RegExp(`hidden=\\{activeWorkspace !== "${item.id}"\\}`));
    assert.match(appSource, new RegExp(`<TabPanel\\s+value="${item.id}"\\s+forceMount`));
  }
  assert.doesNotMatch(appSource, /art-panel-frameworks/);
  assert.match(
    styleSource,
    /\.loom-art-tabs \{\s*display: grid;\s*grid-template-columns: repeat\(3, minmax\(0, 1fr\)\);/,
  );
  assert.doesNotMatch(styleSource, /\.art-hub__tab(?:--|\s*\{)/);
});

test("trust and credentials use editable field rows plus persistent install policy", () => {
  assert.match(appSource, /credentialValueTypeLabels/);
  assert.match(appSource, /getRowKey=\{credentialFieldId\}/);
  assert.doesNotMatch(
    appSource,
    /function CredentialFieldDialog[\s\S]*?closeButtonRef\.current\?\.focus[\s\S]*?function PluginSecurityPanel/,
  );
  assert.doesNotMatch(appSource, /credential-field-dialog__more/);
  assert.match(appSource, /!credential\.scope\.frameworkId[\s\S]*?&& !credential\.scope\.artId/);
  assert.match(appSource, /scope: \{\}/);
  assert.doesNotMatch(appSource, /visibleSummaries\.map\([\s\S]*?revealPluginCredential/);
  assert.doesNotMatch(appSource, /<code title=\{credential\.value\}>\{credential\.value\}<\/code>/);
  assert.match(appSource, /••••••••/);
  assert.match(appSource, /const togglePrivateKey = async \(\) =>/);
  assert.match(appSource, /draft\.valueType === "boolean"/);
  assert.match(appSource, /draft\.valueType === "json"/);
  assert.match(appSource, /draft\.valueType === "string" \? "text" : "number"/);
  assert.match(appSource, /<Button variant="danger" disabled=\{busy\} onClick=\{onDelete\}>/);
  assert.match(appSource, /value="require_signed"/);
  assert.match(appSource, /value="require_trusted"/);
  assert.match(appSource, /value="allow_unsigned"/);
  assert.match(appSource, /trust\.policy === "require_trusted"/);
  assert.match(appSource, /placeholder="L0000000000"/);
  assert.match(appSource, /identity\?\.userId \?\? "L0000000000"/);
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
  assert.doesNotMatch(styleSource, /\.publisher-identity__secret \.studio-input/);
  assert.match(uiStyles, /\.n-toast-stack\s*\{[^}]*position: fixed;/);
  assert.match(uiStyles, /\.n-toast\s*\{[^}]*pointer-events: auto;/);
  assert.match(uiStyles, /\.n-toast\[data-exiting\]\s*\{[^}]*opacity: 0;/);
});

test("keeps framework filtering beside a modal management trigger", () => {
  assert.match(appSource, /<Toolbar className="framework-filter" role="group" aria-label=\{t\("art\.filter\.ariaLabel"\)\}>/);
  assert.match(appSource, /activeWorkspace === "registry" \? \(\s*<FrameworkFilter/);
  assert.match(appSource, /activeWorkspace === "store" \? \(\s*<FrameworkFilter/);
  assert.doesNotMatch(appSource, /<legend>/);
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
  for (const key of ["art.action.createArt", "art.action.manageFrameworks", "art.action.publishArt"])
    assert.ok(appSource.includes(`t("${key}")`), key);
  assert.match(
    appSource,
    /type="search"\s+aria-label=\{t\("art\.store\.searchAria"\)\}\s+placeholder=\{t\("art\.store\.searchPlaceholder"\)\}/,
  );
  assert.match(appSource, /aria-label=\{t\("art\.store\.officialOnlyAria"\)\}/);
  assert.match(appSource, /<Checkbox\s+className="loom-art-official"\s+label=\{t\("art\.store\.officialShort"\)\}/);
  assert.match(styleSource, /\.framework-filter__actions \{[\s\S]*?flex-wrap: wrap;/);
  assert.match(styleSource, /\.framework-filter \{[\s\S]*?border-bottom: 1px solid var\(--n-line\);/);
  assert.match(styleSource, /\.loom-art-store-search \{[\s\S]*?width: clamp\(120px, 14vw, 190px\);/);
  assert.match(styleSource, /\.loom-art-framework-picker \{\s*width: min\(240px, 100%\);/);
  assert.doesNotMatch(appSource, /editButtonRefs|selectAdjacentWorkspace/);
});

test("publishes only locally authored Arts from an accessible dialog", () => {
  assert.match(appSource, /<Dialog\s+open\s+title=\{t\("art\.action\.publishArt"\)\}[\s\S]*?busy=\{busyArtId !== null\}/);
  assert.match(appSource, /tools\.filter\(isLocallyAuthoredTool\)/);
  assert.match(appSource, /await publishArt\(baseUrl, tool\.id\)/);
  assert.match(appSource, /<EmptyState title=\{t\("art\.publish\.empty"\)\} \/>/);
  assert.match(appSource, /<Panel className="art-publish-dialog__row"/);
  assert.match(
    appSource,
    /loading=\{busyArtId === tool\.id\}[\s\S]*?disabled=\{busyArtId !== null\}[\s\S]*?>\s*\{t\("art\.action\.publish"\)\}\s*<\/Button>/,
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
  assert.doesNotMatch(appSource, /<p className="section-kicker">/);
  assert.match(appSource, /filterArtStoreEntries\(/);
  assert.match(appSource, /art\.official !== true &&\s*!\(await requestAppConfirmation/);
  assert.match(appSource, /title: t\("art\.store\.unverifiedConfirm\.title"\)/);
  assert.match(appSource, /tone: "warning"/);
  assert.doesNotMatch(appSource, /window\.confirm|globalThis\.confirm/);
  assert.match(appSource, /tf\("art\.store\.unverifiedConfirm\.message"/);
  assert.match(appSource, /art\.official === true \? t\("art\.store\.badge\.official"\) : t\("art\.store\.badge\.unverified"\)/);
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
