"use strict";

const WRAPPER_VERSION = "2.9.0";
const PYSNOWBALL_VERSION = "0.1.8";
const MARKET_PERIODS = Object.freeze([
  "minute",
  "five-day",
  "day",
  "week",
  "month",
  "quarter",
  "year",
  "minute-120",
  "minute-60",
  "minute-30",
  "minute-15",
  "minute-5",
  "minute-1",
]);
const PERIOD_CODES = Object.freeze({
  minute: "1",
  "five-day": "5",
  day: "101",
  week: "102",
  month: "103",
  quarter: "104",
  year: "106",
  "minute-120": "60",
  "minute-60": "60",
  "minute-30": "30",
  "minute-15": "15",
  "minute-5": "5",
  "minute-1": "1",
});
const KLINE_HOSTS = Object.freeze([
  "7.push2his.eastmoney.com",
  "push2his.eastmoney.com",
  "33.push2his.eastmoney.com",
  "63.push2his.eastmoney.com",
  "91.push2his.eastmoney.com",
]);
const QUOTE_HOSTS = Object.freeze([
  "push2delay.eastmoney.com",
  "push2.eastmoney.com",
]);
const XUEQIU_QUOTE_URL = "https://stock.xueqiu.com/v5/stock/realtime/quotec.json";
const XUEQIU_ORDER_BOOK_URL = "https://stock.xueqiu.com/v5/stock/realtime/pankou.json";
const XUEQIU_REFERER = "https://xueqiu.com/";
const PYSNOWBALL_USER_AGENT = "Xueqiu iPhone 14.15.1";
const LIVE_SOURCES = Object.freeze(["auto", "xueqiu", "pysnowball"]);
const ORDER_BOOK_LEVELS = 10;
const REQUEST_TIMEOUT_MILLIS = 8000;
const HOST_OPERATION_TIMEOUT_MILLIS = 18000;
const HOST_RETRY_ROUNDS = 2;
const HOST_RETRY_DELAY_MILLIS = 150;
const MAX_RESPONSE_BYTES = 5 * 1024 * 1024;
const MAX_REQUEST_BYTES = 1 * 1024 * 1024;
const SUCCESS_CACHE_LIMIT = 64;
const QUOTE_CACHE_TTL_MILLIS = 2 * 60 * 1000;
const MARKET_SERIES_CACHE_TTL_MILLIS = 15 * 60 * 1000;
const ORDER_BOOK_CACHE_TTL_MILLIS = 45 * 1000;
const quoteCache = new Map();
const marketSeriesCache = new Map();
const orderBookCache = new Map();
let loopbackFixtureEnabled = false;
const ORDER_BOOK_TOOL = Object.freeze({
  name: "get_order_book",
  description: "Get a ten-level order book and intraday realtime tape through the pysnowball-compatible API with the existing Xueqiu path as fallback.",
  inputSchema: {
    type: "object",
    additionalProperties: false,
    required: ["code"],
    properties: {
      code: {
        type: "string",
        minLength: 1,
        description: "Unified stock code, such as SZ000034, HK00700, or USAAPL.",
      },
      source: {
        type: "string",
        enum: LIVE_SOURCES,
      },
    },
  },
});
const MARKET_SERIES_TOOL = Object.freeze({
  name: "get_market_series",
  description: "Get normalized Eastmoney intraday or K-line rows for Stock Monitor period tabs.",
  inputSchema: {
    type: "object",
    additionalProperties: false,
    required: ["code", "period"],
    properties: {
      code: {
        type: "string",
        minLength: 1,
        description: "Unified stock code, such as SZ000034, HK00700, or USAAPL.",
      },
      period: {
        type: "string",
        enum: MARKET_PERIODS,
      },
      count: {
        type: "number",
        minimum: 1,
        maximum: 2000,
      },
      adjust: {
        type: "string",
        enum: ["none", "qfq", "hfq"],
      },
      source: {
        type: "string",
        enum: ["eastmoney"],
      },
    },
  },
});

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
  loopbackFixtureEnabled = true;
  globalThis.fetch = (input, init) => {
    const target = typeof input === "string" || input instanceof URL ? String(input) : input.url;
    const proxy = new URL("proxy", base.href.endsWith("/") ? base : `${base.href}/`);
    proxy.searchParams.set("url", target);
    return nativeFetch(proxy, init);
  };
}

function asObject(value) {
  return value && typeof value === "object" && !Array.isArray(value) ? value : {};
}

function requireString(value, name) {
  if (typeof value !== "string" || !value.trim()) throw new Error(`Missing or invalid ${name}`);
  return value.trim();
}

