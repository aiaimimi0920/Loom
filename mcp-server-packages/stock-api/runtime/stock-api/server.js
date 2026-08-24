"use strict";

// Owns MCP request routing and bounded newline-delimited JSON-RPC framing.
const {
  MARKET_SERIES_TOOL,
  MAX_BUFFERED_INPUT_BYTES,
  MAX_IN_FLIGHT_REQUESTS,
  MAX_PENDING_REQUESTS,
  MAX_REQUEST_BYTES,
  ORDER_BOOK_TOOL,
  WRAPPER_VERSION,
} = require("./constants.js");
const {
  asObject,
  createToolResult,
  marketSeriesError,
} = require("./helpers.js");
const {
  executeMarketQuote,
  executeMarketSeries,
  executeOrderBook,
} = require("./executors.js");

async function handleWrapperRequest(request, upstreamHandle) {
  if (!request || typeof request !== "object") return upstreamHandle(request);
  if (request.method === "initialize" && request.id !== undefined) {
    const response = await upstreamHandle(request);
    if (response?.result?.serverInfo) response.result.serverInfo.version = WRAPPER_VERSION;
    return response;
  }
  if (request.method === "tools/list" && request.id !== undefined) {
    const response = await upstreamHandle(request);
    if (response?.result?.tools) {
      for (const tool of [MARKET_SERIES_TOOL, ORDER_BOOK_TOOL]) {
        if (!response.result.tools.some((existing) => existing.name === tool.name)) {
          response.result.tools.push(tool);
        }
      }
    }
    return response;
  }
  if (request.method === "tools/call" && request.id !== undefined) {
    const params = asObject(request.params);
    if (params.name === "get_stock") {
      const args = asObject(params.arguments);
      if (args.source === "eastmoney") {
        let result;
        try {
          result = createToolResult(await executeMarketQuote(args));
        } catch (error) {
          result = marketSeriesError(args, error, "get_stock");
        }
        return { jsonrpc: "2.0", id: request.id, result };
      }
    }
    if (params.name === ORDER_BOOK_TOOL.name) {
      const args = asObject(params.arguments);
      let result;
      try {
        result = createToolResult(await executeOrderBook(args));
      } catch (error) {
        result = marketSeriesError(args, error, params.name);
      }
      return { jsonrpc: "2.0", id: request.id, result };
    }
    if (
      params.name === MARKET_SERIES_TOOL.name
      || (params.name === "get_klines" && params.arguments?.source === "eastmoney")
    ) {
      const args = asObject(params.arguments);
      let result;
      try {
        result = createToolResult(await executeMarketSeries(args, upstreamHandle));
      } catch (error) {
        result = marketSeriesError(args, error, params.name);
      }
      return { jsonrpc: "2.0", id: request.id, result };
    }
  }
  return upstreamHandle(request);
}

