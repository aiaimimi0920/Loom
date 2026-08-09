import fs from "node:fs/promises";
import path from "node:path";

function parseArgs(argv) {
  const values = {};
  for (let index = 0; index < argv.length; index += 1) {
    const key = argv[index];
    if (!key.startsWith("--")) continue;
    const value = argv[index + 1];
    if (!value || value.startsWith("--")) throw new Error(`Missing value for ${key}`);
    values[key.slice(2)] = value;
    index += 1;
  }
  if (!values["debug-port"] || !values.output || !values.screenshot) {
    throw new Error("Usage: node Inspect-LoomWebView.mjs --debug-port <port> --output <json> --screenshot <png> [--min-nodes <count>]");
  }
  return values;
}

async function readJson(url, timeoutMs = 10000) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  try {
    const response = await fetch(url, { signal: controller.signal });
    if (!response.ok) throw new Error(`CDP endpoint returned HTTP ${response.status}: ${url}`);
    return await response.json();
  } finally {
    clearTimeout(timer);
  }
}

function waitForSocketOpen(socket, timeoutMs = 10000) {
  return new Promise((resolve, reject) => {
    const cleanup = () => {
      clearTimeout(timer);
      socket.removeEventListener("open", handleOpen);
      socket.removeEventListener("error", handleError);
    };
    const handleOpen = () => {
      cleanup();
      resolve();
    };
    const handleError = () => {
      cleanup();
      reject(new Error("WebView CDP WebSocket failed to open"));
    };
    const timer = setTimeout(() => {
      cleanup();
      reject(new Error("Timed out opening the WebView CDP WebSocket"));
    }, timeoutMs);
    socket.addEventListener("open", handleOpen, { once: true });
    socket.addEventListener("error", handleError, { once: true });
  });
}

class CdpClient {
  constructor(url) {
    this.socket = new WebSocket(url);
    this.nextId = 1;
    this.pending = new Map();
    this.messageHandler = (event) => {
      const message = JSON.parse(String(event.data));
      if (!message.id) return;
      const pending = this.pending.get(message.id);
      if (!pending) return;
      this.pending.delete(message.id);
      clearTimeout(pending.timer);
      if (message.error) pending.reject(new Error(message.error.message || "CDP command failed"));
      else pending.resolve(message.result);
    };
    this.socket.addEventListener("message", this.messageHandler);
  }

  async open() {
    await waitForSocketOpen(this.socket);
  }

  command(method, params = {}, timeoutMs = 10000) {
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        if (!this.pending.delete(id)) return;
        reject(new Error(`Timed out waiting for CDP command: ${method}`));
      }, timeoutMs);
      this.pending.set(id, { resolve, reject, timer });
      try {
        this.socket.send(JSON.stringify({ id, method, params }));
      } catch (error) {
        clearTimeout(timer);
        this.pending.delete(id);
        reject(error);
      }
    });
  }

  async evaluate(expression, timeoutMs = 10000) {
    const result = await this.command("Runtime.evaluate", {
      expression,
      awaitPromise: true,
      returnByValue: true,
      userGesture: true,
    }, timeoutMs);
    if (result.exceptionDetails) {
      throw new Error(result.exceptionDetails.text || "Browser evaluation failed");
    }
    return result.result?.value;
  }

  close() {
    for (const pending of this.pending.values()) {
      clearTimeout(pending.timer);
      pending.reject(new Error("CDP connection closed"));
    }
    this.pending.clear();
    this.socket.removeEventListener("message", this.messageHandler);
    if (this.socket.readyState === WebSocket.OPEN) {
      try {
        this.socket.close();
      } catch {
        // The Inspector result must not be replaced by a close-handshake error.
      }
    }
  }
}

async function waitFor(client, expression, timeoutMs = 15000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const remainingMs = Math.max(1, deadline - Date.now());
    if (await client.evaluate(expression, Math.min(10000, remainingMs))) return;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`Timed out waiting for browser condition: ${expression}`);
}

async function readNodePresentations(client, rootSelector) {
  return await client.evaluate(`(() => {
    const root = document.querySelector(${JSON.stringify(rootSelector)});
    if (!root) return [];
    return Array.from(root.querySelectorAll('[data-testid="hook-canvas-node"]')).map((node) => ({
      nodeId: node.getAttribute('data-node-id') ?? '',
      text: node.textContent?.trim() ?? '',
      placeholderText: node.querySelector('.hook-canvas-node__placeholder')?.textContent?.trim() ?? null,
      placeholderDetailText: node.querySelector('.hook-canvas-node__placeholder-detail')?.textContent?.trim() ?? null,
      placeholderClassName: node.querySelector('.hook-canvas-node__placeholder')?.className ?? null,
      className: node.className,
      hasImage: Boolean(node.querySelector('img')),
      imageCount: node.querySelectorAll('img').length,
    }));
  })()`);
}

