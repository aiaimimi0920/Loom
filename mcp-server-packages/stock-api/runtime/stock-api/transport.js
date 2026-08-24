"use strict";

// Owns loopback fixture routing, bounded HTTP bodies, fetch timeouts, and host retry.
const {
  HOST_OPERATION_TIMEOUT_MILLIS,
  HOST_RETRY_DELAY_MILLIS,
  HOST_RETRY_ROUNDS,
  MAX_RESPONSE_BYTES,
  REQUEST_TIMEOUT_MILLIS,
  runtimeState,
} = require("./constants.js");

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
  runtimeState.loopbackFixtureEnabled = true;
  globalThis.fetch = (input, init) => {
    const target = typeof input === "string" || input instanceof URL ? String(input) : input.url;
    const proxy = new URL("proxy", base.href.endsWith("/") ? base : `${base.href}/`);
    proxy.searchParams.set("url", target);
    return nativeFetch(proxy, init);
  };
}

class BoundedResponseBuffer {
  constructor(limitBytes, declaredLength = Number.NaN) {
    if (!Number.isSafeInteger(limitBytes) || limitBytes <= 0) {
      throw new Error("response buffer limit must be a positive safe integer");
    }
    this.limitBytes = limitBytes;
    const knownLength = Number.isSafeInteger(declaredLength) && declaredLength >= 0
      ? declaredLength
      : null;
    if (knownLength !== null && knownLength > limitBytes) {
      throw new RangeError("response exceeds buffer limit");
    }
    const initialCapacity = knownLength !== null && knownLength > 0
      ? knownLength
      : Math.min(64 * 1024, limitBytes);
    this.buffer = new Uint8Array(initialCapacity);
    this.length = 0;
    // During a growth copy both the old and replacement arrays are live. Recording that transient
    // allocation makes the byte-buffer peak testable without relying on noisy process-wide RSS.
    this.peakAllocatedBytes = initialCapacity;
  }

  append(value) {
    const chunk = value instanceof Uint8Array ? value : new Uint8Array(value);
    const required = this.length + chunk.byteLength;
    if (!Number.isSafeInteger(required) || required > this.limitBytes) {
      throw new RangeError("response exceeds buffer limit");
    }
    if (required > this.buffer.byteLength) {
      const previousCapacity = this.buffer.byteLength;
      const nextCapacity = Math.min(
        this.limitBytes,
        Math.max(required, Math.max(1, previousCapacity * 2)),
      );
      const replacement = new Uint8Array(nextCapacity);
      replacement.set(this.buffer.subarray(0, this.length));
      this.peakAllocatedBytes = Math.max(
        this.peakAllocatedBytes,
        previousCapacity + nextCapacity,
      );
      this.buffer = replacement;
    }
    this.buffer.set(chunk, this.length);
    this.length = required;
  }

  bytes() {
    // subarray is a view over the one retained allocation; it does not make the second contiguous
    // copy that the old chunks-plus-final-array implementation made.
    return this.buffer.subarray(0, this.length);
  }
}

async function readBoundedResponseBody(response, providerLabel, maximumBytes = MAX_RESPONSE_BYTES) {
  const rawDeclaredLength = response.headers.get("content-length");
  const declaredLength = rawDeclaredLength === null ? Number.NaN : Number(rawDeclaredLength);
  if (Number.isFinite(declaredLength) && declaredLength > maximumBytes) {
    throw new Error(`${providerLabel} response exceeds ${maximumBytes} bytes`);
  }
  if (!response.body) throw new Error(`${providerLabel} returned an empty response body`);

  const reader = response.body.getReader();
  const accumulator = new BoundedResponseBuffer(maximumBytes, declaredLength);
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      accumulator.append(value);
    }
    return accumulator;
  } catch (error) {
    try {
      await reader.cancel();
    } catch {
      // Preserve the read/size failure when provider cancellation also fails.
    }
    if (error instanceof RangeError) {
      throw new Error(`${providerLabel} response exceeds ${maximumBytes} bytes`);
    }
    throw error;
  } finally {
    if (typeof reader.releaseLock === "function") reader.releaseLock();
  }
}

async function parseBoundedJsonResponse(response, providerLabel, maximumBytes = MAX_RESPONSE_BYTES) {
  const accumulator = await readBoundedResponseBody(response, providerLabel, maximumBytes);
  try {
    return JSON.parse(new TextDecoder("utf-8").decode(accumulator.bytes()));
  } catch {
    throw new Error(`${providerLabel} returned invalid JSON`);
  }
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
    return await parseBoundedJsonResponse(response, providerLabel);
  } finally {
    clearTimeout(timeout);
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

module.exports = Object.freeze({
  configureLoopbackFixture,
  BoundedResponseBuffer,
  readBoundedResponseBody,
  parseBoundedJsonResponse,
  fetchJson,
  fetchFromHosts,
});
