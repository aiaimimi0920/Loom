"use strict";

// Owns stock quote, order-book, and market-series tool execution.
const {
  KLINE_HOSTS,
  LIVE_SOURCES,
  MARKET_PERIODS,
  MARKET_SERIES_CACHE_TTL_MILLIS,
  ORDER_BOOK_CACHE_TTL_MILLIS,
  PERIOD_CODES,
  PYSNOWBALL_VERSION,
  QUOTE_CACHE_TTL_MILLIS,
  QUOTE_HOSTS,
  marketSeriesCache,
  orderBookCache,
  quoteCache,
  runtimeState,
} = require("./constants.js");
const {
  adjustCode,
  asObject,
  markCachedResult,
  markFreshResult,
  optionalCount,
  providerMetadata,
  readRememberedSuccess,
  rememberSuccess,
  requireString,
} = require("./helpers.js");
const {
  aggregateTwoHourRows,
  keepLatestTradingDays,
  marketSecidCandidates,
  parseKline,
  parseQuote,
  parseXueqiuOrderBook,
  parseXueqiuRealtime,
  tradingDate,
  xueqiuSymbol,
} = require("./parsers.js");
const {
  executeStableMarketSeries,
  fetchLiveValue,
  pysnowballCookie,
} = require("./providers.js");
const { fetchFromHosts } = require("./transport.js");

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
  if (!runtimeState.loopbackFixtureEnabled) {
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

module.exports = Object.freeze({
  executeMarketQuote,
  executeOrderBook,
  executeMarketSeries,
});