function optionalCount(value, fallback) {
  if (value === undefined) return fallback;
  if (typeof value !== "number" || !Number.isFinite(value) || value <= 0 || value > 2000) {
    throw new Error(`Invalid count: ${String(value)}`);
  }
  return Math.floor(value);
}

function adjustCode(value) {
  if (value === undefined || value === "none") return "0";
  if (value === "qfq") return "1";
  if (value === "hfq") return "2";
  throw new Error(`Invalid adjust: ${String(value)}`);
}

function createToolResult(data, isError = false) {
  const structuredContent = JSON.parse(JSON.stringify(data));
  return {
    content: [{ type: "text", text: JSON.stringify(structuredContent, null, 2) }],
    structuredContent,
    ...(isError ? { isError: true } : {}),
  };
}

function cloneJson(value) {
  return JSON.parse(JSON.stringify(value));
}

function rememberSuccess(cache, key, value) {
  cache.delete(key);
  cache.set(key, { storedAtMillis: Date.now(), value: cloneJson(value) });
  while (cache.size > SUCCESS_CACHE_LIMIT) {
    cache.delete(cache.keys().next().value);
  }
}

function readRememberedSuccess(cache, key, ttlMillis) {
  const entry = cache.get(key);
  if (entry === undefined) return undefined;
  const ageMillis = Math.max(0, Date.now() - Number(entry.storedAtMillis || 0));
  if (ageMillis > ttlMillis) {
    cache.delete(key);
    return undefined;
  }
  cache.delete(key);
  cache.set(key, entry);
  return { ageMillis, value: cloneJson(entry.value) };
}

function providerMetadata(source, extra = {}) {
  return {
    id: "stock-api",
    wrapperVersion: WRAPPER_VERSION,
    source,
    ...extra,
  };
}

function markFreshResult(result, ttlMillis) {
  const response = asObject(result.response);
  response.fetchedAt = new Date().toISOString();
  response.cached = false;
  response.cacheAgeMillis = 0;
  response.cacheTtlMillis = ttlMillis;
  response.stale = false;
  result.response = response;
  return result;
}

function markCachedResult(entry, ttlMillis) {
  const result = entry.value;
  const response = asObject(result.response);
  response.cached = true;
  response.cacheAgeMillis = entry.ageMillis;
  response.cacheTtlMillis = ttlMillis;
  response.stale = false;
  result.response = response;
  return result;
}

function marketSeriesError(args, error, toolName = MARKET_SERIES_TOOL.name) {
  return createToolResult({
    input: { arguments: args, tool: toolName },
    response: {
      code: "STOCK_API_TOOL_ERROR",
      message: error instanceof Error ? error.message : String(error),
    },
  }, true);
}

function parseKline(line) {
  const [date, open, close, high, low, volume] = String(line).split(",");
  const row = {
    date,
    open: Number(open),
    close: Number(close),
    high: Number(high),
    low: Number(low),
    volume: Number.isFinite(Number(volume)) ? Number(volume) : 0,
    source: "eastmoney",
  };
  return date && [row.open, row.close, row.high, row.low].every(Number.isFinite) ? row : null;
}

function marketSecidCandidates(value) {
  const code = requireString(value, "code").toUpperCase();
  let match = /^(SH|SZ|BJ)(\d{6})$/.exec(code);
  if (match) return [`${match[1] === "SH" ? "1" : "0"}.${match[2]}`];
  match = /^HK(\d{1,5})$/.exec(code);
  if (match) return [`116.${match[1].padStart(5, "0")}`];
  match = /^US([A-Z][A-Z0-9.-]{0,19})$/.exec(code);
  if (match) return ["105", "106", "107"].map((market) => `${market}.${match[1]}`);
  throw new Error(`Unsupported unified stock code: ${code}`);
}

function quoteNumber(value) {
  if (value === undefined || value === null || value === "-") return null;
  const number = Number(value);
  return Number.isFinite(number) ? number : null;
}

function parseQuote(code, value) {
  const quote = asObject(value);
  const name = String(quote.f58 || quote.f14 || "").trim();
  const now = quoteNumber(quote.f43 ?? quote.f2);
  const yesterday = quoteNumber(quote.f60 ?? quote.f18);
  if (!name || now === null || now <= 0) return null;
  const providerPercent = quoteNumber(quote.f170 ?? quote.f3);
  return {
    code,
    name,
    now,
    low: quoteNumber(quote.f45 ?? quote.f16),
    high: quoteNumber(quote.f44 ?? quote.f15),
    yesterday,
    percent: providerPercent !== null
      ? providerPercent / 100
      : yesterday !== null && yesterday > 0 ? now / yesterday - 1 : null,
    source: "eastmoney",
  };
}

function tradingDate(row) {
  return String(row.date || "").slice(0, 10);
}

