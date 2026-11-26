// Cross-platform Node.js stdio MCP entrypoint for neuro-image-search (Linux/Docker & universal).
import net from "node:net";
import readline from "node:readline";

const TOOL_NAME = "brave_image_search";
const MAX_RESPONSE_BYTES = 8 * 1024 * 1024;
const BRAVE_ENDPOINT = "https://api.search.brave.com/res/v1/images/search";
const OVERRIDE_ENV = "LOOM_IMAGE_SEARCH_ENDPOINT_OVERRIDE";

function isLoopbackUrl(rawUrl) {
  let parsed;
  try {
    parsed = new URL(rawUrl);
  } catch {
    return false;
  }
  if (parsed.protocol !== "http:" && parsed.protocol !== "https:") {
    return false;
  }
  if (parsed.username || parsed.password) {
    return false;
  }
  const host = parsed.hostname.replace(/^\[|\]$/g, "");
  if (!host) {
    return false;
  }
  if (host.toLowerCase() === "localhost" || host === "127.0.0.1" || host === "::1") {
    return true;
  }
  if (net.isIPv4(host)) {
    return host.startsWith("127.");
  }
  return false;
}

function resolveSearchEndpoint() {
  if (process.argv.slice(2).some((arg) => arg.toLowerCase() === "-endpoint" || arg.toLowerCase() === "--endpoint")) {
    throw new Error(`The image-search endpoint is fixed; set ${OVERRIDE_ENV} to a loopback URL for offline tests.`);
  }
  const override = (process.env[OVERRIDE_ENV] || "").trim();
  if (!override) {
    return BRAVE_ENDPOINT;
  }
  if (!isLoopbackUrl(override)) {
    throw new Error(`${OVERRIDE_ENV} must be an unauthenticated loopback http(s) URL`);
  }
  return new URL(override).toString();
}

let searchEndpoint;
try {
  searchEndpoint = resolveSearchEndpoint();
} catch (error) {
  process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
  process.exit(1);
}

function toHttpUrl(value) {
  if (typeof value !== "string" || !value.trim()) {
    return null;
  }
  try {
    const parsed = new URL(value.trim());
    return parsed.protocol === "http:" || parsed.protocol === "https:" ? parsed.toString() : null;
  } catch {
    return null;
  }
}

function toOptionalPositiveInt(value) {
  if (value === null || value === undefined) {
    return null;
  }
  const num = Number.parseInt(String(value), 10);
  return Number.isInteger(num) && num > 0 ? num : null;
}

function toCandidate(result) {
  if (!result || typeof result !== "object") {
    return null;
  }
  const properties = result.properties && typeof result.properties === "object" ? result.properties : {};
  const thumbnail = result.thumbnail && typeof result.thumbnail === "object" ? result.thumbnail : {};
  const thumbnailUrl = toHttpUrl(thumbnail.src);
  const imageUrl = toHttpUrl(result.url) || toHttpUrl(properties.url) || thumbnailUrl;
  if (!imageUrl) {
    return null;
  }
  return {
    imageUrl,
    thumbnailUrl,
    title: typeof result.title === "string" ? result.title.trim() : "",
    sourcePageUrl: toHttpUrl(result.source),
    width: toOptionalPositiveInt(properties.width),
    height: toOptionalPositiveInt(properties.height),
  };
}

async function invokeBraveImageSearch(query, count) {
  const apiKey = (process.env.BRAVE_API_KEY || "").trim();
  if (!apiKey) {
    throw new Error("BRAVE_API_KEY is required");
  }
  const target = new URL(searchEndpoint);
  target.searchParams.set("q", query);
  target.searchParams.set("count", String(count));
  target.searchParams.set("safesearch", "strict");

  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), 45_000);
  try {
    const response = await fetch(target, {
      method: "GET",
      headers: {
        Accept: "application/json",
        "X-Subscription-Token": apiKey,
      },
      redirect: "error",
      signal: controller.signal,
    });
    if (!response.ok) {
      throw new Error(`Brave image search returned HTTP ${response.status}`);
    }
    const reader = response.body?.getReader();
    if (!reader) {
      throw new Error("Brave image search returned an empty body");
    }
    const chunks = [];
    let total = 0;
    while (true) {
      const { done, value } = await reader.read();
      if (done) {
        break;
      }
      total += value.byteLength;
      if (total > MAX_RESPONSE_BYTES) {
        await reader.cancel();
        throw new Error("Brave image search response exceeds 8 MiB");
      }
      chunks.push(value);
    }
    const merged = Buffer.concat(chunks, total).toString("utf8");
    return JSON.parse(merged);
  } finally {
    clearTimeout(timer);
  }
}

