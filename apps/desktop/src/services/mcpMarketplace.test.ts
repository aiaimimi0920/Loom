import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import { currentLanguage, setLanguage, uiTextFor } from "../i18n/index.ts";
import { desktopStyleSource as styleSource } from "./desktopStyleSource.ts";
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

const hubSource = [
  "../components/mcp/McpHub.tsx",
  "../components/mcp/McpHubToolbar.tsx",
  "../components/mcp/McpServicesPanel.tsx",
  "../components/mcp/McpStorePanel.tsx",
  "../components/mcp/McpServerDialog.tsx",
  "../components/mcp/McpCredentialDialog.tsx",
]
  .map((sourcePath) => readFileSync(new URL(sourcePath, import.meta.url), "utf8"))
  .join("\n");
const credentialDialogSource = readFileSync(
  new URL("../components/mcp/McpCredentialDialog.tsx", import.meta.url),
  "utf8",
);

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
  const zhTitle = "\u672c\u5730\u5316 MCP";
  const zhDescription = "\u6765\u81ea\u4ed3\u5e93\u7684\u4e2d\u6587\u4ecb\u7ecd\u3002";
  const response: McpRegistryResponse = {
    servers: [
      {
        server: {
          name: "io.example/localized",
          title: "Localized MCP",
          description: "English upstream description.",
          localizations: {
            "zh-Hans": { title: zhTitle, description: zhDescription },
            en: { title: "Localized MCP (English)", description: "Localized English description." },
          },
          packages: [{ registryType: "npm", identifier: "localized-mcp", transport: { type: "stdio" } }],
        },
      },
    ],
  };

  const [chinese] = mapRegistryResponseToMarketplace(response, "zh-CN");
  const [english] = mapRegistryResponseToMarketplace(response, "en-US");
  const [fallback] = mapRegistryResponseToMarketplace(response, "ja-JP");

  assert.equal(chinese.name, zhTitle);
  assert.equal(chinese.description, zhDescription);
  assert.equal(english.name, "Localized MCP (English)");
  assert.equal(english.description, "Localized English description.");
  assert.equal(fallback.name, "Localized MCP");
  assert.equal(fallback.description, "English upstream description.");
});

test("normalizes Registry repository transports into safe browser links", () => {
  const [server] = mapRegistryResponseToMarketplace({
    servers: [
      {
        server: {
          name: "io.example/source-link",
          repository: { url: "git+https://github.com/example/source-link.git" },
          packages: [{ registryType: "npm", identifier: "source-link-mcp", transport: { type: "stdio" } }],
        },
      },
    ],
  });

  assert.equal(server.sourceUrl, "https://github.com/example/source-link.git");
});

test("builds compact, directly selectable MCP page numbers", () => {
  assert.deepEqual(buildMcpPaginationItems(1, 3), [1, 2, 3]);
  assert.deepEqual(buildMcpPaginationItems(1, 12), [1, 2, 3, 4, "end-ellipsis", 12]);
  assert.deepEqual(buildMcpPaginationItems(6, 12), [1, "start-ellipsis", 5, 6, 7, "end-ellipsis", 12]);
  assert.deepEqual(buildMcpPaginationItems(12, 12), [1, "start-ellipsis", 9, 10, 11, 12]);
});

