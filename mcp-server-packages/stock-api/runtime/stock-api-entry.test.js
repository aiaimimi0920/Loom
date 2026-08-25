"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { PassThrough } = require("node:stream");
const test = require("node:test");
const {
  BoundedResponseBuffer,
  MAX_RESPONSE_BYTES,
  parseBoundedJsonResponse,
  readBoundedResponseBody,
} = require("./stock-api-entry.js");
const {
  MAX_BUFFERED_INPUT_BYTES,
  MAX_IN_FLIGHT_REQUESTS,
} = require("./stock-api/constants.js");
const { runWrapperServer } = require("./stock-api/server.js");
const { getDelimitedParams } = require("./vendor/stock-api/dist/stocks/shared/provider.js");

function responseFromChunks(chunks, declaredLength = null) {
  let index = 0;
  let cancelled = false;
  return {
    headers: {
      get(name) {
        return name.toLowerCase() === "content-length" ? declaredLength : null;
      },
    },
    body: {
      getReader() {
        return {
          async read() {
            if (index >= chunks.length) return { done: true, value: undefined };
            const value = chunks[index];
            index += 1;
            return { done: false, value };
          },
          async cancel() {
            cancelled = true;
          },
        };
      },
    },
    wasCancelled() {
      return cancelled;
    },
  };
}

function splitBytes(bytes, chunkSize) {
  const chunks = [];
  for (let offset = 0; offset < bytes.byteLength; offset += chunkSize) {
    chunks.push(bytes.subarray(offset, Math.min(bytes.byteLength, offset + chunkSize)));
  }
  return chunks;
}

async function waitFor(predicate, message) {
  const deadline = Date.now() + 5000;
  while (!predicate()) {
    if (Date.now() >= deadline) throw new Error(message);
    await new Promise((resolve) => setTimeout(resolve, 5));
  }
}

test("vendored stock parsers remove every upstream quote delimiter", () => {
  assert.deepEqual(getDelimitedParams('v_quote="A"B~12"', "~"), ["AB", "12"]);
  for (const relative of [
    "vendor/stock-api/dist/stocks/tencent/index.js",
    "vendor/stock-api/dist/browser/stock-api.esm.mjs",
    "vendor/stock-api/dist/browser/stock-api.iife.js",
  ]) {
    const source = fs.readFileSync(path.join(__dirname, relative), "utf8");
    assert.doesNotMatch(source, /\.replace\('\"', ""\)/);
  }
});

test("known 5 MiB Unicode body uses one exact byte buffer and parses at the boundary", async () => {
  const unicodeTail = "汉";
  const envelopeBytes = Buffer.byteLength(JSON.stringify({ value: unicodeTail }), "utf8");
  const json = JSON.stringify({
    value: `${"x".repeat(MAX_RESPONSE_BYTES - envelopeBytes)}${unicodeTail}`,
  });
  const bytes = new TextEncoder().encode(json);
  assert.equal(bytes.byteLength, MAX_RESPONSE_BYTES);
  const response = responseFromChunks(splitBytes(bytes, 73 * 1024), String(bytes.byteLength));

  const parsed = await parseBoundedJsonResponse(response, "fixture");
  assert.equal(parsed.value.length, MAX_RESPONSE_BYTES - envelopeBytes + 1);
  assert.equal(parsed.value.endsWith(unicodeTail), true);

  const allocationEvidence = new BoundedResponseBuffer(
    MAX_RESPONSE_BYTES,
    MAX_RESPONSE_BYTES,
  );
  assert.equal(allocationEvidence.buffer.byteLength, MAX_RESPONSE_BYTES);
  assert.equal(allocationEvidence.peakAllocatedBytes, MAX_RESPONSE_BYTES);
});

test("unknown-length growth has a bounded transient peak and no final byte copy", () => {
  const accumulator = new BoundedResponseBuffer(MAX_RESPONSE_BYTES);
  const chunk = new Uint8Array(61 * 1024);
  while (accumulator.length + chunk.byteLength <= MAX_RESPONSE_BYTES) accumulator.append(chunk);
  accumulator.append(new Uint8Array(MAX_RESPONSE_BYTES - accumulator.length));

  assert.equal(accumulator.length, MAX_RESPONSE_BYTES);
  assert.equal(accumulator.bytes().buffer, accumulator.buffer.buffer);
  assert.ok(accumulator.peakAllocatedBytes < MAX_RESPONSE_BYTES * 2);
});

test("streamed body over the cap is cancelled and rejected", async () => {
  const response = responseFromChunks([
    new Uint8Array(MAX_RESPONSE_BYTES),
    new Uint8Array(1),
  ]);

  await assert.rejects(
    readBoundedResponseBody(response, "fixture"),
    new RegExp(`fixture response exceeds ${MAX_RESPONSE_BYTES} bytes`),
  );
  assert.equal(response.wasCancelled(), true);
});

