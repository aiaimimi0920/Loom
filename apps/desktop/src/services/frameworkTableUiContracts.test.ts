import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { desktopStyleSource } from "./desktopStyleSource.ts";

const source = readFileSync(new URL("../components/art/FrameworkManagementDialog.tsx", import.meta.url), "utf8");
const owner = readFileSync(new URL("../components/art/ArtPanel.tsx", import.meta.url), "utf8");

test("framework loading and a completed empty catalog have distinct standard feedback", () => {
  assert.match(owner, /const \[frameworksLoading, setFrameworksLoading\] = useState\(true\)/);
  assert.match(owner, /const version = \+\+frameworkLoadVersion.current;\s*setFrameworksLoading\(true\)/);
  assert.match(owner, /finally \{\s*if \(version === frameworkLoadVersion.current\) setFrameworksLoading\(false\)/);
  assert.match(owner, /loading=\{frameworksLoading\}/);
  assert.match(source, /aria-busy=\{loading\}/);
  assert.match(source, /error \? null : loading \? <Notice>加载中…<\/Notice> : <EmptyState role="status" title="暂无框架" \/>/);
  assert.doesNotMatch(source, /<p[^>]*role="status"/);
});

test("framework rows use standard table semantics and domain identity", () => {
  assert.match(source, /<DataTable\s+label="框架列表"\s+rows=\{frameworks\}\s+getRowKey=\{frameworkIdentity\}/);
  assert.match(source, /key: "identity", header: "框架", rowHeader: true, cell: frameworkFilterLabel/);
  for (const header of ["版本", "安装", "更新"]) assert.ok(source.includes(`header: "${header}"`));
  assert.doesNotMatch(source, /<(?:table|thead|tbody|tr|th|td)\b|className="framework-dialog__table/);
  assert.doesNotMatch(desktopStyleSource, /\.framework-dialog__table/);
  assert.match(desktopStyleSource, /\.loom-framework-dialog__table > \.n-table-scroll\s*\{[^}]*min-height: 0;/);
});

test("framework operations keep explicit uninstall approval and stable busy labels", () => {
  assert.match(source, /if \(!framework\.installed\) \{\s*void onToggle\(framework\);\s*return;/);
  assert.match(source, /if \(!confirmingUninstall\) \{\s*setPendingUninstallId\(identity\);\s*return;/);
  assert.match(source, /setPendingUninstallId\(null\);\s*void onToggle\(framework\);/);
  assert.match(source, /loading=\{toggleBusy\}/);
  assert.match(source, /loading=\{upgradeBusy\}/);
  assert.match(source, /disabled=\{!framework\.installed \|\| busyId !== null\}/);
  assert.doesNotMatch(source, /处理中|更新中/);
  assert.match(source, /if \(!open\) setPendingUninstallId\(null\)/);
});

test("hidden package input retains same-file reselection and original file forwarding", () => {
  assert.match(source, /<input\s+hidden/);
  assert.equal((source.match(/<input\b/g) || []).length, 1);
  assert.match(source, /accept="\.zip,application\/zip"/);
  assert.match(source, /event\.currentTarget\.value = "";\s*if \(file\) void onUpgrade\(framework, file\);/);
  assert.match(source, /fileInputRefs\.current\[identity\]\?\.click\(\)/);
  assert.doesNotMatch(source, /createPortal|addEventListener|document\.body/);
});