test("preserves configured secrets when rebuilding an official Registry service", (context) => {
  const previousLanguage = currentLanguage();
  context.after(() => setLanguage(previousLanguage));

  const [marketItem] = mapRegistryResponseToMarketplace({
    servers: [
      {
        server: {
          name: "io.example/secure",
          title: "Secure MCP",
          packages: [
            {
              registryType: "npm",
              identifier: "@example/secure-mcp",
              transport: { type: "stdio" },
              environmentVariables: [{ name: "API_KEY", isRequired: true }],
            },
          ],
        },
      },
    ],
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

  for (const language of ["en", "zh-Hans"] as const) {
    setLanguage(language);
    const health = getMarketplaceHealth(marketItem, configured);
    assert.equal(configured.command, "npx");
    assert.equal(configured.env?.API_KEY, "saved-secret");
    assert.equal(configured.enabled, false);
    assert.equal(health.configured, true);
    assert.equal(health.requiredEnvPresent, true);
    assert.deepEqual(
      health.tags.map((tag) => tag.label),
      [
        uiTextFor(language, "mcp.health.installed"),
        uiTextFor(language, "mcp.status.disabled"),
        uiTextFor(language, "mcp.health.secretConfigured"),
      ],
    );
  }
});

test("ships a small reviewed Loom catalog instead of the public Registry firehose", (context) => {
  const previousLanguage = currentLanguage();
  context.after(() => setLanguage(previousLanguage));

  const [registryServer] = mapRegistryResponseToMarketplace({
    servers: [
      {
        server: {
          name: "io.example/echo",
          packages: [{ registryType: "npm", identifier: "echo-mcp", transport: { type: "stdio" } }],
        },
      },
    ],
  });
  assert.ok(registryServer);
  const merged = mergeRegistryAndCuratedMarketplace([registryServer, registryServer], MCP_MARKET_SERVERS);

  assert.equal(MCP_MARKET_SERVERS.length, 8);
  assert.ok(MCP_MARKET_SERVERS.every((server) => server.sourceKind === "curated"));
  for (const language of ["en", "zh-Hans"] as const) {
    setLanguage(language);
    assert.ok(
      MCP_MARKET_SERVERS.every((server) => server.sourceLabel === uiTextFor(language, "mcp.market.source.curated")),
    );
    const filesystem = MCP_MARKET_SERVERS.find((server) => server.id === "loom.curated/filesystem");
    assert.equal(filesystem?.args[2], uiTextFor(language, "mcp.catalog.filesystem.argAllowedDirectory"));
  }
  assert.deepEqual(
    MCP_MARKET_SERVERS.map((server) => server.id),
    [
      "loom.curated/memory",
      "loom.curated/filesystem",
      "loom.curated/fetch",
      "loom.curated/git",
      "loom.curated/time",
      "loom.curated/sequential-thinking",
      "loom.curated/playwright",
      "loom.curated/github",
    ],
  );
  assert.equal(
    MCP_MARKET_SERVERS.find((server) => server.id === "loom.curated/filesystem")?.requiresManualConfiguration,
    true,
  );
  assert.deepEqual(MCP_MARKET_SERVERS.find((server) => server.id === "loom.curated/github")?.requiredEnvKeys, [
    "GITHUB_PERSONAL_ACCESS_TOKEN",
  ]);
  assert.equal(merged.filter((server) => server.id === registryServer.id).length, 1);
});

test("validates editable MCP key-value fields without discarding valid entries", () => {
  const parsed = parseMcpKeyValueLines(
    ["# local comment", "API_KEY=secret=value", "missing-separator", " =missing-key", "EMPTY="].join("\n"),
  );

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

test("rejects unsafe Registry remotes before they reach install configuration", () => {
  const [server] = mapRegistryResponseToMarketplace({
    servers: [
      {
        server: {
          name: "io.example/remote-safety",
          remotes: [
            { type: "streamable-http", url: "javascript:alert(1)" },
            { type: "streamable-http", url: "file:///tmp/mcp" },
            { type: "streamable-http", url: "https://user:secret@example.test/mcp" },
            {
              type: "streamable-http",
              url: "https://safe.example.test/{tenant}/mcp",
              variables: { tenant: { isRequired: true } },
              headers: [{ name: "Authorization", isRequired: true, isSecret: true }],
            },
          ],
        },
      },
    ],
  });

  assert.ok(server);
  assert.equal(server.installOptions.length, 1);
  assert.equal(server.installOptions[0].url, "https://safe.example.test/{tenant}/mcp");
  assert.equal(server.installOptions[0].requiresManualConfiguration, true);
  assert.deepEqual(server.installOptions[0].requiredHeaderKeys, ["Authorization"]);
});

test("does not embed Registry secret variable values in remote URLs", () => {
  const [server] = mapRegistryResponseToMarketplace({
    servers: [
      {
        server: {
          name: "io.example/remote-secret",
          remotes: [
            {
              type: "streamable-http",
              url: "https://safe.example.test/{token}/mcp",
              variables: { token: { value: "registry-secret", isSecret: true } },
            },
          ],
        },
      },
    ],
  });

  assert.equal(server.installOptions[0].url, "https://safe.example.test/{token}/mcp");
  assert.equal(server.installOptions[0].requiresManualConfiguration, true);
  assert.doesNotMatch(server.installOptions[0].url, /registry-secret/);
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
  assert.match(hubSource, /<TabsRoot asChild value=\{activeWorkspace\}/);
  assert.match(hubSource, /<TabList label=\{t\("mcp\.workspace\.ariaLabel"\)\}>/);
  assert.match(hubSource, /<TabPanel\s+value="services"\s+forceMount/);
  assert.match(hubSource, /<TabPanel\s+value="store"\s+forceMount/);
  assert.match(hubSource, /id: "services", label: t\("mcp\.workspace\.services"\)/);
  assert.match(hubSource, /id: "store", label: t\("mcp\.workspace\.store"\)/);
  assert.doesNotMatch(hubSource, /fetchMcpRegistry\(baseUrl/);
  assert.match(hubSource, /MCP_MARKET_SERVERS\.filter/);
  assert.match(hubSource, /await saveMcpServer\(baseUrl, server\)/);
  assert.match(hubSource, /await testInstalledMcpServer\(baseUrl, saved\.id\)/);
  assert.match(hubSource, /t\("mcp\.toolbar\.installPackage"\)/);
  assert.match(hubSource, /t\("mcp\.toolbar\.addManual"\)/);
  assert.match(hubSource, /t\("mcp\.toolbar\.addByLink"\)/);
  assert.match(hubSource, /mode: "link", server: createRemoteMcpDraft\(\)/);
  assert.match(hubSource, /transport: "streamable-http"/);
  assert.match(hubSource, /editor\?\.mode === "install" \|\| editor\?\.mode === "link"/);
  assert.match(hubSource, /loading=\{busyMarketplaceId === marketItem\.id\}/);
  assert.match(hubSource, /t\("mcp\.installOption\.remoteHttp"\)/);
  assert.match(hubSource, /findInstalledMcpServer\(installedServers, marketItem\)/);
  assert.match(hubSource, /tf\("mcp\.toast\.installedTestFailed"/);
  assert.match(hubSource, /t\("mcp\.market\.source\.curated"\)/);
  assert.match(hubSource, /connectionLabel\.startsWith\(`\$\{categoryLabel\} ·`\)/);
  assert.doesNotMatch(hubSource, /registryProgressRef|loadMarketplace/);
  assert.match(hubSource, /buildMcpPaginationItems/);
  assert.match(hubSource, /aria-label=\{t\("mcp\.store\.paginationAria"\)\}/);
  assert.match(hubSource, /aria-current=\{item === resolvedMarketPage \? "page" : undefined\}/);
  assert.match(hubSource, /title=\{marketItem\.description\}/);
  assert.match(hubSource, /t\("mcp\.store\.configureBeforeInstall"\)/);
  assert.match(hubSource, /if \(isTauri\(\)\)/);
  assert.match(hubSource, /const sourceUrl = normalizeHttpsExternalUrl\(marketItem\.sourceUrl\)/);
  assert.match(hubSource, /invoke\("open_mcp_source_url", \{ url: sourceUrl \}\)/);
  assert.doesNotMatch(hubSource, /window\.open\(marketItem\.sourceUrl/);
  assert.match(hubSource, /openMarketplaceSource\(marketItem\)/);
  assert.match(hubSource, /onClick=\{\(\) => onTest\(configured\)\}/);
  assert.match(hubSource, /configuredSnapshot\?\.status === "success"/);
  assert.match(hubSource, /t\("mcp\.store\.connected"\)/);
  assert.doesNotMatch(hubSource, /mcp-hub__load-more/);
  assert.doesNotMatch(hubSource, /mcp-hub__tab(?:s|--active)|role="tab"/);
  assert.doesNotMatch(hubSource, /mcp-hub__notice/);
  assert.match(styleSource, /\.mcp-card-grid\s*\{[\s\S]*?repeat\(auto-fill, minmax\(min\(100%, 240px\), 1fr\)\)/);
  assert.match(hubSource, /<Panel[\s\S]*?className="mcp-card mcp-service-card"/);
  assert.match(hubSource, /data-state=\{enabled \? "enabled" : "disabled"\}/);
  assert.match(hubSource, /<Panel\s+className="mcp-card mcp-store-card"/);
  assert.doesNotMatch(hubSource, /glass-card|art-registry-card|art-store-card|mcp-service-card__test-dot/);
  assert.match(styleSource, /\.mcp-card\s*\{[\s\S]*?min-height: 210px;[\s\S]*?grid-template-rows: auto 1fr auto;/);
  assert.match(styleSource, /\.mcp-hub__toolbar\s*\{[\s\S]*?flex-wrap: wrap;[\s\S]*?overflow-x: clip;/);
  assert.match(styleSource, /\.mcp-card__description\s*\{[\s\S]*?min-height: 3em;[\s\S]*?-webkit-line-clamp: 2;/);
  assert.match(styleSource, /\.mcp-hub__pagination\s*\{[\s\S]*?justify-content: space-between;/);
  // Focus, loading geometry and reduced motion are owned by the consumed package.
  const controls = readFileSync(new URL(import.meta.resolve("@neuro/ui/styles.css")), "utf8");
  assert.match(controls, /\.neuro-root :where\(button,[^\n]*\):focus-visible \{ outline: 2px solid var\(--n-signal\)/);
  assert.match(controls, /prefers-reduced-motion/);
  assert.match(controls, /\.n-loading \.n-button-label\s*\{[^}]*opacity: 0/);
});

test("renders MCP packages as independently managed services", () => {
  assert.doesNotMatch(hubSource, /isArtManagedServer/);
  assert.match(hubSource, /server\.source === "package"/);
  assert.match(hubSource, /assertMcpPackageFileSize\(file\.size\)/);
  assert.match(hubSource, /await installMcpServerPackage\(baseUrl, encodeMcpPackageBytes\(bytes\)\)/);
  assert.match(hubSource, /await setMcpServerEnabled\(baseUrl, server\.id, enabled\)/);
  assert.match(hubSource, /await updateMcpServerCredentials\(baseUrl, server\.id, values, clear\)/);
  assert.match(hubSource, /await deleteMcpServer\(baseUrl, server\.id\)/);
  assert.match(hubSource, /tf\("mcp\.service\.usedByArts", \{ count: server\.usageCount \}\)/);
  assert.match(hubSource, /t\("mcp\.credentials\.notice"\)/);
  assert.match(
    credentialDialogSource,
    /<Dialog\s+open\s+title=\{t\("mcp\.credentials\.title"\)\}[\s\S]*?busy=\{busy\}\s+initialFocusRef=\{firstInputRef\}/,
  );
  assert.doesNotMatch(credentialDialogSource, /createPortal|document\.body\.style\.overflow|addEventListener\("keydown"/);
});