async function readFailedArtExecutionFailureVisible(client, rootSelector) {
  return await client.evaluate(`(() => {
    const root = document.querySelector(${JSON.stringify(rootSelector)});
    if (!root) return false;
    const node = root.querySelector('[data-testid="hook-canvas-node"][data-node-id="failed-art"]');
    if (!node) return false;
    const placeholder = node.querySelector('.hook-canvas-node__placeholder');
    const placeholderTitle = node.querySelector('.hook-canvas-node__placeholder-title')?.textContent?.trim() ?? '';
    return Boolean(placeholder)
      && placeholderTitle === "执行失败"
      && placeholder.className.includes('hook-canvas-node__placeholder--error')
      && node.className.includes('hook-canvas-node--status-error')
      && !node.querySelector('img');
  })()`);
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const debugPort = Number.parseInt(args["debug-port"], 10);
  if (!Number.isInteger(debugPort) || debugPort <= 0) throw new Error("Invalid CDP debug port");
  const minimumNodes = Number.parseInt(args["min-nodes"] ?? "1", 10);
  if (!Number.isInteger(minimumNodes) || minimumNodes < 0) throw new Error("Invalid minimum node count");

  let client = null;
  let diagnostic = {};
  try {
    const targets = await readJson(`http://127.0.0.1:${debugPort}/json/list`);
    const target = targets.find((item) => String(item.url || "").includes("tauri.localhost"));
    if (!target?.webSocketDebuggerUrl) throw new Error("Could not find the Loom WebView2 target");
    client = new CdpClient(target.webSocketDebuggerUrl);
    await client.open();
    await client.command("Runtime.enable");
    await client.command("Page.enable");
    await client.evaluate("document.readyState === 'loading' ? new Promise(resolve => window.addEventListener('load', resolve, { once: true })) : true");
    await client.evaluate("document.querySelector('[data-testid=\"nav-hook-bridge\"]')?.click(); true");
    await waitFor(client, `(() => {
      const thumbnail = document.querySelector('[data-testid="hook-canvas-thumbnail"]');
      const revision = thumbnail?.getAttribute('data-revision') ?? 'empty';
      const nodeCount = thumbnail?.querySelectorAll('[data-testid="hook-canvas-node"]').length ?? 0;
      return Boolean(thumbnail) && revision !== 'empty' && nodeCount >= ${minimumNodes};
    })()`);

    const thumbnailNodes = await readNodePresentations(client, '[data-testid="hook-canvas-thumbnail"]');
    const failedArtThumbnailFailureVisible = await readFailedArtExecutionFailureVisible(
      client,
      '[data-testid="hook-canvas-thumbnail"]',
    );
    const uiState = await client.evaluate(`(() => {
      const thumbnail = document.querySelector('[data-testid="hook-canvas-thumbnail"]');
      const yaml = document.querySelector('.studio-textarea--yaml');
      const visible = yaml && (yaml.offsetWidth || yaml.offsetHeight || yaml.getClientRects().length);
      return {
        thumbnailVisible: Boolean(thumbnail),
        thumbnailNodeCount: thumbnail?.querySelectorAll('[data-testid="hook-canvas-node"]').length ?? 0,
        thumbnailEdgeCount: thumbnail?.querySelectorAll('svg line').length ?? 0,
        revision: thumbnail?.getAttribute('data-revision') ?? null,
        yamlVisible: Boolean(visible),
        advancedOpen: Array.from(document.querySelectorAll('details')).some((item) => item.open),
        offlineTextVisible: document.body.innerText.includes('本地服务离线'),
        visibleText: document.body.innerText.slice(0, 1200),
        thumbnailNodes: [],
        failedArtThumbnailFailureVisible: false,
      };
    })()`);
    uiState.thumbnailNodes = thumbnailNodes;
    uiState.failedArtThumbnailFailureVisible = failedArtThumbnailFailureVisible;
    const tauriProbe = await client.evaluate(`(async () => {
      try {
        const baseUrl = document.querySelector('.daemon-chip')?.textContent?.trim() ?? '';
        const value = await window.__TAURI_INTERNALS__?.invoke?.('get_loom_daemon_json', {
          baseUrl,
          path: '/v1/hook-bridge/canvas',
        });
        return { ok: true, value };
      } catch (error) {
        return { ok: false, error: String(error), stack: error?.stack ?? null };
      }
    })()`);
    diagnostic = { uiState, tauriProbe };
    const screenshot = await client.command("Page.captureScreenshot", { format: "png", fromSurface: true });
    const result = uiState;
    await fs.mkdir(path.dirname(args.screenshot), { recursive: true });
    await fs.writeFile(args.output, `${JSON.stringify(result, null, 2)}\n`, "utf8");
    await fs.writeFile(args.screenshot, Buffer.from(screenshot.data, "base64"));
    await new Promise((resolve, reject) => {
      process.stdout.write(`${JSON.stringify(result)}\n`, (error) => {
        if (error) reject(error);
        else resolve();
      });
    });
  } catch (error) {
    const failure = { ...diagnostic, error: String(error) };
    try {
      await fs.mkdir(path.dirname(args.output), { recursive: true });
      await fs.writeFile(args.output, `${JSON.stringify(failure, null, 2)}\n`, "utf8");
    } catch {
      // Preserve the original CDP failure when the output path is unavailable.
    }
    if (client) try {
      failure.afterFailure = await client.evaluate(`(() => ({
        bodyText: document.body.innerText.slice(0, 1600),
        activeHookView: Boolean(document.querySelector('[data-testid="hook-canvas-view"]')),
        activeSection: document.querySelector('.workspace-header h1')?.textContent?.trim() ?? null,
        workflowRequestText: document.querySelector('.workflow-studio')?.innerText?.slice(0, 500) ?? null,
      }))()`);
      await fs.writeFile(args.output, `${JSON.stringify(failure, null, 2)}\n`, "utf8");
    } catch {
      // Preserve the original CDP failure when the target has already closed.
    }
    throw error;
  } finally {
    if (client) client.close();
  }
}

main()
  .then(() => process.exit(0))
  .catch((error) => {
    const message = `${error.stack || error}\n`;
    const fallback = setTimeout(() => process.exit(1), 1000);
    process.stderr.write(message, () => {
      clearTimeout(fallback);
      process.exit(1);
    });
  });
