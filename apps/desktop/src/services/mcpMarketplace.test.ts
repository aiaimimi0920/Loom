import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import {
  MCP_MARKET_SERVERS,
  buildMcpPaginationItems,
  buildMarketplaceServerConfig,
  findInstalledMcpServer,
  getMarketplaceHealth,
  isValidMcpRemoteUrl,
  mapRegistryResponseToMarketplace,
  mergeRegistryAndCuratedMarketplace,
  parseMcpKeyValueLines,
  type McpRegistryResponse,
} from "./mcpMarketplace.ts";

const hubSource = readFileSync(new URL("../components/mcp/McpHub.tsx", import.meta.url), "utf8");
const styleSource = readFileSync(new URL("../styles.css", import.meta.url), "utf8");

test("maps official Registry packages and remotes into selectable install options", () => {
  const response: McpRegistryResponse = {
    servers: [
      {
        server: {
          name: "io.example/search-mcp",
          title: "Example Search",
          description: "Search the public web.",
          repository: { url: "https://example.test/search", source: "example" },
          packages: [
            {
              registryType: "npm",
              identifier: "@example/search-mcp",
              version: "1.2.3",
              transport: { type: "stdio" },
              environmentVariables: [{ name: "EXAMPLE_TOKEN", isRequired: true }],
            },
          ],
          remotes: [
            {
              type: "streamable-http",
              url: "https://search.example.test/{tenant}/mcp",
              variables: { tenant: { isRequired: true } },
              headers: [{ name: "Authorization", isRequired: true, isSecret: true }],
            },
          ],
        },
        _meta: {
          "io.modelcontextprotocol.registry/official": {
            status: "active",
            isLatest: true,
          },
        },
      },
      {
        server: {
          name: "io.example/deprecated",
          packages: [{ registryType: "npm", identifier: "deprecated", transport: { type: "stdio" } }],
        },
        _meta: {
          "io.modelcontextprotocol.registry/official": { status: "deprecated", isLatest: true },
        },
      },
    ],
  };

  const servers = mapRegistryResponseToMarketplace(response);

  assert.equal(servers.length, 1);
  assert.equal(servers[0].id, "io.example/search-mcp");
  assert.equal(servers[0].command, "npx");
  assert.deepEqual(servers[0].args, ["-y", "@example/search-mcp@1.2.3"]);
  assert.deepEqual(servers[0].requiredEnvKeys, ["EXAMPLE_TOKEN"]);
  assert.equal(servers[0].sourceKind, "registry");
  assert.equal(servers[0].installOptions.length, 2);
  assert.equal(servers[0].installOptions[0].transport, "stdio");
  assert.equal(servers[0].installOptions[1].transport, "streamable-http");
  assert.deepEqual(servers[0].installOptions[1].requiredHeaderKeys, ["Authorization"]);
  assert.equal(servers[0].installOptions[1].requiresManualConfiguration, true);
});

test("prefers a real Registry localization matching the Loom language", () => {
  const response: McpRegistryResponse = {
    servers: [{
      server: {
        name: "io.example/localized",
        title: "Localized MCP",
        description: "English upstream description.",
        localizations: {
          "zh-Hans": { title: "本地化 MCP", description: "来自仓库的中文介绍。" },
          en: { title: "Localized MCP (English)", description: "Localized English description." },
        },
        packages: [{ registryType: "npm", identifier: "localized-mcp", transport: { type: "stdio" } }],
      },
    }],
  };

  const [chinese] = mapRegistryResponseToMarketplace(response, "zh-CN");
  const [english] = mapRegistryResponseToMarketplace(response, "en-US");
  const [fallback] = mapRegistryResponseToMarketplace(response, "ja-JP");

  assert.equal(chinese.name, "本地化 MCP");
  assert.equal(chinese.description, "来自仓库的中文介绍。");
  assert.equal(english.name, "Localized MCP (English)");
  assert.equal(english.description, "Localized English description.");
  assert.equal(fallback.name, "Localized MCP");
  assert.equal(fallback.description, "English upstream description.");
});

test("normalizes Registry repository transports into safe browser links", () => {
  const [server] = mapRegistryResponseToMarketplace({
    servers: [{
      server: {
        name: "io.example/source-link",
        repository: { url: "git+https://github.com/example/source-link.git" },
        packages: [{ registryType: "npm", identifier: "source-link-mcp", transport: { type: "stdio" } }],
      },
    }],
  });

  assert.equal(server.sourceUrl, "https://github.com/example/source-link.git");
});

test("builds compact, directly selectable MCP page numbers", () => {
  assert.deepEqual(buildMcpPaginationItems(1, 3), [1, 2, 3]);
  assert.deepEqual(buildMcpPaginationItems(1, 12), [1, 2, 3, 4, "end-ellipsis", 12]);
  assert.deepEqual(buildMcpPaginationItems(6, 12), [1, "start-ellipsis", 5, 6, 7, "end-ellipsis", 12]);
  assert.deepEqual(buildMcpPaginationItems(12, 12), [1, "start-ellipsis", 9, 10, 11, 12]);
});

