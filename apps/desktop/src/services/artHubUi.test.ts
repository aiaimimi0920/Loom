import assert from "node:assert/strict";
import test from "node:test";
import { currentLanguage, setLanguage, uiTextFor } from "../i18n/index.ts";
import {
  artDisplayIdentity,
  artDisplayLocale,
  artFrameworkReference,
  artPackageIdentity,
  artPublisherIconSource,
  filterArtStoreEntries,
  filterToolsByFrameworks,
  frameworkFilterLabel,
  isLocallyAuthoredTool,
  officialFrameworkDisplayName,
  toggleArtFrameworkFilter,
} from "./artHubUi.ts";
import type { LoomFramework, LoomToolDefinition } from "./loomApi.ts";

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

test("resolves localized Art identity and publisher metadata", (context) => {
  const previousLanguage = currentLanguage();
  context.after(() => setLanguage(previousLanguage));

  const zhSampleName = "\u793a\u4f8b Art";
  const zhSummary = "\u4e2d\u6587\u7b80\u8ff0";
  const tool: LoomToolDefinition = {
    id: "sample-art",
    name: "Fallback Name",
    description: "Fallback Description",
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
        names: { "zh-CN": zhSampleName, "en-US": "Sample Art" },
        descriptions: { "zh-CN": zhSummary, "en-US": "English summary" },
      },
    },
  };

  const chinese = artDisplayIdentity(tool, null, "zh-Hans-CN");
  assert.equal(chinese.locale, "zh-CN");
  assert.equal(chinese.publisher.name, "Neuro");
  assert.equal(chinese.publisher.initials, "N");
  assert.equal(chinese.englishName, "sample-art");
  assert.equal(chinese.globalId, "NA20260802999");
  assert.equal(chinese.localizedName, zhSampleName);
  assert.equal(chinese.localizedDescription, zhSummary);

  const english = artDisplayIdentity(tool, null, "en-GB");
  assert.equal(english.locale, "en-US");
  assert.equal(english.localizedName, "Sample Art");
  assert.equal(english.localizedDescription, "English summary");

  const anonymousTool: LoomToolDefinition = { id: "anon-art", name: "Anon" };
  setLanguage("en");
  assert.equal(artDisplayLocale(), "en-US");
  assert.equal(artDisplayIdentity(anonymousTool).publisher.name, uiTextFor("en", "art.localUser"));
  setLanguage("zh-Hans");
  assert.equal(artDisplayLocale(), "zh-CN");
  assert.equal(artDisplayIdentity(anonymousTool).publisher.name, uiTextFor("zh-Hans", "art.localUser"));
});

test("resolves installed Art packages by publisher-qualified identity", () => {
  assert.equal(
    artPackageIdentity({
      id: "sample-art",
      name: "Sample Art",
      metadata: {
        artPackage: { qualifiedId: "publisher.test/sample-art" },
      },
    }),
    "publisher.test/sample-art",
  );
  assert.equal(artPackageIdentity({ id: "compat-art", name: "Compat Art" }), null);
});

test("falls back safely for older Art metadata and publisher icons", () => {
  const localName = "\u672c\u5730\u5de5\u5177";
  const localDescription = "\u672c\u5730\u63cf\u8ff0";
  const tool: LoomToolDefinition = {
    id: "custom-local-tool",
    name: localName,
    description: localDescription,
    metadata: { authoring: { owner: "local-user" } },
  };
  const identity = artDisplayIdentity(tool, "local-user/custom-local-tool", "zh-CN");
  assert.equal(identity.publisher.name, "local-user");
  assert.equal(identity.englishName, "custom-local-tool");
  assert.equal(identity.globalId, null);
  assert.equal(identity.localizedName, localName);
  assert.equal(identity.localizedDescription, localDescription);
  assert.equal(artDisplayLocale("zh-TW"), "zh-CN");
  assert.equal(artDisplayLocale("fr-FR"), "en-US");
  assert.equal(artPublisherIconSource("N"), null);
  assert.equal(artPublisherIconSource("http://example.com/icon.png"), null);
  assert.equal(artPublisherIconSource("https://example.com/icon.png"), "https://example.com/icon.png");
  assert.equal(artPublisherIconSource("data:image/png;base64,AAAA"), "data:image/png;base64,AAAA");
});

