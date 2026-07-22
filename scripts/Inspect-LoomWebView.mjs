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

async function readJson(url) {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`CDP endpoint returned HTTP ${response.status}: ${url}`);
  return await response.json();
}

function waitForSocketOpen(socket) {
  return new Promise((resolve, reject) => {
    socket.addEventListener("open", resolve, { once: true });
    socket.addEventListener("error", () => reject(new Error("WebView CDP WebSocket failed to open")), { once: true });
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
      if (message.error) pending.reject(new Error(message.error.message || "CDP command failed"));
      else pending.resolve(message.result);
    };
    this.socket.addEventListener("message", this.messageHandler);
  }

  async open() {
    await waitForSocketOpen(this.socket);
  }

  command(method, params = {}) {
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.socket.send(JSON.stringify({ id, method, params }));
    });
  }

  async evaluate(expression) {
    const result = await this.command("Runtime.evaluate", {
      expression,
      awaitPromise: true,
      returnByValue: true,
      userGesture: true,
    });
    if (result.exceptionDetails) {
      throw new Error(result.exceptionDetails.text || "Browser evaluation failed");
    }
    return result.result?.value;
  }

  close() {
    for (const pending of this.pending.values()) pending.reject(new Error("CDP connection closed"));
    this.pending.clear();
    this.socket.close();
  }
}

async function waitFor(client, expression, timeoutMs = 15000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (await client.evaluate(expression)) return;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`Timed out waiting for browser condition: ${expression}`);
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const debugPort = Number.parseInt(args["debug-port"], 10);
  if (!Number.isInteger(debugPort) || debugPort <= 0) throw new Error("Invalid CDP debug port");
  const minimumNodes = Number.parseInt(args["min-nodes"] ?? "1", 10);
  if (!Number.isInteger(minimumNodes) || minimumNodes < 0) throw new Error("Invalid minimum node count");

  const targets = await readJson(`http://127.0.0.1:${debugPort}/json/list`);
  const target = targets.find((item) => String(item.url || "").includes("tauri.localhost"));
  if (!target?.webSocketDebuggerUrl) throw new Error("Could not find the Loom WebView2 target");

  const client = new CdpClient(target.webSocketDebuggerUrl);
  let diagnostic = null;
  try {
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

    const beforeOpen = await client.evaluate(`(() => {
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
      };
    })()`);
    const clickResult = await client.evaluate(`(() => {
      const button = document.querySelector('[data-testid="hook-canvas-thumbnail"] .hook-canvas-thumbnail__actions .signal-button');
      if (!button) return { found: false };
      button.click();
      return { found: true, disabled: Boolean(button.disabled), text: button.textContent?.trim() ?? '' };
    })()`);
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
    diagnostic = { beforeOpen, clickResult, tauriProbe };
    await fs.mkdir(path.dirname(args.output), { recursive: true });
    await fs.writeFile(args.output, `${JSON.stringify(diagnostic, null, 2)}\n`, "utf8");

    await waitFor(client, "Boolean(document.querySelector('[data-testid=\"hook-canvas-view\"]'))");
    const afterOpen = await client.evaluate(`(() => ({
      fullCanvasVisible: Boolean(document.querySelector('[data-testid="hook-canvas-view"]')),
      selectedNodeCount: document.querySelectorAll('[data-testid="hook-canvas-view"] .hook-canvas-node--selected').length,
    }))()`);
    const screenshot = await client.command("Page.captureScreenshot", { format: "png", fromSurface: true });

    const result = {
      ...beforeOpen,
      ...afterOpen,
      selectedNodeCount: afterOpen.selectedNodeCount ?? 0,
    };
    await fs.mkdir(path.dirname(args.screenshot), { recursive: true });
    await fs.writeFile(args.output, `${JSON.stringify(result, null, 2)}\n`, "utf8");
    await fs.writeFile(args.screenshot, Buffer.from(screenshot.data, "base64"));
    process.stdout.write(`${JSON.stringify(result)}\n`);
  } catch (error) {
    if (diagnostic) {
      try {
        diagnostic.afterFailure = await client.evaluate(`(() => ({
          bodyText: document.body.innerText.slice(0, 1600),
          activeHookView: Boolean(document.querySelector('[data-testid="hook-canvas-view"]')),
          activeSection: document.querySelector('.workspace-header h1')?.textContent?.trim() ?? null,
          workflowRequestText: document.querySelector('.workflow-studio')?.innerText?.slice(0, 500) ?? null,
        }))()`);
        await fs.writeFile(args.output, `${JSON.stringify({ ...diagnostic, error: String(error) }, null, 2)}\n`, "utf8");
      } catch {
        // Preserve the original CDP failure when the target has already closed.
      }
    }
    throw error;
  } finally {
    client.close();
  }
}

main().catch((error) => {
  process.stderr.write(`${error.stack || error}\n`);
  process.exitCode = 1;
});
