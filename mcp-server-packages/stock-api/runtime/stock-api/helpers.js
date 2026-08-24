"use strict";

// Owns validation, JSON result, cache, and freshness helpers shared by providers.
const {
  MARKET_SERIES_TOOL,
  SUCCESS_CACHE_LIMIT,
  WRAPPER_VERSION,
} = require("./constants.js");

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

module.exports = Object.freeze({
  asObject,
  requireString,
  optionalCount,
  adjustCode,
  createToolResult,
  rememberSuccess,
  readRememberedSuccess,
  providerMetadata,
  markFreshResult,
  markCachedResult,
  marketSeriesError,
});