test("preserves configured secrets when rebuilding an official Registry service", () => {
  const [marketItem] = mapRegistryResponseToMarketplace({
    servers: [{
      server: {
        name: "io.example/secure",
        title: "Secure MCP",
        packages: [{
          registryType: "npm",
          identifier: "@example/secure-mcp",
          transport: { type: "stdio" },
          environmentVariables: [{ name: "API_KEY", isRequired: true }],
        }],
      },
    }],
  });
  assert.ok(marketItem);

  const configured = buildMarketplaceServerConfig(marketItem, {
    id: marketItem.id,
    name: marketItem.name,
    command: "old-command",
    args: [],
    env: { API_KEY: "saved-secret" },
    enabled: false,
  });
  const health = getMarketplaceHealth(marketItem, configured);

  assert.equal(configured.command, "npx");
  assert.equal(configured.env?.API_KEY, "saved-secret");
  assert.equal(configured.enabled, false);
  assert.equal(health.configured, true);
  assert.equal(health.requiredEnvPresent, true);
  assert.deepEqual(health.tags.map((tag) => tag.label), ["已安装", "已禁用", "密钥已填"]);
});

test("ships a small reviewed Loom catalog instead of the public Registry firehose", () => {
  const [registryServer] = mapRegistryResponseToMarketplace({
    servers: [{ server: { name: "io.example/echo", packages: [{ registryType: "npm", identifier: "echo-mcp", transport: { type: "stdio" } }] } }],
  });
  assert.ok(registryServer);
  const merged = mergeRegistryAndCuratedMarketplace([registryServer, registryServer], MCP_MARKET_SERVERS);

  assert.equal(MCP_MARKET_SERVERS.length, 8);
  assert.ok(MCP_MARKET_SERVERS.every((server) => server.sourceKind === "curated"));
  assert.ok(MCP_MARKET_SERVERS.every((server) => server.sourceLabel === "Loom 精选"));
  assert.deepEqual(MCP_MARKET_SERVERS.map((server) => server.id), [
    "loom.curated/memory",
    "loom.curated/filesystem",
    "loom.curated/fetch",
    "loom.curated/git",
    "loom.curated/time",
    "loom.curated/sequential-thinking",
    "loom.curated/playwright",
    "loom.curated/github",
  ]);
  assert.equal(MCP_MARKET_SERVERS.find((server) => server.id === "loom.curated/filesystem")?.requiresManualConfiguration, true);
  assert.deepEqual(MCP_MARKET_SERVERS.find((server) => server.id === "loom.curated/github")?.requiredEnvKeys, ["GITHUB_PERSONAL_ACCESS_TOKEN"]);
  assert.equal(merged.filter((server) => server.id === registryServer.id).length, 1);
});

test("validates editable MCP key-value fields without discarding valid entries", () => {
  const parsed = parseMcpKeyValueLines([
    "# local comment",
    "API_KEY=secret=value",
    "missing-separator",
    " =missing-key",
    "EMPTY=",
  ].join("\n"));

  assert.deepEqual(parsed.values, {
    API_KEY: "secret=value",
    EMPTY: "",
  });
  assert.deepEqual(parsed.invalidLineNumbers, [3, 4]);
});

test("accepts only credential-free HTTP MCP URLs", () => {
  assert.equal(isValidMcpRemoteUrl("https://example.test/mcp"), true);
  assert.equal(isValidMcpRemoteUrl("http://127.0.0.1:3123/mcp"), true);
  assert.equal(isValidMcpRemoteUrl("https://user:secret@example.test/mcp"), false);
  assert.equal(isValidMcpRemoteUrl("file:///tmp/mcp"), false);
  assert.equal(isValidMcpRemoteUrl("not-a-url"), false);
});

test("matches installed Registry services by stable id instead of display name", () => {
  const sameName = {
    id: "local/custom",
    name: "Shared Display Name",
    command: "local-server",
    args: [],
  };
  const exactId = {
    id: "io.example/official",
    name: "Renamed Service",
    command: "official-server",
    args: [],
  };

  assert.equal(findInstalledMcpServer([sameName], { id: "io.example/official" }), undefined);
  assert.equal(findInstalledMcpServer([sameName, exactId], { id: "io.example/official" }), exactId);
});

