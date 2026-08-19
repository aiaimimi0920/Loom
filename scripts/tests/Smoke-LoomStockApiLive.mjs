import { spawn } from "node:child_process";
import { createInterface } from "node:readline";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const defaultEntry = resolve(
  scriptDir,
  "../../mcp-server-packages/stock-api/runtime/stock-api-entry.js",
);
const entry = resolve(process.argv[2] || defaultEntry);
const child = spawn(process.execPath, [entry], {
  stdio: ["pipe", "pipe", "pipe"],
  windowsHide: true,
});
const pending = new Map();
let nextId = 1;
let stderr = "";

child.stderr.setEncoding("utf8");
child.stderr.on("data", (chunk) => {
  stderr = `${stderr}${chunk}`.slice(-4000);
});

createInterface({ input: child.stdout }).on("line", (line) => {
  let response;
  try {
    response = JSON.parse(line);
  } catch {
    return;
  }
  const request = pending.get(response.id);
  if (!request) return;
  pending.delete(response.id);
  clearTimeout(request.timeout);
  if (response.error) request.reject(new Error(JSON.stringify(response.error)));
  else request.resolve(response.result);
});

const send = (method, params, timeoutMillis = 45000) => new Promise((resolveRequest, rejectRequest) => {
  const id = nextId++;
  const timeout = setTimeout(() => {
    pending.delete(id);
    rejectRequest(new Error(`${method} timed out after ${timeoutMillis}ms`));
  }, timeoutMillis);
  pending.set(id, { resolve: resolveRequest, reject: rejectRequest, timeout });
  child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id, method, params })}\n`);
});

const structuredContent = (result, toolName) => {
  if (result?.isError) {
    const message = result?.content?.map((item) => item?.text).filter(Boolean).join(" ");
    throw new Error(`${toolName} failed: ${message || "unknown MCP error"}`);
  }
  if (!result?.structuredContent) throw new Error(`${toolName} returned no structuredContent`);
  return result.structuredContent;
};

try {
  const startedAt = Date.now();
  const initialized = await send("initialize", {
    protocolVersion: "2025-06-18",
    capabilities: {},
    clientInfo: { name: "loom-live-smoke", version: "1" },
  });
  child.stdin.write('{"jsonrpc":"2.0","method":"notifications/initialized"}\n');

  const quote = structuredContent(await send("tools/call", {
    name: "get_stock",
    arguments: { code: "SZ000034", source: "eastmoney" },
  }), "get_stock");
  const requestedPeriods = [
    ["minute", 240],
    ["five-day", 2000],
    ["day", 240],
    ["week", 120],
    ["quarter", 40],
  ];
  const liveSeries = [];
  for (const [period, count] of requestedPeriods) {
    const series = structuredContent(await send("tools/call", {
      name: "get_market_series",
      arguments: {
        code: "SZ000034",
        source: "eastmoney",
        period,
        count,
        adjust: "none",
      },
    }), `get_market_series:${period}`);
    const rows = series?.response?.klines;
    if (!Array.isArray(rows) || rows.length === 0 || !(rows.at(-1)?.close > 0)) {
      throw new Error(`invalid live ${period} series: ${JSON.stringify(series?.response)}`);
    }
    liveSeries.push({
      period: series.response.period,
      rows: rows.length,
      lastTradingDate: series.response.lastTradingDate,
      lastClose: rows.at(-1).close,
      source: rows.at(-1).source,
    });
  }

  const stock = quote?.response?.stock;
  if (initialized?.serverInfo?.version !== "2.8.0") {
    throw new Error(`unexpected wrapper version: ${initialized?.serverInfo?.version}`);
  }
  if (stock?.code !== "SZ000034" || stock?.source !== "eastmoney" || !(stock?.now > 0)) {
    throw new Error(`invalid live quote: ${JSON.stringify(stock)}`);
  }
  const orderBook = structuredContent(await send("tools/call", {
    name: "get_order_book",
    arguments: { code: "SZ000034", source: "xueqiu" },
  }), "get_order_book");
  const book = orderBook?.response?.orderBook;
  const tape = orderBook?.response?.realtime;
  if (!(book?.levels > 0) || !(book?.bids?.[0]?.price > 0) || !(tape?.now > 0)) {
    throw new Error(`invalid live order book: ${JSON.stringify(orderBook?.response)}`);
  }
  console.log(JSON.stringify({
    ok: true,
    entry,
    version: initialized.serverInfo.version,
    code: stock.code,
    name: stock.name,
    price: stock.now,
    source: stock.source,
    series: liveSeries,
    orderBook: {
      levels: book.levels,
      bestBid: book.bids[0].price,
      bestAsk: book.asks?.[0]?.price ?? null,
      buyPercent: book.buyPercent,
      netVolume: book.netVolume,
      source: book.source,
    },
    liveTape: {
      price: tape.now,
      avgPrice: tape.avgPrice,
      turnoverRate: tape.turnoverRate,
      observedAt: tape.observedAt,
    },
    elapsedMillis: Date.now() - startedAt,
  }));
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  if (stderr.trim()) console.error(stderr.trim());
  process.exitCode = 1;
} finally {
  for (const request of pending.values()) clearTimeout(request.timeout);
  pending.clear();
  child.stdin.end();
  child.kill();
}