async function handleToolsCall(params) {
  if (!params || typeof params !== "object") {
    throw new Error("tools/call params are required");
  }
  if (params.name !== TOOL_NAME) {
    throw new Error(`unknown MCP tool: ${String(params.name || "")}`);
  }
  const args = params.arguments && typeof params.arguments === "object" ? params.arguments : {};
  for (const key of Object.keys(args)) {
    if (key !== "query" && key !== "count") {
      throw new Error(`unknown image-search argument: ${key}`);
    }
  }
  const query = typeof args.query === "string" ? args.query.trim() : "";
  if (!query) {
    throw new Error("query is required");
  }
  if (query.length > 512) {
    throw new Error("query exceeds 512 characters");
  }
  let count = 3;
  if (args.count !== undefined && args.count !== null) {
    const parsedCount = Number(args.count);
    if (!Number.isInteger(parsedCount)) {
      throw new Error("count must be an integer");
    }
    if (parsedCount < 1 || parsedCount > 6) {
      throw new Error("count must be between 1 and 6");
    }
    count = parsedCount;
  }
  const payload = await invokeBraveImageSearch(query, count);
  const rawResults = Array.isArray(payload?.results) ? payload.results : [];
  const candidates = [];
  for (const item of rawResults) {
    const candidate = toCandidate(item);
    if (candidate) {
      candidates.push(candidate);
    }
    if (candidates.length >= count) {
      break;
    }
  }
  if (candidates.length === 0) {
    throw new Error("Brave image search returned no usable image candidates");
  }
  const structured = { query, count: candidates.length, candidates };
  return {
    content: [{ type: "text", text: JSON.stringify(structured) }],
    structuredContent: structured,
    isError: false,
  };
}

function writeMessage(msg) {
  process.stdout.write(`${JSON.stringify(msg)}\n`);
}

const rl = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
for await (const rawLine of rl) {
  const line = rawLine.replace(/^\uFEFF/, "").trim();
  if (!line) {
    continue;
  }
  let message;
  try {
    message = JSON.parse(line);
  } catch {
    writeMessage({ jsonrpc: "2.0", id: null, error: { code: -32700, message: "invalid JSON-RPC payload" } });
    continue;
  }
  const id = message?.id ?? null;
  const method = typeof message?.method === "string" ? message.method : "";
  if (message?.jsonrpc !== "2.0" || !method) {
    writeMessage({ jsonrpc: "2.0", id, error: { code: -32600, message: "invalid JSON-RPC request" } });
    continue;
  }
  if (method === "notifications/initialized") {
    continue;
  }
  try {
    if (method === "initialize") {
      writeMessage({
        jsonrpc: "2.0",
        id,
        result: {
          protocolVersion: "2024-11-05",
          capabilities: { tools: { listChanged: false } },
          serverInfo: { name: "neuro-image-search-mcp", version: "1.0.0" },
        },
      });
    } else if (method === "tools/list") {
      writeMessage({
        jsonrpc: "2.0",
        id,
        result: {
          tools: [
            {
              name: TOOL_NAME,
              description: "Searches Brave for image candidates used by the Loom image-search Art.",
              inputSchema: {
                type: "object",
                properties: {
                  query: { type: "string", minLength: 1, maxLength: 512 },
                  count: { type: "integer", minimum: 1, maximum: 6, default: 3 },
                },
                required: ["query"],
                additionalProperties: false,
              },
            },
          ],
        },
      });
    } else if (method === "tools/call") {
      const result = await handleToolsCall(message.params);
      writeMessage({ jsonrpc: "2.0", id, result });
    } else {
      writeMessage({ jsonrpc: "2.0", id, error: { code: -32601, message: `method not found: ${method}` } });
    }
  } catch (error) {
    writeMessage({
      jsonrpc: "2.0",
      id,
      error: { code: -32000, message: error instanceof Error ? error.message : String(error) },
    });
  }
}
