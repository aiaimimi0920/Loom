import assert from "node:assert/strict";
import { Buffer } from "node:buffer";
import test from "node:test";

import {
  MAX_MCP_PACKAGE_FILE_BYTES,
  assertMcpPackageFileSize,
  encodeMcpPackageBytes,
} from "./McpPackageFile.ts";

test("bounds MCP package files before reading or encoding them", () => {
  assert.doesNotThrow(() => assertMcpPackageFileSize(0));
  assert.doesNotThrow(() => assertMcpPackageFileSize(MAX_MCP_PACKAGE_FILE_BYTES));
  assert.throws(() => assertMcpPackageFileSize(MAX_MCP_PACKAGE_FILE_BYTES + 1), /不能超过 64 MiB/);
  assert.throws(() => assertMcpPackageFileSize(-1), /大小无效/);
  assert.throws(() => assertMcpPackageFileSize(Number.POSITIVE_INFINITY), /大小无效/);
});

test("encodes binary MCP packages without one archive-sized intermediate string", () => {
  assert.equal(encodeMcpPackageBytes(new Uint8Array()), "");
  assert.equal(encodeMcpPackageBytes(new Uint8Array([0, 1, 2, 253, 254, 255])), "AAEC/f7/");

  const bytes = Uint8Array.from({ length: 0x6000 + 5 }, (_, index) => index % 256);
  assert.equal(encodeMcpPackageBytes(bytes), Buffer.from(bytes).toString("base64"));
});