function xueqiuSymbol(value) {
  const code = requireString(value, "code").toUpperCase();
  let match = /^(SH|SZ|BJ)(\d{6})$/.exec(code);
  if (match) return `${match[1]}${match[2]}`;
  match = /^HK(\d{1,5})$/.exec(code);
  if (match) return `HK${match[1].padStart(5, "0")}`;
  match = /^US([A-Z][A-Z0-9.-]{0,19})$/.exec(code);
  if (match) return match[1];
  throw new Error(`Unsupported unified stock code: ${code}`);
}

function xueqiuTimestamp(value) {
  const millis = quoteNumber(value);
  if (millis === null || millis <= 0 || millis > 8.64e15) return null;
  const date = new Date(millis);
  if (!Number.isFinite(date.getTime())) return null;
  return date.toISOString();
}

function parseXueqiuRealtime(code, value, source = "xueqiu") {
  const row = asObject(value);
  const current = quoteNumber(row.current);
  if (current === null || current <= 0) return null;
  const previousClose = quoteNumber(row.last_close);
  const providerPercent = quoteNumber(row.percent);
  return {
    code,
    now: current,
    open: quoteNumber(row.open),
    high: quoteNumber(row.high),
    low: quoteNumber(row.low),
    yesterday: previousClose,
    change: quoteNumber(row.chg),
    percent: providerPercent !== null
      ? providerPercent / 100
      : previousClose !== null && previousClose > 0 ? current / previousClose - 1 : null,
    avgPrice: quoteNumber(row.avg_price),
    volume: quoteNumber(row.volume),
    amount: quoteNumber(row.amount),
    turnoverRate: quoteNumber(row.turnover_rate),
    amplitude: quoteNumber(row.amplitude),
    marketCapital: quoteNumber(row.market_capital),
    isTrade: row.is_trade === true,
    tradeSession: quoteNumber(row.trade_session),
    observedAt: xueqiuTimestamp(row.timestamp),
    source,
  };
}

function parseXueqiuOrderBookSide(value, pricePrefix, volumePrefix, orderPrefix) {
  const row = asObject(value);
  const levels = [];
  for (let level = 1; level <= ORDER_BOOK_LEVELS; level += 1) {
    const price = quoteNumber(row[`${pricePrefix}${level}`]);
    const volume = quoteNumber(row[`${volumePrefix}${level}`]);
    if (price === null || price <= 0) continue;
    levels.push({ level, price, volume, orders: quoteNumber(row[`${orderPrefix}${level}`]) });
  }
  return levels;
}

function parseXueqiuOrderBook(code, value, source = "xueqiu") {
  const row = asObject(value);
  const bids = parseXueqiuOrderBookSide(row, "bp", "bc", "bn");
  const asks = parseXueqiuOrderBookSide(row, "sp", "sc", "sn");
  if (bids.length === 0 && asks.length === 0) return null;
  return {
    code,
    bids,
    asks,
    current: quoteNumber(row.current),
    buyPercent: quoteNumber(row.buypct),
    sellPercent: quoteNumber(row.sellpct),
    netVolume: quoteNumber(row.diff),
    ratio: quoteNumber(row.ratio),
    levels: Math.max(bids.length, asks.length),
    observedAt: xueqiuTimestamp(row.timestamp),
    source,
  };
}

function keepLatestTradingDays(rows, days) {
  const dates = [...new Set(rows.map(tradingDate).filter(Boolean))].slice(-days);
  const allowed = new Set(dates);
  return rows.filter((row) => allowed.has(tradingDate(row)));
}

function aggregateTwoHourRows(rows) {
  const aggregated = [];
  for (const date of [...new Set(rows.map(tradingDate).filter(Boolean))]) {
    const sameDay = rows.filter((row) => tradingDate(row) === date);
    for (let index = 0; index < sameDay.length; index += 2) {
      const group = sameDay.slice(index, index + 2);
      if (group.length === 0) continue;
      aggregated.push({
        date: group[group.length - 1].date,
        open: group[0].open,
        close: group[group.length - 1].close,
        high: Math.max(...group.map((row) => row.high)),
        low: Math.min(...group.map((row) => row.low)),
        volume: group.reduce((sum, row) => sum + (Number.isFinite(row.volume) ? row.volume : 0), 0),
        source: "eastmoney",
      });
    }
  }
  return aggregated;
}

function normalizeProviderKline(value, source) {
  const row = asObject(value);
  const normalized = {
    date: String(row.date ?? row.day ?? ""),
    open: Number(row.open),
    close: Number(row.close),
    high: Number(row.high),
    low: Number(row.low),
    volume: Number.isFinite(Number(row.volume)) ? Number(row.volume) : 0,
    source,
  };
  return normalized.date && [normalized.open, normalized.close, normalized.high, normalized.low].every(Number.isFinite)
    ? normalized
    : null;
}

