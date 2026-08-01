import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { artWorkspaceItems, nextArtWorkspaceIndex } from "./artHubUi.ts";

const appSource = readFileSync(new URL("../App.tsx", import.meta.url), "utf8");

test("exposes one Art navigation entry instead of separate registry and framework pages", () => {
  assert.match(appSource, /id: "registry", label: "Art", eyebrow: ""/);
  assert.doesNotMatch(appSource, /id: "frameworks", label: "框架"/);
  assert.doesNotMatch(appSource, /activeSection === "frameworks"/);
  assert.match(appSource, /activeSection === "registry" && \(\s*<ArtPanel/);
});

test("keeps the Art workspace compact without a descriptive hero", () => {
  assert.doesNotMatch(appSource, /art-hub__hero/);
  assert.doesNotMatch(appSource, /Art 运行与注册中心/);
  assert.doesNotMatch(appSource, /Layer 2 · Art runtime/);
  assert.doesNotMatch(appSource, /Art 状态摘要/);
});

test("keeps registry frameworks store and security inside the Art workspace", () => {
  assert.deepEqual(artWorkspaceItems.map((item) => item.id), ["registry", "frameworks", "store", "security"]);
  assert.match(appSource, /role="tablist" aria-label="Art 工作区"/);
  for (const item of artWorkspaceItems) {
    assert.match(appSource, new RegExp(`id="art-panel-${item.id}"`));
    assert.match(appSource, new RegExp(`aria-labelledby="art-tab-${item.id}"`));
    assert.match(appSource, new RegExp(`hidden=\\{activeWorkspace !== "${item.id}"\\}`));
  }
});

test("moves Art workspace focus with arrow, Home, and End keys", () => {
  const count = artWorkspaceItems.length;
  assert.equal(nextArtWorkspaceIndex("ArrowRight", 3, count), 0);
  assert.equal(nextArtWorkspaceIndex("ArrowLeft", 0, count), 3);
  assert.equal(nextArtWorkspaceIndex("Home", 2, count), 0);
  assert.equal(nextArtWorkspaceIndex("End", 1, count), 3);
  assert.equal(nextArtWorkspaceIndex("Enter", 1, count), null);
  assert.equal(nextArtWorkspaceIndex("ArrowRight", 0, 0), null);
});
