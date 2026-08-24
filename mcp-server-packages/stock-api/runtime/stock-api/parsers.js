"use strict";

// Owns provider record parsing and deterministic market-series aggregation.
const { ORDER_BOOK_LEVELS } = require("./constants.js");
const { asObject, requireString } = require("./helpers.js");

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

module.exports = Object.freeze({
  parseKline,
  marketSecidCandidates,
  quoteNumber,
  parseQuote,
  tradingDate,
  xueqiuSymbol,
  parseXueqiuRealtime,
  parseXueqiuOrderBook,
  keepLatestTradingDays,
  aggregateTwoHourRows,
  normalizeProviderKline,
  aggregateRows,
  aggregateCalendarRows,
});