test("provider stream read failures cancel and release the reader", async () => {
  let cancelled = false;
  let released = false;
  const response = {
    headers: { get() { return null; } },
    body: {
      getReader() {
        return {
          async read() { throw new Error("fixture stream failed"); },
          async cancel() { cancelled = true; },
          releaseLock() { released = true; },
        };
      },
    },
  };

  await assert.rejects(readBoundedResponseBody(response, "fixture"), /fixture stream failed/);
  assert.equal(cancelled, true);
  assert.equal(released, true);
});

test("declared body over the cap is rejected before a reader is acquired", async () => {
  const response = responseFromChunks([], String(MAX_RESPONSE_BYTES + 1));
  await assert.rejects(
    readBoundedResponseBody(response, "fixture"),
    new RegExp(`fixture response exceeds ${MAX_RESPONSE_BYTES} bytes`),
  );
});

test("invalid JSON keeps the provider-specific error contract", async () => {
  const response = responseFromChunks([new TextEncoder().encode("{not-json")]);
  await assert.rejects(
    parseBoundedJsonResponse(response, "fixture"),
    /fixture returned invalid JSON/,
  );
});

test("stdio framing applies bounded in-flight backpressure", async () => {
  const input = new PassThrough();
  const output = new PassThrough();
  let outputBuffer = "";
  const responses = [];
  output.setEncoding("utf8");
  output.on("data", (chunk) => {
    outputBuffer += chunk;
    let newline = outputBuffer.indexOf("\n");
    while (newline >= 0) {
      responses.push(JSON.parse(outputBuffer.slice(0, newline)));
      outputBuffer = outputBuffer.slice(newline + 1);
      newline = outputBuffer.indexOf("\n");
    }
  });

  let active = 0;
  let peakActive = 0;
  const releases = [];
  const upstreamHandle = (request) => new Promise((resolve) => {
    active += 1;
    peakActive = Math.max(peakActive, active);
    releases.push(() => {
      active -= 1;
      resolve({ jsonrpc: "2.0", id: request.id, result: { ok: true } });
    });
  });

  try {
    void runWrapperServer(input, output, upstreamHandle);
    const requestCount = 40;
    input.write(`${Array.from({ length: requestCount }, (_, index) => JSON.stringify({
      jsonrpc: "2.0",
      id: index + 1,
      method: "fixture",
    })).join("\n")}\n`);

    await waitFor(() => releases.length === MAX_IN_FLIGHT_REQUESTS, "initial request window did not fill");
    assert.equal(input.isPaused(), true);
    while (responses.length < requestCount) {
      await waitFor(() => releases.length > 0, "queued request window did not advance");
      for (const release of releases.splice(0)) release();
      await new Promise((resolve) => setImmediate(resolve));
    }
    assert.equal(peakActive, MAX_IN_FLIGHT_REQUESTS);
    assert.equal(responses.length, requestCount);
    assert.deepEqual(
      [...responses.map((response) => response.id)].sort((left, right) => left - right),
      Array.from({ length: requestCount }, (_, index) => index + 1),
    );
  } finally {
    input.destroy();
    output.destroy();
  }
});

test("stdio framing drops an oversized delivered chunk and recovers at a newline", async () => {
  const input = new PassThrough();
  const output = new PassThrough();
  let outputBuffer = "";
  const responses = [];
  const upstreamIds = [];
  output.setEncoding("utf8");
  output.on("data", (chunk) => {
    outputBuffer += chunk;
    let newline = outputBuffer.indexOf("\n");
    while (newline >= 0) {
      responses.push(JSON.parse(outputBuffer.slice(0, newline)));
      outputBuffer = outputBuffer.slice(newline + 1);
      newline = outputBuffer.indexOf("\n");
    }
  });

  try {
    void runWrapperServer(input, output, async (request) => {
      upstreamIds.push(request.id);
      return { jsonrpc: "2.0", id: request.id, result: { ok: true } };
    });
    const oversizedChunk = "{}\n".repeat(Math.ceil(MAX_BUFFERED_INPUT_BYTES / 3) + 1);
    input.write(oversizedChunk);
    await waitFor(() => responses.length === 1, "backlog rejection was not emitted");
    assert.equal(responses[0].error.code, -32000);
    assert.deepEqual(upstreamIds, []);

    input.write(`${JSON.stringify({ jsonrpc: "2.0", id: 99, method: "fixture" })}\n`);
    await waitFor(() => responses.length === 2, "framing did not recover after backlog rejection");
    assert.equal(responses[1].id, 99);
    assert.deepEqual(upstreamIds, [99]);
  } finally {
    input.destroy();
    output.destroy();
  }
});
