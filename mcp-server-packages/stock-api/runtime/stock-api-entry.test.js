"use strict";

const assert = require("node:assert/strict");
const test = require("node:test");
const {
  BoundedResponseBuffer,
  MAX_RESPONSE_BYTES,
  parseBoundedJsonResponse,
  readBoundedResponseBody,
} = require("./stock-api-entry.js");

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