function aggregateRows(rows, groupSize, source) {
  const aggregated = [];
  for (const date of [...new Set(rows.map(tradingDate).filter(Boolean))]) {
    const sameDay = rows.filter((row) => tradingDate(row) === date);
    for (let index = 0; index < sameDay.length; index += groupSize) {
      const group = sameDay.slice(index, index + groupSize);
      if (group.length === 0) continue;
      aggregated.push({
        date: group[group.length - 1].date,
        open: group[0].open,
        close: group[group.length - 1].close,
        high: Math.max(...group.map((row) => row.high)),
        low: Math.min(...group.map((row) => row.low)),
        volume: group.reduce((sum, row) => sum + (Number.isFinite(row.volume) ? row.volume : 0), 0),
        source,
      });
    }
  }
  return aggregated;
}

function aggregateCalendarRows(rows, period) {
  const groups = new Map();
  for (const row of rows) {
    const date = /^([0-9]{4})-([0-9]{2})-/.exec(String(row.date));
    if (!date) continue;
    const key = period === "year"
      ? date[1]
      : `${date[1]}-Q${Math.floor((Number(date[2]) - 1) / 3) + 1}`;
    const group = groups.get(key) || [];
    group.push(row);
    groups.set(key, group);
  }
  return [...groups.values()].map((group) => ({
    date: group[group.length - 1].date,
    open: group[0].open,
    close: group[group.length - 1].close,
    high: Math.max(...group.map((row) => row.high)),
    low: Math.min(...group.map((row) => row.low)),
    volume: group.reduce((sum, row) => sum + (Number.isFinite(row.volume) ? row.volume : 0), 0),
    source: group[group.length - 1].source,
  }));
}

function sinaSymbol(code) {
  const match = /^(SH|SZ)([0-9]{6})$/.exec(code);
  return match ? `${match[1].toLowerCase()}${match[2]}` : null;
}

async function fetchSinaRows(code, scale, count) {
  const symbol = sinaSymbol(code);
  if (!symbol) throw new Error(`Sina fallback does not support ${code}`);
  const url = new URL("https://quotes.sina.cn/cn/api/json_v2.php/CN_MarketData.getKLineData");
  url.searchParams.set("symbol", symbol);
  url.searchParams.set("scale", String(scale));
  url.searchParams.set("ma", "no");
  url.searchParams.set("datalen", String(Math.min(2000, Math.max(1, count))));
  const response = await fetchJson(url, REQUEST_TIMEOUT_MILLIS, {
    Referer: "https://finance.sina.com.cn/",
  });
  if (!Array.isArray(response)) throw new Error("Sina did not return market series rows");
  return response.map((row) => normalizeProviderKline(row, "sina")).filter(Boolean);
}

async function fetchEastmoneyMinuteRows(code, count) {
  const url = new URL("https://push2delay.eastmoney.com/api/qt/stock/kline/get");
  url.searchParams.set("fields1", "f1,f2,f3,f4,f5,f6");
  url.searchParams.set("fields2", "f51,f52,f53,f54,f55,f56");
  url.searchParams.set("ut", "7eea3edcaed734bea9cbfc24409ed989");
  url.searchParams.set("klt", "1");
  url.searchParams.set("fqt", "0");
  url.searchParams.set("end", "20500101");
  url.searchParams.set("lmt", String(Math.min(800, Math.max(1, count))));
  for (const secid of marketSecidCandidates(code)) {
    url.searchParams.set("secid", secid);
    const response = await fetchJson(url);
    const rows = (response?.data?.klines || []).map(parseKline).filter(Boolean);
    if (rows.length > 0) return keepLatestTradingDays(rows, 1);
  }
  throw new Error("Eastmoney did not return minute rows");
}

async function callUpstreamKlines(upstreamHandle, code, period, count, adjust, source) {
  const response = await upstreamHandle({
    jsonrpc: "2.0",
    id: 0,
    method: "tools/call",
    params: {
      name: "get_klines",
      arguments: { code, period, count, adjust, source },
    },
  });
  const result = response?.result;
  if (response?.error || result?.isError) {
    throw new Error(`${source} K-line fallback failed`);
  }
  const rows = result?.structuredContent?.response?.klines;
  if (!Array.isArray(rows) || rows.length === 0) {
    throw new Error(`${source} did not return K-line rows`);
  }
  return rows.map((row) => normalizeProviderKline(row, source)).filter(Boolean);
}

