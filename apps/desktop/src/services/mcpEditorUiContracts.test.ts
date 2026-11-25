import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const server = readFileSync(new URL("../components/mcp/McpServerDialog.tsx", import.meta.url), "utf8");
const credentials = readFileSync(new URL("../components/mcp/McpCredentialDialog.tsx", import.meta.url), "utf8");
const shortcuts = readFileSync(new URL("../components/settings/SettingsShortcutDialogs.tsx", import.meta.url), "utf8");
const mcpStyle = readFileSync(new URL("../styles/mcp-dialogs.css", import.meta.url), "utf8");
const shortcutsStyle = readFileSync(new URL("../styles/settings-shortcuts.css", import.meta.url), "utf8");
const hub = readFileSync(new URL("../components/mcp/McpHub.tsx", import.meta.url), "utf8");

test("MCP editor fields delegate labels and identity to standard Field", () => {
  for (const label of ["名称", "ID", "描述", "连接方式", "启动命令", "参数", "环境变量", "MCP 地址", "请求头"]) {
    assert.ok(server.includes(`<Field label="${label}">`), label);
  }
  for (const source of [server, credentials]) {
    assert.match(source, /\bField\b[^;]+from "@neuro\/ui"/);
    assert.doesNotMatch(source, /<label\b|mcp-server-dialog__warning/);
  }
  assert.match(credentials, /<Field label=\{`\$\{requirement\.label\}/);
  assert.match(credentials, /<EmptyState title="此 MCP 服务没有声明凭据槽位。"/);
  assert.doesNotMatch(
    mcpStyle,
    /\.mcp-server-dialog__(?:scroll|grid|credential)\s*>\s*label|\.mcp-server-dialog__warning/,
  );
});

test("all seven MCP diagnostics remain standard notices with stable IDs and field descriptions", () => {
  const suffixes = [
    "missing-environment",
    "unresolved-arguments",
    "missing-headers",
    "unresolved-url",
    "invalid-url",
    "invalid-environment",
    "invalid-headers",
  ];
  assert.equal((server.match(/<Notice id="mcp-server-/g) || []).length, suffixes.length);
  for (const suffix of suffixes) {
    const id = `mcp-server-${suffix}`;
    assert.ok(server.includes(`<Notice id="${id}" tone="warning">`), id);
    assert.ok((server.match(new RegExp(`"${id}"`, "g")) || []).length >= 3, `${id} links field, dialog and notice`);
  }
  assert.match(server, /aria-describedby=\{validationDescription\}/);
  assert.match(server, /aria-invalid=\{unresolvedArguments\}/);
  assert.match(server, /invalidEnvironmentLines\.length > 0 \|\| missingEnvironmentKeys\.length > 0/);
  assert.match(server, /invalidHeaderLines\.length > 0 \|\| missingHeaderKeys\.length > 0/);
  assert.match(server, /if \(!canSave \|\| busy\) return/);
});

test("loading keeps MCP action labels while mutation and secret handling remain application-owned", () => {
  for (const source of [server, credentials]) {
    assert.match(source, /type="submit" loading=\{busy\} disabled=\{!canSave\}/);
    assert.doesNotMatch(source, /busy \? "(?:处理中|保存中)"/);
  }
  assert.match(server, /editor\.mode === "install" \? "安装" : "保存"/);
  assert.match(credentials, />\s*保存凭据\s*<\/Button>/);
  assert.match(credentials, /void onSave\(enteredValues, \[\.\.\.clear\]\)/);
  assert.match(credentials, /type="password"/);
  assert.match(credentials, /disabled=\{clear\.has\(requirement\.id\)\}/);
});

test("quick binding uses standard Field without moving business key capture into the package", () => {
  assert.match(shortcuts, /<Field label="Art">/);
  assert.doesNotMatch(shortcuts + shortcutsStyle, /quick-binding-dialog__art/);
  assert.match(shortcuts, /<ShortcutSlotsEditor\s+value=\{quickBindingEditor\}/);
  assert.match(shortcuts, /onEscapeKeyDown=\{captureEscape\}/);
  assert.match(shortcutsStyle, /\.shortcut-capture-slot > span\s*\{/);
});

test("MCP success closes only after actions unlock and failed mutations keep the editor", () => {
  const persist = hub.slice(hub.indexOf("const persistServer ="), hub.indexOf("const installMarketplaceServer ="));
  assert.match(persist, /saved = await saveMcpServer\(baseUrl, server\)/);
  assert.match(persist, /catch \(error\) \{[\s\S]*?setBusyMarketplaceId\(null\);\s*return;/);
  assert.match(
    persist,
    /finally \{[\s\S]*?setBusyServerId\(null\);\s*setBusyMarketplaceId\(null\);[\s\S]*?setEditor\(null\);/,
  );
  assert.equal((persist.match(/setEditor\(null\)/g) || []).length, 1);
  const credentials = hub.slice(hub.indexOf("const saveCredentials ="), hub.indexOf("const normalizedServiceSearch ="));
  assert.match(credentials, /let updated = false;/);
  assert.match(
    credentials,
    /await updateMcpServerCredentials\(baseUrl, server\.id, values, clear\);\s*updated = true;/,
  );
  assert.match(
    credentials,
    /finally \{[\s\S]*?setBusyServerId\(null\);[\s\S]*?if \(updated\) setCredentialServer\(null\);/,
  );
});
