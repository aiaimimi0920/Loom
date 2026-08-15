"use strict";

function configureLoopbackFixture() {
  const configured = String(process.env.LOOM_STOCK_API_TEST_BASE_URL || "").trim();
  if (!configured) return;

  const base = new URL(configured);
  const loopbackHosts = new Set(["127.0.0.1", "::1", "[::1]"]);
  if (
    base.protocol !== "http:" ||
    !loopbackHosts.has(base.hostname) ||
    base.username ||
    base.password ||
    base.search ||
    base.hash
  ) {
    throw new Error("LOOM_STOCK_API_TEST_BASE_URL must be an unauthenticated loopback HTTP URL");
  }

  const nativeFetch = globalThis.fetch.bind(globalThis);
  globalThis.fetch = (input, init) => {
    const target = typeof input === "string" || input instanceof URL ? String(input) : input.url;
    const proxy = new URL("proxy", base.href.endsWith("/") ? base : `${base.href}/`);
    proxy.searchParams.set("url", target);
    return nativeFetch(proxy, init);
  };
}

try {
  configureLoopbackFixture();
  const { runMcpServer } = require("./vendor/stock-api/dist/mcp/server.js");
  void runMcpServer().catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  });
} catch (error) {
  process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
  process.exitCode = 1;
}