async function executeStableMarketSeries(code, period, count, adjust, upstreamHandle) {
  if (period === "minute") {
    try {
      return await fetchEastmoneyMinuteRows(code, count);
    } catch {
      return (await fetchSinaRows(code, 5, Math.max(count, 240))).slice(-count);
    }
  }
  if (period === "five-day" || period.startsWith("minute-")) {
    const size = period === "minute-120" ? 24
      : period === "minute-60" ? 12
        : period === "minute-30" ? 6
          : period === "minute-15" ? 3
            : 1;
    const requestedRows = period === "five-day" ? 1200 : Math.min(2000, count * size);
    const rows = await fetchSinaRows(code, 5, requestedRows);
    if (period === "five-day") return keepLatestTradingDays(rows, 5).slice(-count);
    return aggregateRows(rows, size, "sina").slice(-count);
  }

  const providerPeriod = period === "quarter" || period === "year" ? "day" : period;
  const providerCount = period === "quarter" || period === "year" ? 2000 : count;
  let rows;
  try {
    rows = await callUpstreamKlines(upstreamHandle, code, providerPeriod, providerCount, adjust, "tencent");
  } catch {
    const scale = providerPeriod === "week" ? 1200 : providerPeriod === "month" ? 7200 : 240;
    rows = await fetchSinaRows(code, scale, providerCount);
  }
  if (period === "quarter" || period === "year") rows = aggregateCalendarRows(rows, period);
  return rows.slice(-count);
}

async function fetchJson(url, timeoutMillis = REQUEST_TIMEOUT_MILLIS, extraHeaders = {}, providerLabel = "Eastmoney") {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), timeoutMillis);
  try {
    const response = await fetch(url, {
      headers: {
        Accept: "application/json,text/plain,*/*",
        Referer: "https://quote.eastmoney.com/",
        ...extraHeaders,
      },
      signal: controller.signal,
    });
    if (!response.ok) throw new Error(`${providerLabel} HTTP ${response.status}`);
    const declaredLength = Number(response.headers.get("content-length"));
    if (Number.isFinite(declaredLength) && declaredLength > MAX_RESPONSE_BYTES) {
      throw new Error(`${providerLabel} response exceeds ${MAX_RESPONSE_BYTES} bytes`);
    }
    if (!response.body) throw new Error(`${providerLabel} returned an empty response body`);
    const reader = response.body.getReader();
    const chunks = [];
    let length = 0;
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      length += value.byteLength;
      if (length > MAX_RESPONSE_BYTES) {
        await reader.cancel();
        throw new Error(`${providerLabel} response exceeds ${MAX_RESPONSE_BYTES} bytes`);
      }
      chunks.push(value);
    }
    const bytes = new Uint8Array(length);
    let offset = 0;
    for (const chunk of chunks) {
      bytes.set(chunk, offset);
      offset += chunk.byteLength;
    }
    try {
      return JSON.parse(new TextDecoder("utf-8").decode(bytes));
    } catch {
      throw new Error(`${providerLabel} returned invalid JSON`);
    }
  } finally {
    clearTimeout(timeout);
  }
}

function pysnowballCookie() {
  const value = String(process.env.LOOM_PYSNOWBALL_TOKEN || "").trim();
  if (!value) return null;
  if (/[\r\n]/.test(value)) throw new Error("LOOM_PYSNOWBALL_TOKEN contains invalid characters");
  return value;
}

async function fetchXueqiuLike(url, symbol, provider, requireToken = false) {
  const target = new URL(url);
  target.searchParams.set("symbol", symbol);
  const cookie = provider === "pysnowball" ? pysnowballCookie() : null;
  if (requireToken && !cookie) {
    throw new Error("pysnowball order-book depth requires LOOM_PYSNOWBALL_TOKEN");
  }
  const headers = provider === "pysnowball"
    ? {
        Referer: XUEQIU_REFERER,
        "User-Agent": PYSNOWBALL_USER_AGENT,
        "Accept-Language": "zh-Hans-CN;q=1",
        ...(cookie ? { Cookie: cookie } : {}),
      }
    : { Referer: XUEQIU_REFERER };
  const providerLabel = provider === "pysnowball" ? "pysnowball" : "Xueqiu";
  const payload = await fetchJson(target, REQUEST_TIMEOUT_MILLIS, headers, providerLabel);
  const errorCode = Number(asObject(payload).error_code ?? 0);
  if (errorCode !== 0) {
    const message = String(asObject(payload).error_description || "").trim();
    throw new Error(`${providerLabel} error ${errorCode}${message ? `: ${message}` : ""}`);
  }
  return asObject(payload).data;
}

