import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, statSync, writeFileSync } from "node:fs";
import { isAbsolute, join, relative, resolve } from "node:path";
import { parseArgs } from "node:util";
import { ProjectionClient } from "./protocol.ts";
import { object, ProjectionRuntime, textField } from "./runtime.ts";
import { runScenarios } from "./scenarios.ts";

function candidate(packageDir: string) {
    const manifestPath = join(packageDir, "manifest.json");
    assert.ok(statSync(manifestPath).size <= 2 * 1024 * 1024, "package manifest is too large");
    const manifest = object(JSON.parse(readFileSync(manifestPath, "utf8").replace(/^\uFEFF/, "")));
    assert.equal(manifest.app, "Loom", "candidate application");
    assert.ok(Array.isArray(manifest.exes), "candidate executable list missing");
    const entry = manifest.exes.map(object).find((exe) => exe.name === "loom-daemon.exe");
    assert.ok(entry, "daemon metadata missing");
    const executable = realpathSync(join(packageDir, "runtime", "loom-daemon.exe"));
    const within = relative(packageDir, executable);
    assert.ok(!isAbsolute(within) && !within.startsWith(".."), "daemon escaped package directory");
    const bytes = statSync(executable).size;
    assert.ok(bytes > 0 && bytes <= 512 * 1024 * 1024, "invalid daemon size");
    assert.equal(bytes, entry.bytes, "candidate daemon size mismatch");
    const sha256 = createHash("sha256").update(readFileSync(executable)).digest("hex");
    assert.equal(sha256, entry.sha256, "candidate daemon digest mismatch");
    return { executable, bytes, sha256, versionId: textField(manifest.versionId, "candidate version") };
}

async function main(): Promise<void> {
    const { values } = parseArgs({ options: { "package-dir": { type: "string" }, "evidence-root": { type: "string" } } });
    const packageDir = realpathSync(resolve(textField(values["package-dir"], "package directory")));
    const binary = candidate(packageDir);
    const evidenceRoot = resolve(textField(values["evidence-root"], "evidence root"));
    mkdirSync(evidenceRoot, { recursive: true });
    const evidence = mkdtempSync(join(realpathSync(evidenceRoot), "qr-projection-"));
    const controller = new AbortController();
    const runtime = new ProjectionRuntime(binary.executable, controller.signal);
    const checks: string[] = ["candidate_executable_digest_matches_manifest"];
    const startedAt = new Date().toISOString();
    const deadline = setTimeout(() => controller.abort(), 120_000);
    const interrupt = () => controller.abort();
    process.once("SIGINT", interrupt);
    process.once("SIGTERM", interrupt);
    let failure: string | undefined;
    try {
        await runtime.start();
        await runScenarios(new ProjectionClient(runtime), checks);
    } catch (error) {
        failure = error instanceof Error ? error.message : "projection smoke failed";
    } finally {
        clearTimeout(deadline);
        process.removeListener("SIGINT", interrupt);
        process.removeListener("SIGTERM", interrupt);
        try { await runtime.dispose(); }
        catch { failure = `${failure ?? ""} owned runtime cleanup failed`.trim(); }
    }
    const reportPath = join(evidence, "summary.json");
    writeFileSync(reportPath, JSON.stringify({
        schemaVersion: 1, status: failure ? "failed" : "passed", scope: "packaged-daemon-loopback",
        startedAt, finishedAt: new Date().toISOString(), packageDir, binary, checks,
        daemonPids: runtime.pids, temporaryRootRemoved: !existsSync(runtime.root),
        nativeHookWindowsTested: false, crossMachineHttpsTested: false, failure,
    }, null, 2) + "\n", { encoding: "utf8", flag: "wx" });
    console.log(`[loom-qr-projection-smoke] ${failure ? "Failed" : "Passed"}: ${reportPath}`);
    if (failure) throw new Error(failure);
}

main().catch((error: unknown) => {
    console.error(error instanceof Error ? error.message : "projection smoke failed");
    process.exitCode = 1;
});