test("uses unified official framework display names", (context) => {
  const previousLanguage = currentLanguage();
  context.after(() => setLanguage(previousLanguage));

  setLanguage("en");
  assert.equal(frameworkFilterLabel(framework("cloud_api", undefined, "Cloud API Framework")), "Cloud");
  assert.equal(frameworkFilterLabel(framework("mcp", undefined, "MCP Framework")), "MCP");
  assert.equal(frameworkFilterLabel(framework("process", undefined, "Local Process Framework")), "Script");
  assert.equal(frameworkFilterLabel(framework("workflow", undefined, "Workflow Framework")), "Workflow");
  assert.equal(frameworkFilterLabel(framework("process", "neuro.official/process", "Process Framework")), "Script");
  assert.equal(frameworkFilterLabel(framework("custom", undefined, "Custom Framework")), "Custom");
  assert.equal(
    frameworkFilterLabel(framework("process", "publisher.test/process", "\u7b2c\u4e09\u65b9\u8fdb\u7a0b\u6846\u67b6")),
    "\u7b2c\u4e09\u65b9\u8fdb\u7a0b",
  );

  setLanguage("zh-Hans");
  assert.equal(
    frameworkFilterLabel(framework("cloud_api", undefined, "Cloud API")),
    uiTextFor("zh-Hans", "art.framework.official.cloudApi"),
  );
  assert.equal(
    frameworkFilterLabel(framework("process", "neuro.official/process", "Process Framework")),
    uiTextFor("zh-Hans", "art.framework.official.process"),
  );
});

test("resolves official framework display names from Art references", (context) => {
  const previousLanguage = currentLanguage();
  context.after(() => setLanguage(previousLanguage));

  for (const language of ["en", "zh-Hans"] as const) {
    setLanguage(language);
    assert.equal(officialFrameworkDisplayName("cloud_api"), uiTextFor(language, "art.framework.official.cloudApi"));
    assert.equal(officialFrameworkDisplayName("neuro.official/mcp"), uiTextFor(language, "art.framework.official.mcp"));
    assert.equal(officialFrameworkDisplayName("process"), uiTextFor(language, "art.framework.official.process"));
    assert.equal(officialFrameworkDisplayName("workflow"), uiTextFor(language, "art.framework.official.workflow"));
    assert.equal(officialFrameworkDisplayName("publisher.test/process"), null);
  }
});

test("resolves authored, official, and third-party Art framework references", () => {
  assert.equal(
    artFrameworkReference({
      id: "authored",
      name: "Authored",
      execution: { type: "framework_art", framework: "fallback" },
      metadata: { dependencies: { framework: "neuro.official/process" } },
    }),
    "neuro.official/process",
  );
  assert.equal(
    artFrameworkReference({
      id: "official",
      name: "Official",
      execution: { type: "framework_art", framework: "process" },
    }),
    "process",
  );
  assert.equal(
    artFrameworkReference({
      id: "third-party",
      name: "Third Party",
      execution: { type: "framework_art", framework: "publisher.alpha/shared" },
    }),
    "publisher.alpha/shared",
  );
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
  assert.equal(
    isLocallyAuthoredTool({
      id: "local",
      name: "Local",
      metadata: { authoring: { origin: "local", owner: "local-user" } },
    }),
    true,
  );
  assert.equal(
    isLocallyAuthoredTool({
      id: "published",
      name: "Published",
      metadata: {
        authoring: { origin: "local", owner: "local-user" },
        packageSecurity: { publisher: { id: "publisher.example" } },
      },
    }),
    false,
  );
  assert.equal(isLocallyAuthoredTool({ id: "legacy", name: "Legacy" }), false);
});

test("framework toggle normalizes only the complete current identity set and leaves its input unchanged", () => {
  const ids = ["a", "b", "c"];
  assert.deepEqual(toggleArtFrameworkFilter(null, ids, "b"), new Set(["a", "c"]));
  const current = new Set(["a", "c"]);
  assert.equal(toggleArtFrameworkFilter(current, ids, "b"), null);
  assert.deepEqual(current, new Set(["a", "c"]));
  assert.deepEqual(toggleArtFrameworkFilter(new Set(["a"]), ids, "a"), new Set());
});

test("temporarily absent framework selection cannot erase an unrelated explicit exclusion", () => {
  const current = new Set(["a", "c", "absent"]);
  const withoutA = toggleArtFrameworkFilter(current, ["a", "b", "c"], "a");
  const restoredA = toggleArtFrameworkFilter(withoutA, ["a", "b", "c"], "a");
  assert.deepEqual(restoredA, new Set(["a", "c", "absent"]));
  assert.equal(restoredA?.has("b"), false);
  assert.equal(restoredA?.has("absent"), true);
  assert.deepEqual(current, new Set(["a", "c", "absent"]));
  assert.equal(toggleArtFrameworkFilter(restoredA, ["a", "b", "c", "absent"], "b"), null);
});

test("filters the Art store by framework search text and server-certified status", () => {
  const frameworks = [framework("process", "neuro.official/process"), framework("mcp", "neuro.official/mcp")];
  const entries = [
    {
      id: "official-script",
      qualifiedId: "neuro.official/official-script",
      globalId: "NA40000000000",
      name: "Image Compressor",
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
    filterArtStoreEntries(entries, frameworks, new Set(["neuro.official/process"]), "", false).map((entry) => entry.id),
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
