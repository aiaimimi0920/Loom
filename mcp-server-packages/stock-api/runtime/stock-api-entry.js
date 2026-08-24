"use strict";

// Composes the stock-api wrapper without changing its public CommonJS or stdio entry contracts.
const {
  MAX_RESPONSE_BYTES,
} = require("./stock-api/constants.js");
const {
  BoundedResponseBuffer,
  configureLoopbackFixture,
  parseBoundedJsonResponse,
  readBoundedResponseBody,
} = require("./stock-api/transport.js");
const { runWrapperServer } = require("./stock-api/server.js");

module.exports = Object.freeze({
  BoundedResponseBuffer,
  MAX_RESPONSE_BYTES,
  parseBoundedJsonResponse,
  readBoundedResponseBody,
});

if (require.main === module) {
  try {
    configureLoopbackFixture();
    const { handleMcpRequest } = require("./vendor/stock-api/dist/mcp/server.js");
    void runWrapperServer(process.stdin, process.stdout, handleMcpRequest);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  }
}
