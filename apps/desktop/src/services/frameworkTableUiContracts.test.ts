import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { uiTextFor } from "../i18n/index.ts";
import { desktopStyleSource } from "./desktopStyleSource.ts";

const source = readFileSync(new URL("../components/art/FrameworkManagementDialog.tsx", import.meta.url), "utf8");
const owner = readFileSync(new URL("../components/art/ArtPanel.tsx", import.meta.url), "utf8");

test("framework loading and a completed empty catalog have distinct standard feedback", () => {
  assert.match(owner, /const \[frameworksLoading, setFrameworksLoading\] = useState\(true\)/);
  assert.match(owner, /const version = \+\+frameworkLoadVersion.current;\s*setFrameworksLoading\(true\)/);
  assert.match(owner, /finally \{\s*if \(version === frameworkLoadVersion.current\) setFrameworksLoading\(false\)/);
  assert.match(owner, /loading=\{frameworksLoading\}/);
  assert.match(source, /aria-busy=\{loading\}/);
  assert.match(source, /<Notice>\{t\("art\.framework\.loading"\)\}<\/Notice>/);
  assert.match(source, /<EmptyState role="status" title=\{t\("art\.framework\.empty"\)\} \/>/);
  assert.equal(uiTextFor("en", "art.framework.loading"), "Loading…");
  assert.ok(uiTextFor("zh-Hans", "art.framework.loading").length > 0);
  assert.doesNotMatch(source, /<p[^>]*role="status"/);
});

test("framework rows use standard table semantics and domain identity", () => {
  assert.match(source, /<DataTable\s+label=\{t\("art\.framework\.tableLabel"\)\}\s+rows=\{frameworks\}\s+getRowKey=\{frameworkIdentity\}/);
  assert.match(
    source,
    /key: "identity", header: t\("art\.framework\.column\.framework"\), rowHeader: true, cell: frameworkFilterLabel/,
  );
  for (const key of [
    "art.framework.column.version",
    "art.framework.column.install",
    "art.framework.column.update",
  ]) {
    assert.ok(source.includes(`header: t("${key}")`), key);
    assert.ok(uiTextFor("en", key).length > 0);
    assert.ok(uiTextFor("zh-Hans", key).length > 0);
  }
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
  assert.doesNotMatch(source, /toggleBusy \?|upgradeBusy \?/);
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
