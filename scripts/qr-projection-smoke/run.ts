import { existsSync, mkdirSync, mkdtempSync, realpathSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { parseArgs } from "node:util";
import { candidate } from "./candidate.ts";
import { ProjectionClient } from "./protocol.ts";
import { ProjectionRuntime, textField } from "./runtime.ts";
import { runScenarios } from "./scenarios.ts";

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