test("presents MCP as service and store workspaces with real install actions", () => {
  assert.match(hubSource, /role="tablist" aria-label="MCP 工作区"/);
  assert.match(hubSource, /id: "services", label: "服务"/);
  assert.match(hubSource, /id: "store", label: "商店"/);
  assert.doesNotMatch(hubSource, /fetchMcpRegistry\(baseUrl/);
  assert.match(hubSource, /MCP_MARKET_SERVERS\.filter/);
  assert.match(hubSource, /await saveMcpServer\(baseUrl, server\)/);
  assert.match(hubSource, /await testInstalledMcpServer\(baseUrl, saved\.id\)/);
  assert.match(hubSource, />安装 MCP 包</);
  assert.match(hubSource, />添加手动配置</);
  assert.match(hubSource, />\s*链接添加\s*</);
  assert.match(hubSource, /mode: "link", server: createRemoteMcpDraft\(\)/);
  assert.match(hubSource, /transport: "streamable-http"/);
  assert.match(hubSource, /editor\?\.mode === "install" \|\| editor\?\.mode === "link"/);
  assert.match(hubSource, /aria-busy=\{busyMarketplaceId === marketItem\.id\}/);
  assert.doesNotMatch(hubSource, /图片搜索手工测试流|MCP 包兼容|安装命令预览/);
  assert.match(hubSource, /远程 · Streamable HTTP/);
  assert.match(hubSource, /findInstalledMcpServer\(servers, marketItem\)/);
  assert.match(hubSource, /已安装，但连接测试失败/);
  assert.match(hubSource, /Loom 精选/);
  assert.match(hubSource, /connectionLabel\.startsWith\(`\$\{categoryLabel\} ·`\)/);
  assert.doesNotMatch(hubSource, /registryProgressRef|loadMarketplace|继续载入|刷新 MCP 商店/);
  assert.match(hubSource, /buildMcpPaginationItems/);
  assert.match(hubSource, /aria-label="MCP 商店分页"/);
  assert.match(hubSource, /aria-current=\{item === resolvedMarketPage \? "page" : undefined\}/);
  assert.match(hubSource, /title=\{marketItem\.description\}/);
  assert.match(hubSource, /安装前需配置/);
  assert.match(hubSource, /if \(isTauri\(\)\)/);
  assert.match(hubSource, /invoke\("open_mcp_source_url", \{ url: marketItem\.sourceUrl \}\)/);
  assert.match(hubSource, /openMarketplaceSource\(marketItem\)/);
  assert.match(hubSource, /onClick=\{\(\) => void testServer\(configured\)\}/);
  assert.match(hubSource, /configuredSnapshot\?\.status === "success"/);
  assert.match(hubSource, /"已连接"/);
  assert.doesNotMatch(hubSource, /加载更多|mcp-hub__load-more/);
  assert.match(styleSource, /\.mcp-hub__tabs\s*\{[\s\S]*?grid-template-columns: repeat\(2/);
  assert.doesNotMatch(hubSource, /mcp-hub__notice/);
  assert.match(styleSource, /\.mcp-card-grid\s*\{[\s\S]*?repeat\(auto-fill, minmax\(min\(100%, 240px\), 1fr\)\)/);
  assert.match(styleSource, /\.mcp-service-card\.art-registry-card--enabled\s*\{[\s\S]*?var\(--loom-theme-panel\)/);
  assert.match(styleSource, /\.mcp-store-card \.art-store-card__description\s*\{[\s\S]*?var\(--loom-theme-text\)/);
  assert.match(styleSource, /\.mcp-hub__toolbar\s*\{[\s\S]*?flex-wrap: wrap;[\s\S]*?overflow-x: clip;/);
  assert.match(styleSource, /\.mcp-store-card \.art-store-card__description\s*\{[\s\S]*?min-height: 3em;[\s\S]*?-webkit-line-clamp: 2;/);
  assert.match(styleSource, /\.mcp-store-card__actions \.mcp-store-card__install\s*\{[\s\S]*?min-width: 56px;/);
  assert.match(styleSource, /\.mcp-hub__pagination\s*\{[\s\S]*?justify-content: space-between;/);
  assert.match(styleSource, /\.mcp-hub :is\(button, input, select, textarea\):focus-visible[\s\S]*?var\(--loom-theme-accent-text\)/);
  assert.match(styleSource, /@media \(prefers-reduced-motion: reduce\)[\s\S]*?\.mcp-busy-indicator/);
});

test("renders MCP packages as independently managed services", () => {
  assert.doesNotMatch(hubSource, /isArtManagedServer|由 Art 管理|只读 · 请在 Art 管理中配置/);
  assert.match(hubSource, /server\.source === "package"/);
  assert.match(hubSource, /await installMcpServerPackage\(baseUrl, btoa\(binary\)\)/);
  assert.match(hubSource, /await setMcpServerEnabled\(baseUrl, server\.id, enabled\)/);
  assert.match(hubSource, /await updateMcpServerCredentials\(baseUrl, server\.id, values, clear\)/);
  assert.match(hubSource, /await deleteMcpServer\(baseUrl, server\.id\)/);
  assert.match(hubSource, /被 \{server\.usageCount\} 个 Art 使用/);
  assert.match(hubSource, /凭据由 Loom CredentialStore 加密保存/);
});
