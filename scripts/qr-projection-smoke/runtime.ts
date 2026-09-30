import assert from "node:assert/strict";
import { spawn, type ChildProcess } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { basename, dirname, join } from "node:path";
import { setTimeout as delay } from "node:timers/promises";

export type JsonObject = Record<string, unknown>;

export function object(value: unknown): JsonObject {
    assert.ok(value !== null && typeof value === "object" && !Array.isArray(value), "expected JSON object");
    return value as JsonObject;
}

export function textField(value: unknown, label: string): string {
    assert.ok(typeof value === "string" && value.length > 0 && value.length <= 4096, `invalid ${label}`);
    return value;
}

async function freePort(): Promise<number> {
    const listener = createServer();
    await new Promise<void>((resolve, reject) => {
        listener.once("error", reject);
        listener.listen(0, "127.0.0.1", resolve);
    });
    const address = listener.address();
    assert.ok(address && typeof address === "object", "port allocation failed");
    await new Promise<void>((resolve, reject) => listener.close((error) => error ? reject(error) : resolve()));
    return address.port;
}

export class ProjectionRuntime {
    readonly root = realpathSync(mkdtempSync(join(tmpdir(), "loom-qr-projection-")));
    readonly pids: number[] = [];
    origin = "";
    private child: ChildProcess | undefined;
    private spawnFailed = false;

    readonly executable: string;
    readonly signal: AbortSignal;

    constructor(executable: string, signal: AbortSignal) {
        this.executable = executable;
        this.signal = signal;
    }

    get adminToken(): string {
        return readFileSync(join(this.root, "control", "daemon-token"), "utf8").trim();
    }

    async start(): Promise<void> {
        assert.equal(this.child, undefined, "daemon already owned");
        const port = this.origin ? Number(new URL(this.origin).port) : await freePort();
        this.origin = `http://127.0.0.1:${port}`;
        const manifestRoot = join(this.root, "capabilities");
        mkdirSync(manifestRoot, { recursive: true });
        // Do not inherit the developer's Loom paths, credentials or integrations.
        const environment = Object.fromEntries(Object.entries(process.env).filter(([key]) => !/^LOOM_/i.test(key)));
        Object.assign(environment, {
            LOOM_DAEMON_HOST: "127.0.0.1",
            LOOM_DAEMON_PORT: String(port),
            LOOM_CONTROL_PLANE_ROOT: join(this.root, "control"),
            LOOM_CONFIGURATION_ROOT: join(this.root, "configuration"),
            LOOM_RUN_STORE_PATH: join(this.root, "runs.sqlite3"),
        });
        this.spawnFailed = false;
        this.child = spawn(this.executable, ["--manifest-dir", manifestRoot], {
            cwd: this.root, env: environment, windowsHide: true, stdio: "ignore",
        });
        this.child.once("error", () => { this.spawnFailed = true; });
        if (this.child.pid) this.pids.push(this.child.pid);
        const deadline = Date.now() + 20_000;
        while (Date.now() < deadline) {
            this.signal.throwIfAborted();
            assert.ok(!this.spawnFailed && this.child.exitCode === null, "candidate daemon exited before readiness");
            try {
                const response = await this.request("/health");
                if (response.status === 200 && response.body.status === "ok") {
                    assert.ok(this.adminToken.length > 0, "isolated administrator token missing");
                    return;
                }
            } catch {
                // A refused connection is expected until this child binds its port.
            }
            await delay(100, undefined, { signal: this.signal });
        }
        throw new Error("candidate daemon readiness timed out");
    }

    async request(path: string, body?: unknown, headers: Record<string, string> = {}, method = "POST") {
        assert.ok(path.startsWith("/") && !path.startsWith("//"), "request must use an owned route");
        const response = await fetch(this.origin + path, {
            method: body === undefined ? "GET" : method,
            headers: { "Content-Type": "application/json", ...headers },
            body: body === undefined ? undefined : JSON.stringify(body),
            redirect: "error",
            signal: AbortSignal.any([this.signal, AbortSignal.timeout(path.startsWith("/v1/projections/v2/") ? 40_000 : 5000)]),
        });
        const chunks: Uint8Array[] = [];
        let bytes = 0;
        const reader = response.body?.getReader();
        if (reader) {
            try {
                while (true) {
                    const item = await reader.read();
                    if (item.done) break;
                    bytes += item.value.length;
                    assert.ok(bytes <= 6 * 1024 * 1024, "response exceeded projection budget");
                    chunks.push(item.value);
                }
            } finally {
                await reader.cancel();
                reader.releaseLock();
            }
        }
        return { status: response.status, body: object(JSON.parse(Buffer.concat(chunks).toString("utf8"))) };
    }

    async stop(): Promise<void> {
        const child = this.child;
        if (!child) return;
        if (child.exitCode === null && child.signalCode === null && !this.spawnFailed) {
            // Termination deliberately exercises recovery after process loss.
            assert.ok(child.kill(), "could not stop owned daemon");
            const deadline = Date.now() + 5000;
            while (child.exitCode === null && child.signalCode === null && Date.now() < deadline) await delay(25);
            assert.ok(child.exitCode !== null || child.signalCode !== null, "owned daemon did not exit");
        }
        this.child = undefined;
    }

    async dispose(): Promise<void> {
        await this.stop();
        // Only this freshly allocated, canonical temporary root may be deleted.
        assert.equal(dirname(this.root), realpathSync(tmpdir()), "temporary root escaped its parent");
        assert.ok(basename(this.root).startsWith("loom-qr-projection-"), "unexpected temporary root");
        assert.equal(realpathSync(this.root), this.root, "temporary root changed identity");
        rmSync(this.root, { recursive: true, maxRetries: 5, retryDelay: 100 });
    }
}