async function fetchLiveValue(requestedSource, kind, symbol) {
  const url = kind === "orderBook" ? XUEQIU_ORDER_BOOK_URL : XUEQIU_QUOTE_URL;
  if (requestedSource === "xueqiu") {
    return { data: await fetchXueqiuLike(url, symbol, "xueqiu"), provider: "xueqiu" };
  }
  if (requestedSource === "pysnowball") {
    return {
      data: await fetchXueqiuLike(url, symbol, "pysnowball", kind === "orderBook"),
      provider: "pysnowball",
    };
  }

  const primary = kind === "orderBook" && !pysnowballCookie() ? "xueqiu" : "pysnowball";
  try {
    return {
      data: await fetchXueqiuLike(url, symbol, primary, primary === "pysnowball" && kind === "orderBook"),
      provider: primary,
    };
  } catch (primaryError) {
    if (primary === "xueqiu") throw primaryError;
    return { data: await fetchXueqiuLike(url, symbol, "xueqiu"), provider: "xueqiu" };
  }
}

async function fetchFromHosts(url, hosts) {
  let lastError;
  const deadline = Date.now() + HOST_OPERATION_TIMEOUT_MILLIS;
  for (let round = 0; round < HOST_RETRY_ROUNDS; round += 1) {
    for (const host of hosts) {
      const remainingMillis = deadline - Date.now();
      if (remainingMillis <= 0) {
        throw lastError || new Error("Eastmoney host operation timed out");
      }
      try {
        const candidate = new URL(url);
        candidate.hostname = host;
        return await fetchJson(candidate, Math.min(REQUEST_TIMEOUT_MILLIS, remainingMillis));
      } catch (error) {
        lastError = error;
      }
    }
    if (round + 1 < HOST_RETRY_ROUNDS) {
      const retryDelay = Math.min(
        HOST_RETRY_DELAY_MILLIS * (round + 1),
        Math.max(0, deadline - Date.now()),
      );
      if (retryDelay > 0) await new Promise((resolve) => setTimeout(resolve, retryDelay));
    }
  }
  throw lastError || new Error("Eastmoney market series is unavailable");
}

async function executeMarketQuote(rawArgs) {
  const args = asObject(rawArgs);
  const code = requireString(args.code, "code").toUpperCase();
  if (args.source !== undefined && args.source !== "eastmoney") {
    throw new Error(`Invalid source: ${String(args.source)}`);
  }
  const url = new URL("https://push2delay.eastmoney.com/api/qt/stock/get");
  url.searchParams.set("fltt", "2");
  url.searchParams.set("invt", "2");
  url.searchParams.set("fields", "f43,f44,f45,f57,f58,f60,f170");
  let lastError;
  for (const secid of marketSecidCandidates(code)) {
    url.searchParams.set("secid", secid);
    let response;
    try {
      response = await fetchFromHosts(url, QUOTE_HOSTS);
    } catch (error) {
      lastError = error;
      continue;
    }
    const stock = parseQuote(code, response?.data);
    if (stock) {
      const result = markFreshResult({
        input: { code, source: "eastmoney" },
        provider: providerMetadata("eastmoney"),
        response: { stock },
      }, QUOTE_CACHE_TTL_MILLIS);
      rememberSuccess(quoteCache, code, result);
      return result;
    }
  }
  const cached = readRememberedSuccess(quoteCache, code, QUOTE_CACHE_TTL_MILLIS);
  if (cached) {
    return markCachedResult(cached, QUOTE_CACHE_TTL_MILLIS);
  }
  if (lastError) throw lastError;
  throw new Error("Eastmoney did not return a valid stock quote");
}

async function executeOrderBook(rawArgs) {
  const args = asObject(rawArgs);
  const code = requireString(args.code, "code").toUpperCase();
  const requestedSource = args.source === undefined ? "auto" : String(args.source);
  if (!LIVE_SOURCES.includes(requestedSource)) {
    throw new Error(`Invalid source: ${String(args.source)}`);
  }
  const symbol = xueqiuSymbol(code);
  const [bookSettled, realtimeSettled] = await Promise.allSettled([
    fetchLiveValue(requestedSource, "orderBook", symbol),
    fetchLiveValue(requestedSource, "realtime", symbol),
  ]);
  const orderBook = bookSettled.status === "fulfilled"
    ? parseXueqiuOrderBook(code, bookSettled.value.data, bookSettled.value.provider)
    : null;
  const realtimeRow = realtimeSettled.status === "fulfilled"
    ? (Array.isArray(realtimeSettled.value.data) ? realtimeSettled.value.data : [])
      .find((row) => String(asObject(row).symbol || "").toUpperCase() === symbol)
    : null;
  const realtime = realtimeRow
    ? parseXueqiuRealtime(code, realtimeRow, realtimeSettled.value.provider)
    : null;
  if (orderBook || realtime) {
    const liveSources = {
      orderBook: orderBook?.source || null,
      realtime: realtime?.source || null,
    };
    const result = markFreshResult({
      input: { code, source: requestedSource, symbol },
      provider: providerMetadata(
        liveSources.orderBook && liveSources.realtime && liveSources.orderBook !== liveSources.realtime
          ? "mixed"
          : liveSources.realtime || liveSources.orderBook || requestedSource,
        {
          requestedSource,
          liveSources,
          pysnowballVersion: PYSNOWBALL_VERSION,
          pysnowballTokenConfigured: Boolean(pysnowballCookie()),
        },
      ),
      response: { orderBook, realtime },
    }, ORDER_BOOK_CACHE_TTL_MILLIS);
    rememberSuccess(orderBookCache, `${code}|${requestedSource}`, result);
    return result;
  }
  const cached = readRememberedSuccess(
    orderBookCache,
    `${code}|${requestedSource}`,
    ORDER_BOOK_CACHE_TTL_MILLIS,
  );
  if (cached) {
    return markCachedResult(cached, ORDER_BOOK_CACHE_TTL_MILLIS);
  }
  for (const settled of [bookSettled, realtimeSettled]) {
    if (settled.status === "rejected") throw settled.reason;
  }
  throw new Error("Xueqiu did not return an order book or realtime tape");
}

