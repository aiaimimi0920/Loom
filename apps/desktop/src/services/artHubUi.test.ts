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
