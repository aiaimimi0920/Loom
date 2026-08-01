import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import {
  artFrameworkReference,
  artWorkspaceItems,
  filterToolsByFrameworks,
  nextArtWorkspaceIndex,
} from "./artHubUi.ts";
import type { LoomFramework, LoomToolDefinition } from "./loomApi.ts";

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

test("keeps only Chinese registry store and security labels in the Art workspace", () => {
  assert.deepEqual(artWorkspaceItems.map((item) => item.id), ["registry", "store", "security"]);
  assert.equal(artWorkspaceItems.some((item) => "eyebrow" in item), false);
  assert.match(appSource, /role="tablist" aria-label="Art 工作区"/);
  for (const item of artWorkspaceItems) {
    assert.match(appSource, new RegExp(`id="art-panel-${item.id}"`));
    assert.match(appSource, new RegExp(`aria-labelledby="art-tab-${item.id}"`));
    assert.match(appSource, new RegExp(`hidden=\\{activeWorkspace !== "${item.id}"\\}`));
  }
  assert.doesNotMatch(appSource, /art-panel-frameworks/);
  assert.doesNotMatch(appSource, /执行框架/);
});

test("keeps framework filtering and management inside the registry", () => {
  assert.match(appSource, /<fieldset className="framework-filter" aria-label="按框架筛选 Art">/);
  assert.match(appSource, /checked=\{checked\}/);
  assert.match(appSource, /visibleTools\.map/);
  assert.match(appSource, /<summary>管理框架<\/summary>/);
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

const framework = (id: string, qualifiedId?: string): LoomFramework => ({
  id,
  qualifiedId,
  name: qualifiedId || id,
  description: "",
  installed: true,
  enabled: true,
  ready: true,
  readyDetail: "ready",
});

test("resolves authored, official, and third-party Art framework references", () => {
  assert.equal(artFrameworkReference({
    id: "authored",
    name: "Authored",
    execution: { type: "framework_art", framework: "fallback" },
    metadata: { dependencies: { framework: "neuro.official/script" } },
  }), "neuro.official/script");
  assert.equal(artFrameworkReference({
    id: "official",
    name: "Official",
    execution: { type: "python_art" },
  }), "python_art");
  assert.equal(artFrameworkReference({
    id: "third-party",
    name: "Third Party",
    execution: { type: "framework_art", framework: "publisher.alpha/shared" },
  }), "publisher.alpha/shared");
});

test("filters registry Arts by exact framework identity", () => {
  const frameworks = [
    framework("script", "neuro.official/script"),
    framework("shared", "publisher.alpha/shared"),
    framework("shared", "publisher.beta/shared"),
  ];
  const tools: LoomToolDefinition[] = [
    {
      id: "authored-script",
      name: "Authored Script",
      execution: { type: "framework_art", framework: "script" },
      metadata: { dependencies: { framework: "neuro.official/script" } },
    },
    { id: "legacy-script", name: "Legacy Script", execution: { type: "script" } },
    {
      id: "alpha-art",
      name: "Alpha Art",
      execution: { type: "framework_art", framework: "publisher.alpha/shared" },
    },
    { id: "ambiguous-art", name: "Ambiguous Art", execution: { type: "framework_art", framework: "shared" } },
    { id: "unclassified", name: "Unclassified", execution: { type: "manual" } },
  ];

  assert.deepEqual(
    filterToolsByFrameworks(tools, frameworks, new Set(["neuro.official/script"])).map((tool) => tool.id),
    ["authored-script", "legacy-script"],
  );
  assert.deepEqual(
    filterToolsByFrameworks(tools, frameworks, new Set(["publisher.alpha/shared"])).map((tool) => tool.id),
    ["alpha-art"],
  );
  assert.equal(filterToolsByFrameworks(tools, frameworks, new Set()).length, 0);
  assert.equal(filterToolsByFrameworks(tools, frameworks, null).length, tools.length);
});
