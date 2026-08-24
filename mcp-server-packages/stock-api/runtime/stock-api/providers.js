"use strict";

// Owns Eastmoney, Sina, Xueqiu, and pysnowball provider selection and fallbacks.
const {
  PYSNOWBALL_USER_AGENT,
  REQUEST_TIMEOUT_MILLIS,
  XUEQIU_ORDER_BOOK_URL,
  XUEQIU_QUOTE_URL,
  XUEQIU_REFERER,
} = require("./constants.js");
const { asObject } = require("./helpers.js");
const {
  aggregateCalendarRows,
  aggregateRows,
  keepLatestTradingDays,
  marketSecidCandidates,
  normalizeProviderKline,
  parseKline,
} = require("./parsers.js");
const { fetchJson } = require("./transport.js");

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

module.exports = Object.freeze({
  fetchSinaRows,
  fetchEastmoneyMinuteRows,
  callUpstreamKlines,
  executeStableMarketSeries,
  pysnowballCookie,
  fetchXueqiuLike,
  fetchLiveValue,
});