async function runWrapperServer(input, output, upstreamHandle) {
  input.setEncoding("utf8");
  let buffer = "";
  let bufferBytes = 0;
  let activeRequests = 0;
  let inputPaused = false;
  let inputFailed = false;
  const pendingLines = [];
  // Set when an oversized line is rejected before its terminating newline has arrived. Every byte
  // up to that newline still belongs to the message that was already answered, so it is dropped
  // instead of parsed: treating the tail as a fresh line would answer a fragment, and from then on
  // every response would belong to the wrong request — the framing never recovers on its own.
  let discardingUntilNewline = false;
  const rejectOversizedRequest = () => {
    output.write(`${JSON.stringify({
      jsonrpc: "2.0",
      id: null,
      error: { code: -32600, message: `JSON-RPC request exceeds ${MAX_REQUEST_BYTES} bytes` },
    })}\n`);
  };
  const rejectOverloadedInput = () => {
    output.write(`${JSON.stringify({
      jsonrpc: "2.0",
      id: null,
      error: { code: -32000, message: "JSON-RPC input backlog limit exceeded" },
    })}\n`);
  };

  const processLine = async (line) => {
    let request;
    try {
      request = JSON.parse(line);
      const response = await handleWrapperRequest(request, upstreamHandle);
      if (response) output.write(`${JSON.stringify(response)}\n`);
    } catch (error) {
      output.write(`${JSON.stringify({
        jsonrpc: "2.0",
        id: request?.id ?? null,
        error: { code: -32603, message: error instanceof Error ? error.message : String(error) },
      })}\n`);
    }
  };

  const schedule = () => {
    while (activeRequests < MAX_IN_FLIGHT_REQUESTS && pendingLines.length > 0) {
      const line = pendingLines.shift();
      activeRequests += 1;
      void processLine(line).finally(() => {
        activeRequests -= 1;
        drainBuffer();
        schedule();
      });
    }
  };

  const updateInputBackpressure = () => {
    if (inputFailed) return;
    const shouldPause = pendingLines.length >= MAX_PENDING_REQUESTS
      || (activeRequests >= MAX_IN_FLIGHT_REQUESTS && buffer.indexOf("\n") >= 0);
    if (shouldPause && !inputPaused) {
      input.pause();
      inputPaused = true;
    } else if (!shouldPause && inputPaused) {
      inputPaused = false;
      input.resume();
    }
  };

  const drainBuffer = () => {
    if (discardingUntilNewline) {
      const boundary = buffer.indexOf("\n");
      if (boundary < 0) {
        // Still inside the rejected message: drop what arrived and keep waiting for the boundary,
        // so a client that never stops writing cannot grow the buffer either.
        buffer = "";
        bufferBytes = 0;
        updateInputBackpressure();
        return;
      }
      buffer = buffer.slice(boundary + 1);
      bufferBytes = Buffer.byteLength(buffer, "utf8");
      discardingUntilNewline = false;
    }
    if (buffer.indexOf("\n") < 0 && bufferBytes > MAX_REQUEST_BYTES) {
      buffer = "";
      bufferBytes = 0;
      discardingUntilNewline = true;
      rejectOversizedRequest();
      updateInputBackpressure();
      return;
    }
    let newlineIndex = buffer.indexOf("\n");
    while (newlineIndex >= 0 && pendingLines.length < MAX_PENDING_REQUESTS) {
      const rawLine = buffer.slice(0, newlineIndex);
      buffer = buffer.slice(newlineIndex + 1);
      const rawLineBytes = Buffer.byteLength(rawLine, "utf8");
      bufferBytes -= rawLineBytes + 1;
      if (rawLineBytes > MAX_REQUEST_BYTES) {
        // This line is already complete, so the same invalid-request answer needs no discard state.
        rejectOversizedRequest();
        newlineIndex = buffer.indexOf("\n");
        continue;
      }
      const line = rawLine.trim();
      if (line) pendingLines.push(line);
      newlineIndex = buffer.indexOf("\n");
    }
    schedule();
    updateInputBackpressure();
  };

  input.on("data", (chunk) => {
    if (inputFailed) return;
    if (discardingUntilNewline) {
      const boundary = chunk.indexOf("\n");
      if (boundary < 0) return;
      chunk = chunk.slice(boundary + 1);
      discardingUntilNewline = false;
    }
    if (!chunk) return;
    const chunkBytes = Buffer.byteLength(chunk, "utf8");
    if (bufferBytes + chunkBytes > MAX_BUFFERED_INPUT_BYTES) {
      // pause() cannot retract a data event already delivered by a custom or malicious stream. Drop
      // through the last complete frame instead of retaining an attacker-sized chunk behind the queue.
      const bufferedBoundary = buffer.indexOf("\n");
      const chunkBoundary = chunk.indexOf("\n");
      const firstFrameBytes = bufferedBoundary >= 0
        ? Buffer.byteLength(buffer.slice(0, bufferedBoundary), "utf8")
        : bufferBytes + Buffer.byteLength(
          chunkBoundary >= 0 ? chunk.slice(0, chunkBoundary) : chunk,
          "utf8",
        );
      const lastBoundary = chunk.lastIndexOf("\n");
      buffer = "";
      bufferBytes = 0;
      if (lastBoundary < 0) {
        discardingUntilNewline = true;
      } else {
        const tail = chunk.slice(lastBoundary + 1);
        const tailBytes = Buffer.byteLength(tail, "utf8");
        if (tailBytes > MAX_REQUEST_BYTES) {
          discardingUntilNewline = true;
        } else {
          buffer = tail;
          bufferBytes = tailBytes;
        }
      }
      if (firstFrameBytes > MAX_REQUEST_BYTES) rejectOversizedRequest();
      else rejectOverloadedInput();
      updateInputBackpressure();
      return;
    }
    buffer += chunk;
    bufferBytes += chunkBytes;
    drainBuffer();
  });
  input.on("error", () => {
    // Stream errors are terminal for framing. Clear retained requests and keep the EventEmitter error
    // handled so a broken stdin pipe does not become an unrelated uncaught process exception.
    inputFailed = true;
    buffer = "";
    bufferBytes = 0;
    pendingLines.length = 0;
  });
}

module.exports = Object.freeze({ handleWrapperRequest, runWrapperServer });
