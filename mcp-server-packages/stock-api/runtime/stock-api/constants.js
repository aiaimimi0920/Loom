"use strict";

// Owns immutable provider limits, MCP tool schemas, caches, and runtime state.
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
// One maximum-sized JSON-RPC frame plus its delimiter may wait behind the bounded request queue.
const MAX_BUFFERED_INPUT_BYTES = MAX_REQUEST_BYTES + 1;
const MAX_IN_FLIGHT_REQUESTS = 8;
const MAX_PENDING_REQUESTS = 16;
const SUCCESS_CACHE_LIMIT = 64;
const QUOTE_CACHE_TTL_MILLIS = 2 * 60 * 1000;
const MARKET_SERIES_CACHE_TTL_MILLIS = 15 * 60 * 1000;
const ORDER_BOOK_CACHE_TTL_MILLIS = 45 * 1000;
const quoteCache = new Map();
const marketSeriesCache = new Map();
const orderBookCache = new Map();
const runtimeState = { loopbackFixtureEnabled: false };
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

module.exports = Object.freeze({
  WRAPPER_VERSION,
  PYSNOWBALL_VERSION,
  MARKET_PERIODS,
  PERIOD_CODES,
  KLINE_HOSTS,
  QUOTE_HOSTS,
  XUEQIU_QUOTE_URL,
  XUEQIU_ORDER_BOOK_URL,
  XUEQIU_REFERER,
  PYSNOWBALL_USER_AGENT,
  LIVE_SOURCES,
  ORDER_BOOK_LEVELS,
  REQUEST_TIMEOUT_MILLIS,
  HOST_OPERATION_TIMEOUT_MILLIS,
  HOST_RETRY_ROUNDS,
  HOST_RETRY_DELAY_MILLIS,
  MAX_RESPONSE_BYTES,
  MAX_REQUEST_BYTES,
  MAX_BUFFERED_INPUT_BYTES,
  MAX_IN_FLIGHT_REQUESTS,
  MAX_PENDING_REQUESTS,
  SUCCESS_CACHE_LIMIT,
  QUOTE_CACHE_TTL_MILLIS,
  MARKET_SERIES_CACHE_TTL_MILLIS,
  ORDER_BOOK_CACHE_TTL_MILLIS,
  quoteCache,
  marketSeriesCache,
  orderBookCache,
  runtimeState,
  ORDER_BOOK_TOOL,
  MARKET_SERIES_TOOL,
});