async function executeMarketSeries(rawArgs, upstreamHandle) {
  const args = asObject(rawArgs);
  const code = requireString(args.code, "code").toUpperCase();
  const period = requireString(args.period, "period");
  if (!MARKET_PERIODS.includes(period)) throw new Error(`Invalid period: ${period}`);
  if (args.source !== undefined && args.source !== "eastmoney") {
    throw new Error(`Invalid source: ${String(args.source)}`);
  }
  const requestedCount = optionalCount(args.count, period === "five-day" ? 2000 : period === "minute" ? 800 : 240);
  const count = period === "five-day" ? 2000 : period === "minute" ? 800 : requestedCount;
  const requestCount = period === "five-day"
    ? 2000
    : period === "minute-120" ? Math.min(2000, count * 2) : count;
  const cacheKey = `${code}|${period}|${count}|${args.adjust || "none"}`;
  const url = new URL("https://push2his.eastmoney.com/api/qt/stock/kline/get");
  url.searchParams.set("fields1", "f1,f2,f3,f4,f5,f6");
  url.searchParams.set("fields2", "f51,f52,f53,f54,f55,f56");
  url.searchParams.set("ut", "7eea3edcaed734bea9cbfc24409ed989");
  url.searchParams.set("klt", PERIOD_CODES[period]);
  url.searchParams.set("fqt", adjustCode(args.adjust));
  url.searchParams.set("end", "20500101");
  url.searchParams.set("lmt", String(requestCount));
  let rows = [];
  let lastError;
  if (!loopbackFixtureEnabled) {
    try {
      rows = await executeStableMarketSeries(code, period, count, args.adjust || "none", upstreamHandle);
    } catch (error) {
      lastError = error;
    }
  }
  for (const secid of marketSecidCandidates(code)) {
    if (rows.length > 0) break;
    url.searchParams.set("secid", secid);
    let response;
    try {
      response = await fetchFromHosts(url, KLINE_HOSTS);
    } catch (error) {
      lastError = error;
      continue;
    }
    rows = (response?.data?.klines || []).map(parseKline).filter(Boolean);
    if (rows.length > 0) break;
  }
  rows.sort((left, right) => String(left.date).localeCompare(String(right.date)));
  if (period === "minute") rows = keepLatestTradingDays(rows, 1);
  if (period === "five-day") rows = keepLatestTradingDays(rows, 5);
  if (period === "minute-120" && rows.every((row) => row.source === "eastmoney")) {
    rows = aggregateTwoHourRows(rows);
  }
  rows = rows.slice(-count);
  if (rows.length === 0) {
    const cached = readRememberedSuccess(marketSeriesCache, cacheKey, MARKET_SERIES_CACHE_TTL_MILLIS);
    if (cached) {
      return markCachedResult(cached, MARKET_SERIES_CACHE_TTL_MILLIS);
    }
    if (lastError) throw lastError;
    throw new Error("Eastmoney did not return market series rows");
  }
  const result = markFreshResult({
    input: {
      adjust: args.adjust || "none",
      code,
      count,
      period,
      source: "eastmoney",
    },
    provider: providerMetadata("eastmoney"),
    response: {
      count: rows.length,
      klines: rows,
      lastTradingDate: tradingDate(rows[rows.length - 1]),
      period,
    },
  }, MARKET_SERIES_CACHE_TTL_MILLIS);
  rememberSuccess(marketSeriesCache, cacheKey, result);
  return result;
}

async function handleWrapperRequest(request, upstreamHandle) {
  if (!request || typeof request !== "object") return upstreamHandle(request);
  if (request.method === "initialize" && request.id !== undefined) {
    const response = await upstreamHandle(request);
    if (response?.result?.serverInfo) response.result.serverInfo.version = WRAPPER_VERSION;
    return response;
  }
  if (request.method === "tools/list" && request.id !== undefined) {
    const response = await upstreamHandle(request);
    if (response?.result?.tools) {
      for (const tool of [MARKET_SERIES_TOOL, ORDER_BOOK_TOOL]) {
        if (!response.result.tools.some((existing) => existing.name === tool.name)) {
          response.result.tools.push(tool);
        }
      }
    }
    return response;
  }
  if (request.method === "tools/call" && request.id !== undefined) {
    const params = asObject(request.params);
    if (params.name === "get_stock") {
      const args = asObject(params.arguments);
      if (args.source === "eastmoney") {
        let result;
        try {
          result = createToolResult(await executeMarketQuote(args));
        } catch (error) {
          result = marketSeriesError(args, error, "get_stock");
        }
        return { jsonrpc: "2.0", id: request.id, result };
      }
    }
    if (params.name === ORDER_BOOK_TOOL.name) {
      const args = asObject(params.arguments);
      let result;
      try {
        result = createToolResult(await executeOrderBook(args));
      } catch (error) {
        result = marketSeriesError(args, error, params.name);
      }
      return { jsonrpc: "2.0", id: request.id, result };
    }
    if (
      params.name === MARKET_SERIES_TOOL.name
      || (params.name === "get_klines" && params.arguments?.source === "eastmoney")
    ) {
      const args = asObject(params.arguments);
      let result;
      try {
        result = createToolResult(await executeMarketSeries(args, upstreamHandle));
      } catch (error) {
        result = marketSeriesError(args, error, params.name);
      }
      return { jsonrpc: "2.0", id: request.id, result };
    }
  }
  return upstreamHandle(request);
}

async function runWrapperServer(input, output, upstreamHandle) {
  input.setEncoding("utf8");
  let buffer = "";
  // Set when an oversized line is rejected before its terminating newline has arrived. Every byte
  // up to that newline still belongs to the message that was already answered, so it is dropped
  // instead of parsed: treating the tail as a fresh line would answer a fragment, and from then on
  // every response would belong to the wrong request — the framing never recovers on its own.
  let discardingUntilNewline = false;
  const rejectOversizedRequest = () => {
    output.write(`${JSON.stringify({
      jsonrpc: "2.0",
      id: null,
      error: { code: -32600, message: `JSON-RPC request exceeds ${MAX_REQUEST_BYTES} bytes` },
    })}\n`);
  };
  input.on("data", (chunk) => {
    buffer += chunk;
    if (discardingUntilNewline) {
      const boundary = buffer.indexOf("\n");
      if (boundary < 0) {
        // Still inside the rejected message: drop what arrived and keep waiting for the boundary,
        // so a client that never stops writing cannot grow the buffer either.
        buffer = "";
        return;
      }
      buffer = buffer.slice(boundary + 1);
      discardingUntilNewline = false;
    }
    if (buffer.indexOf("\n") < 0 && Buffer.byteLength(buffer, "utf8") > MAX_REQUEST_BYTES) {
      buffer = "";
      discardingUntilNewline = true;
      rejectOversizedRequest();
      return;
    }
    let newlineIndex = buffer.indexOf("\n");
    while (newlineIndex >= 0) {
      const rawLine = buffer.slice(0, newlineIndex);
      buffer = buffer.slice(newlineIndex + 1);
      if (Buffer.byteLength(rawLine, "utf8") > MAX_REQUEST_BYTES) {
        // This line is already complete, so the same invalid-request answer needs no discard state.
        rejectOversizedRequest();
        newlineIndex = buffer.indexOf("\n");
        continue;
      }
      const line = rawLine.trim();
      if (line) {
        void (async () => {
          let request;
          try {
            request = JSON.parse(line);
            const response = await handleWrapperRequest(request, upstreamHandle);
            if (response) output.write(`${JSON.stringify(response)}\n`);
          } catch (error) {
            output.write(`${JSON.stringify({
              jsonrpc: "2.0",
              id: request?.id ?? null,
              error: { code: -32603, message: error instanceof Error ? error.message : String(error) },
            })}\n`);
          }
        })();
      }
      newlineIndex = buffer.indexOf("\n");
    }
  });
}

try {
  configureLoopbackFixture();
  const { handleMcpRequest } = require("./vendor/stock-api/dist/mcp/server.js");
  void runWrapperServer(process.stdin, process.stdout, handleMcpRequest);
} catch (error) {
  process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
  process.exitCode = 1;
}
